#!/usr/bin/env python3
"""扮演 SideDesk 宿主的协议驱动：验证 sidecar/v1 链路。

跑法（Linux 原生产物）：

    cargo build && python3 tests/fake-host.py target/debug/game-buddy

剧本：握手 → 读配置 → 下发声明 → 开窗 → 提问 → 拦截 http.fetch 并回一份
OpenAI 兼容响应 → 断言最终状态里出现回答。任何一行 stdout 不是 JSON 都判失败
（宿主对 stdout 的纪律：只能有帧）。
"""

from __future__ import annotations

import json
import subprocess
import sys

PROTOCOL = "sidecar/v1"

CANNED_ANSWER = "先打断读条，再贴身绕后；半血会召唤小怪，优先清掉。"

# 假宿主提供的配置（store.get 的应答）
FAKE_CONFIG = {
    "baseUrl": "https://api.example.com/v1",
    "apiKey": "sk-test",
    "model": "test-model",
}


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

    def serve_until(self, stop, limit: int = 32):
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
        elif method == "ui.declare" or method == "event":
            # 通知帧没有 id，不回包（记账在 read 里完成）
            pass
        else:
            # 未知请求也要有终态，否则模块会一直等
            self.send(
                {
                    "jsonrpc": "2.0",
                    "id": frame["id"],
                    "error": {"code": -32601, "message": f"未实现 {method}"},
                }
            )


def is_declare(frame: dict) -> bool:
    """声明帧。"""
    return frame.get("method") == "ui.declare"


def is_state(frame: dict) -> bool:
    """状态帧。"""
    return frame.get("method") == "event" and frame.get("params", {}).get("topic") == "state"


def state_payload(frame: dict) -> dict:
    """取状态帧里的 payload（`{path}` 的绑定词表）。"""
    return frame["params"]["payload"]


def is_http(frame: dict) -> bool:
    """http.fetch 请求。"""
    return frame.get("method") == "http.fetch"


def is_response_to(target_id: str):
    def check(frame: dict) -> bool:
        """某个 id 的应答。"""
        return frame.get("id") == target_id and "method" not in frame

    return check


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
        assert decl["params"]["json"] and isinstance(decl["params"]["json"], str), "ui.declare 的 json 必须是字符串"
        state = state_payload(d.serve_until(is_state))
        assert state["t_title"], "状态里缺少标题文案"
        assert state["busy"] == "false", state["busy"]

        # 3) 宿主开窗
        d.send({"jsonrpc": "2.0", "method": "ui.lifecycle", "params": {"surface": "overlay", "event": "opened"}})
        d.serve_until(is_state)

        # 4) 用户提问
        d.send({"jsonrpc": "2.0", "id": "r1", "method": "ask", "params": {"value": "这个 Boss 怎么打？"}})
        ack = d.serve_until(is_response_to("r1"))
        assert "result" in ack, f"ask 应回 result：{ack}"

        busy_state = state_payload(d.serve_until(is_state))
        assert "正在检索" in busy_state["status"], f"应先落入忙状态：{busy_state['status']!r}"

        # 5) 模块经宿主 http.fetch 出网
        req = d.serve_until(is_http)
        hp = req["params"]
        assert hp["method"] == "POST", hp
        assert hp["headers"]["Authorization"].startswith("Bearer "), "必须带 Authorization 头"
        assert hp["headers"]["Content-Type"] == "application/json", hp["headers"]
        assert hp["url"].endswith("/chat/completions"), hp["url"]

        # 6) 回一份 OpenAI 兼容响应
        canned = {
            "choices": [{"message": {"role": "assistant", "content": CANNED_ANSWER}}],
        }
        d.reply_ok(req, {"status": 200, "contentType": "application/json", "body": json.dumps(canned, ensure_ascii=False)})

        # 7) 最终状态里应出现回答
        final = state_payload(d.serve_until(is_state))
        assert CANNED_ANSWER in final["answer"], f"回答未出现在状态里：{final['answer']!r}"

        # 7.5) 第二轮：上一轮的问答应作为上下文带进请求
        d.send({"jsonrpc": "2.0", "id": "r2", "method": "ask", "params": {"value": "那第二阶段呢？"}})
        d.serve_until(is_response_to("r2"))
        d.serve_until(is_state)  # 忙状态
        req2 = d.serve_until(is_http)
        sent = json.loads(req2["params"]["body"])
        roles = [m["role"] for m in sent["messages"]]
        assert roles == ["system", "user", "assistant", "user"], f"上下文顺序不对：{roles}"
        assert sent["messages"][1]["content"] == "这个 Boss 怎么打？", sent["messages"][1]
        assert sent["messages"][2]["content"] == CANNED_ANSWER, sent["messages"][2]
        assert sent["messages"][3]["content"] == "那第二阶段呢？", sent["messages"][3]
        d.reply_ok(
            req2,
            {"status": 200, "contentType": "application/json",
             "body": json.dumps({"choices": [{"message": {"content": "第二阶段先躲范围技。"}}]}, ensure_ascii=False)},
        )
        d.serve_until(is_state)

        # 7.6) 清空对话：历史与回答都归零
        d.send({"jsonrpc": "2.0", "id": "r3", "method": "clear", "params": None})
        d.serve_until(is_response_to("r3"))
        cleared = state_payload(d.serve_until(is_state))
        assert cleared["history"] == [], cleared["history"]
        assert cleared["answer"] == "", cleared["answer"]

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

    print(f"已通过! 声明 {len(d.declares)} 次、状态 {len(d.states)} 次、http.fetch {len(d.http_requests)} 次")
    return 0


if __name__ == "__main__":
    sys.exit(main())
