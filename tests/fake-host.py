#!/usr/bin/env python3
"""扮演 SideDesk 宿主的协议驱动：验证 sidecar/v1 链路。

跑法（Linux 原生产物）：

    cargo build && python3 tests/fake-host.py target/debug/game-buddy

剧本：握手 → 读配置 → 下发声明 → 开窗 → 提问（**先检索、再对话**）→ 断言系统提示词里
带上了检索到的资料、状态里有来源 → 第二轮带历史 → 快捷提问 → 清空 → 关窗。

任何一行 stdout 不是 JSON 都判失败（宿主对 stdout 的纪律：只能有帧）。
"""

from __future__ import annotations

import json
import subprocess
import sys

PROTOCOL = "sidecar/v1"

CANNED_ANSWER = "先打断读条，再贴身绕后；半血会召唤小怪，优先清掉。"
CANNED_SOURCE = "首领会先读条，打断后全团集火；半血召唤小怪优先清。"

# 假宿主提供的配置（store.get 的应答）
FAKE_CONFIG = {
    "baseUrl": "https://api.example.com/v1",
    "apiKey": "sk-test",
    "model": "test-model",
    # 值本身是 JSON 字符串；中间那个空槽用来验证「空槽隐藏按钮」
    "presets": json.dumps(["这个 BOSS 怎么打？", "", "下一步去哪？"], ensure_ascii=False),
    "ragUrl": "https://kb.example.com/wow-rag/v1/retrieve",
    "ragUser": "wow",
    "ragPass": "secret",
    "ragTopK": "3",
}

RETRIEVE_PATH = "/v1/retrieve"
CHAT_PATH = "/chat/completions"


class Failure(Exception):
    pass


class Driver:
    def __init__(self, exe: str) -> None:
        self.proc = subprocess.Popen(
            [exe],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            bufsize=1,
        )
        self.declares: list[dict] = []
        self.states: list[dict] = []
        self.http_requests: list[dict] = []

    # ---- 低层 ----

    def send(self, frame: dict) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(json.dumps(frame, ensure_ascii=False) + "\n")
        self.proc.stdin.flush()

    def read(self, expect: str = "任意帧") -> dict:
        assert self.proc.stdout is not None
        line = self.proc.stdout.readline()
        if line == "":
            raise Failure(f"期望{expect}，但子进程已关闭 stdout")
        try:
            frame = json.loads(line)
        except json.JSONDecodeError as exc:
            raise Failure(f"stdout 出现非 JSON 行（违反 stdout 纪律）：{line!r}") from exc
        # 记账放在读取处：匹配到目标的帧不会再经过 auto_reply
        if frame.get("method") == "ui.declare":
            self.declares.append(json.loads(frame["params"]["json"]))
        elif frame.get("method") == "event" and frame.get("params", {}).get("topic") == "state":
            self.states.append(frame["params"]["payload"])
        elif frame.get("method") == "http.fetch":
            self.http_requests.append(frame.get("params") or {})
        return frame

    def reply_ok(self, frame: dict, result: dict) -> None:
        self.send({"jsonrpc": "2.0", "id": frame["id"], "result": result})

    # ---- 宿主侧应答 ----

    def serve_until(self, stop, limit: int = 64):
        """循环读帧并自动应答宿主请求，直到 `stop(frame)` 为真。"""
        for _ in range(limit):
            frame = self.read(stop.__doc__ or "目标帧")
            if stop(frame):
                return frame
            self.auto_reply(frame)
        raise Failure("在预期步数内没有等到目标帧")

    def auto_reply(self, frame: dict) -> None:
        method = frame.get("method")
        if method is None:
            return  # 模块对我们请求的应答，交给调用方处理
        if method == "initialize":
            self.reply_ok(frame, {"protocolVersion": PROTOCOL})
        elif method == "store.get":
            self.reply_ok(frame, {"value": FAKE_CONFIG.get(frame["params"].get("key"))})
        elif method == "store.set":
            self.reply_ok(frame, {"saved": True})
        elif method == "log.write":
            self.reply_ok(frame, {"written": True})
        elif method in ("ui.declare", "event"):
            pass  # 通知帧没有 id，不回包（记账在 read 里完成）
        else:
            self.send(
                {
                    "jsonrpc": "2.0",
                    "id": frame["id"],
                    "error": {"code": -32601, "message": f"未实现 {method}"},
                }
            )

    # ---- 一步问答：检索 + 对话 ----

    def ask(self, req_id: str, question: str, answer: str) -> dict:
        """发一次提问，走完「检索 → 拼提示词 → 对话」，返回最终状态 payload。"""
        self.send({"jsonrpc": "2.0", "id": req_id, "method": "ask", "params": {"value": question}})
        self.serve_until(is_response_to(req_id))
        self.serve_until(is_state)  # 忙状态

        # 1) 先检索
        ret = self.serve_until(is_http_to(RETRIEVE_PATH))
        params = ret["params"]
        assert params["headers"]["Authorization"].startswith("Basic "), params["headers"]
        assert params["headers"]["Authorization"] != "Basic ", "Basic 凭据不能为空"
        query = json.loads(params["body"])
        assert query["query"] == question, query
        assert query["top_k"] == 3, query
        self.reply_ok(
            ret,
            {
                "status": 200,
                "contentType": "application/json",
                "body": json.dumps(
                    {
                        "count": 1,
                        "results": [
                            {
                                "title": "地下城",
                                "section": "血槌熔炉",
                                "site": "wx-wow",
                                "url": "https://x/1",
                                "text": CANNED_SOURCE,
                            }
                        ],
                    },
                    ensure_ascii=False,
                ),
            },
        )

        # 2) 再对话：系统提示词里必须带上刚检索到的资料
        chat = self.serve_until(is_http_to(CHAT_PATH))
        sent = json.loads(chat["params"]["body"])
        system = sent["messages"][0]["content"]
        assert "知识库资料" in system, system
        assert CANNED_SOURCE in system, "检索到的片段应拼进提示词"
        assert sent["messages"][-1]["content"] == question, sent["messages"][-1]

        self.reply_ok(
            chat,
            {
                "status": 200,
                "contentType": "application/json",
                "body": json.dumps({"choices": [{"message": {"content": answer}}]}, ensure_ascii=False),
            },
        )
        return state_payload(self.serve_until(is_state))


def is_declare(frame: dict) -> bool:
    """声明帧。"""
    return frame.get("method") == "ui.declare"


def is_state(frame: dict) -> bool:
    """状态帧。"""
    return frame.get("method") == "event" and frame.get("params", {}).get("topic") == "state"


def is_http_to(fragment: str):
    def check(frame: dict) -> bool:
        """发往指定路径的 http.fetch 请求。"""
        return frame.get("method") == "http.fetch" and fragment in (frame.get("params") or {}).get("url", "")

    return check


def is_response_to(target_id: str):
    def check(frame: dict) -> bool:
        """某个 id 的应答。"""
        return frame.get("id") == target_id and "method" not in frame

    return check


def state_payload(frame: dict) -> dict:
    """取状态帧里的 payload（`{path}` 的绑定词表）。"""
    return frame["params"]["payload"]


def main() -> int:
    exe = sys.argv[1] if len(sys.argv) > 1 else "target/debug/game-buddy"
    d = Driver(exe)

    try:
        # 1) 模块必须先发 initialize，且 id 是字符串
        init = d.read("模块先发的 initialize")
        assert init.get("method") == "initialize", f"第一帧不是 initialize：{init}"
        assert isinstance(init.get("id"), str), "initialize 的 id 必须是字符串"
        params = init["params"]
        assert params.get("protocolVersion") == PROTOCOL, params
        assert params.get("moduleId") == "game-buddy", params
        d.reply_ok(init, {"protocolVersion": PROTOCOL})

        # 2) 握手后读配置（store.get ×N）→ 声明 → 状态
        decl = d.serve_until(is_declare)
        assert isinstance(decl["params"]["json"], str), "ui.declare 的 json 必须是字符串"
        state = state_payload(d.serve_until(is_state))
        assert state["t_title"], "状态里缺少标题文案"
        assert state["has_sources"] == "false", "还没提问就不该有来源"

        # 3) 宿主开窗
        d.send({"jsonrpc": "2.0", "method": "ui.lifecycle", "params": {"surface": "overlay", "event": "opened"}})
        d.serve_until(is_state)

        # 4) 第一问：走检索 + 对话
        final = d.ask("r1", "这个 Boss 怎么打？", CANNED_ANSWER)
        assert CANNED_ANSWER in final["answer"], final["answer"]
        assert final["has_sources"] == "true", "回答后应带出来源"
        assert final["sources"][0]["label"] == "地下城 · 血槌熔炉", final["sources"]

        # 5) 第二问：历史要带进上下文
        final2 = d.ask("r2", "那第二阶段呢？", "第二阶段先躲范围技。")
        assert final2["history"][0]["text"] == "这个 Boss 怎么打？"
        assert len(final2["history"]) == 2, final2["history"]

        # 6) 清空对话：历史、回答、来源都归零
        d.send({"jsonrpc": "2.0", "id": "r3", "method": "clear", "params": None})
        d.serve_until(is_response_to("r3"))
        cleared = state_payload(d.serve_until(is_state))
        assert cleared["history"] == [], cleared["history"]
        assert cleared["answer"] == "", cleared["answer"]
        assert cleared["has_sources"] == "false", cleared["has_sources"]

        # 7) 快捷提问：点一下直接发问；空槽不该出按钮
        assert cleared["p0_on"] == "true", cleared["p0_on"]
        assert cleared["p1_on"] == "false", "空槽必须隐藏按钮"
        d.send({"jsonrpc": "2.0", "id": "r4", "method": "preset.0", "params": None})
        d.serve_until(is_response_to("r4"))
        d.serve_until(is_state)
        ret = d.serve_until(is_http_to(RETRIEVE_PATH))
        assert json.loads(ret["params"]["body"])["query"] == "这个 BOSS 怎么打？"
        d.reply_ok(ret, {"status": 200, "contentType": "application/json", "body": '{"results":[]}'})
        chat = d.serve_until(is_http_to(CHAT_PATH))
        sent = json.loads(chat["params"]["body"])
        assert len(sent["messages"]) == 2, "清空后不该再带历史"
        assert "知识库资料" not in sent["messages"][0]["content"], "没有检索结果时不该有资料块"
        d.reply_ok(
            chat,
            {"status": 200, "contentType": "application/json",
             "body": json.dumps({"choices": [{"message": {"content": "先拉扯再爆发。"}}]}, ensure_ascii=False)},
        )
        d.serve_until(is_state)

        # 8) 关窗 → 进程退出
        d.send({"jsonrpc": "2.0", "method": "ui.lifecycle", "params": {"surface": "overlay", "event": "closed"}})
        code = d.proc.wait(timeout=5)
        assert code == 0, f"关窗后应以 0 退出，实际 {code}"

    except Failure as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        stderr = d.proc.stderr.read() if d.proc.stderr else ""
        if stderr:
            print("--- 模块 stderr ---", file=sys.stderr)
            print(stderr, file=sys.stderr)
        d.proc.kill()
        return 1
    except Exception as exc:  # noqa: BLE001 - 驱动脚本，任何异常都判失败
        print(f"FAIL: {type(exc).__name__}: {exc}", file=sys.stderr)
        d.proc.kill()
        return 1

    print(
        f"已通过! 声明 {len(d.declares)} 次、状态 {len(d.states)} 次、http.fetch {len(d.http_requests)} 次"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
