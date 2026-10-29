# Game Buddy

悬浮在游戏窗口上的在线 RAG 游戏助手——问攻略、问机制、问操作，即时得到回答。

Game Buddy 是 [SideDesk](https://github.com/gozhhu/sidedesk-win) 的**第三方模块**（module sidecar）：
独立进程，通过 stdin/stdout 上的 NDJSON / JSON-RPC 2.0 与宿主通信，界面由 `sideui/v1` 声明式描述，
渲染在宿主 `mounts: ["overlay"]` 的**置顶无边框悬浮窗**里。

## 它能做什么

- 悬浮窗常驻桌面之上，边玩边问，不必切出游戏。
- 走**在线 RAG**：把端点指向你的知识库服务（Dify / RAGFlow / OneAPI / 任意 OpenAI 兼容端点），
  检索与生成都由后端完成，本模块负责提问与展示。
- 回答以 Markdown 渲染，带历史记录，可随时改配置。

## 已知限制（先说清楚）

- **独占全屏（D3D exclusive）游戏上悬浮窗不会显示**——这是 Windows 层限制；无边框 / 窗口化游戏正常。
- 悬浮窗是普通可激活窗口，**点击它会抢焦点**，全屏游戏可能被顶到后台。彻底解决需要宿主给 overlay 加
  `WS_EX_NOACTIVATE`，目前尚未提供。
- 出网经宿主 `http.fetch`，受宿主上限约束：**10 秒超时**、响应 ≤ 2 MiB、请求体 ≤ 64 KiB、**不流式**。
  慢模型或长思考链可能超时。
- API Key 存在模块自己的 SQLite（宿主 `plugin-data/game-buddy.db`），卸载模块即删除；
  宿主审计日志只记请求头**名字**、不记值。

## 安装

1. 从 [Releases](../../releases) 下载 `game-buddy-<版本>.zip`（zip 根目录平铺 `module.json` 与 `game-buddy.exe`）。
2. SideDesk → 设置 → 插件页 → **第三方模块** → **导入模块**，选择该 zip。
3. **授予能力**：设置 → 插件页 → **模块授权** → 给 `game-buddy` 授予 `http.*`。
   不授权的话每次提问都会返回 `2002`（能力未授权）。
4. 在悬浮窗里点「设置」填写端点信息。

> 前提：宿主版本需支持 `http.fetch` 的自定义请求头（[sidedesk-win#850](https://github.com/gozhhu/sidedesk-win/issues/850)）。
> 更早的宿主不支持 `Authorization` 头，本模块无法通过鉴权。

## 配置

设置页填四项，存进模块自己的存储：

| 字段 | 说明 | 示例 |
|---|---|---|
| 接口地址 | OpenAI 兼容基址（会拼 `/chat/completions`） | `https://api.example.com/v1` |
| API Key | 以 `Authorization: Bearer <key>` 发送 | `sk-...` |
| 模型 | 模型名 | `gpt-4o-mini` |
| 系统提示词 | 可留空，留空时用内置的攻略助手提示 | （可选） |

「测试连接」会发一句最小请求验证端点可达。

## 构建

需要 Rust 工具链；交叉编译到 Windows 还需 `x86_64-w64-mingw32-gcc`（mingw-w64）。

```bash
cargo test                                              # 单测
cargo build && python3 tests/fake-host.py target/debug/game-buddy   # 扮演宿主的协议驱动
cargo build --release --target x86_64-pc-windows-gnu    # 产出 game-buddy.exe
bash tools/pack.sh                                      # 打成可导入的 zip
```

`tests/fake-host.py` 会完整走一遍「握手 → 读配置 → 下发声明 → 开窗 → 提问 → 拦截 `http.fetch` 并回
OpenAI 兼容响应 → 断言回答落进状态 → 关窗退出」，并校验 **stdout 只能有帧**这条宿主纪律。

## 协议要点（给改代码的人）

- 模块**先发** `initialize`，`id` 必须是**字符串**；不合规宿主会直接杀进程、不回错误帧。
- **stdout 只写帧**，所有诊断走 stderr。
- `ui.declare` 的载荷在 `params.json` 里，是**字符串**不是对象。
- 文本槽只接受 `@key`（宿主词典）或 `{path}`（数据绑定），**裸字面量会被拒**——所以本模块的全部文案
  都经 `state` 的**扁平**顶层键送达（宿主不按点嵌套解析）。
- 布尔要发**字符串** `"true"`/`"false"`：JSON 布尔经宿主序列化会变成 `"True"`，与它按 Ordinal
  比较的判据对不上。
- 每个宿主请求必须回**恰好一帧**，`id` 原样回传；无值动作的 `params` 是 `null`。

## 许可

MIT
