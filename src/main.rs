//! Game Buddy —— SideDesk 第三方模块（sidecar）。
//!
//! 形态：独立进程，走 stdin/stdout 上的 NDJSON / JSON-RPC 2.0（`sidecar/v1`），
//! 由宿主以 `mounts: ["overlay","tool"]` 拉起悬浮窗与工具面板。
//!
//! 纪律：**stdout 只写帧**，所有诊断走 stderr（宿主只保留 stderr 末尾若干行）。

mod config;
mod i18n;
mod protocol;
mod rag;
mod ui;

use config::{Config, LoadOutcome};
use protocol::{Conn, Incoming, MODULE_ID, PROTOCOL_VERSION};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufWriter, Write};
use ui::{Message, Role, View};

/// 进程内的全部可变状态。
struct App {
    view: View,
    /// 当前已下发的声明对应哪个视图（`None` = 还没发过）。
    declared: Option<View>,
    cfg: Config,
    answer: String,
    status: String,
    /// 完整对话（含助手回答），既用于展示历史也用于多轮上下文。
    turns: Vec<Message>,
    draft: String,
}

impl App {
    fn new(outcome: LoadOutcome) -> Self {
        let strings = i18n::strings(&outcome.cfg.lang);
        // 读不到配置且原因是缺授权时，直接把「去授权」写在状态行上——
        // 否则用户看到的是空表单，会误以为模块坏了。
        let status = if outcome.denied { strings.need_grant } else { strings.idle };
        Self {
            view: View::Ask,
            declared: None,
            cfg: outcome.cfg,
            answer: String::new(),
            status: status.to_owned(),
            turns: Vec::new(),
            draft: String::new(),
        }
    }

    fn strings(&self) -> &'static i18n::Strings {
        i18n::strings(&self.cfg.lang)
    }

    /// 下发声明（视图变了才发）与状态。`force_declare` 用于窗口刚打开时补发。
    fn publish<R: BufRead, W: Write>(
        &mut self,
        conn: &mut Conn<R, W>,
        force_declare: bool,
    ) -> io::Result<()> {
        if force_declare || self.declared != Some(self.view) {
            // 注意：声明放在 params.json 里，是**字符串**不是对象
            conn.notify("ui.declare", json!({ "json": ui::declaration(self.view).to_string() }))?;
            self.declared = Some(self.view);
        }
        let payload = ui::state(self.strings(), &self.cfg, &self.answer, &self.status, &self.turns);
        conn.notify("event", json!({ "topic": "state", "payload": payload }))
    }
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut conn = Conn::new(stdin.lock(), BufWriter::new(stdout.lock()));

    // 握手：模块**先发** initialize，id 必须是字符串。
    let init = json!({
        "protocolVersion": PROTOCOL_VERSION,
        "moduleId": MODULE_ID,
        "capabilities": ["http.*", "store.*", "log.*"],
        "provides": [],
        "supersedes": []
    });
    if let Err(err) = conn.call("initialize", init) {
        eprintln!("[game-buddy] 握手失败：{err}");
        return Ok(());
    }

    // 握手通过后才允许调能力：此时才读配置。
    let mut app = App::new(Config::load(&mut conn));

    // 先把界面与状态发出去：窗口尚未打开时宿主会存着，打开即渲染。
    app.publish(&mut conn, true)?;

    loop {
        let Some(frame) = conn.next() else { break };
        match frame {
            Incoming::Notification { method, params } if method == "ui.lifecycle" => {
                let event = params.get("event").and_then(Value::as_str).unwrap_or_default();
                match event {
                    "opened" | "focused" => app.publish(&mut conn, event == "opened")?,
                    // 关窗即结束：宿主随后回收本进程
                    "closed" => break,
                    _ => {}
                }
            }
            Incoming::Request { id, method, params } => {
                if !handle_action(&mut app, &mut conn, &id, &method, &params)? {
                    // 未知动作也要有终态，否则宿主那边会挂着等
                    conn.respond_err(&id, -32601, &format!("未实现的动作：{method}"))?;
                }
            }
            // 迟到的应答（例如握手期被排队的帧）直接丢弃
            _ => {}
        }
    }

    Ok(())
}

/// 处理一个宿主请求；返回 `false` 表示动作未识别（调用方回 -32601）。
fn handle_action<R: BufRead, W: Write>(
    app: &mut App,
    conn: &mut Conn<R, W>,
    id: &str,
    method: &str,
    params: &Value,
) -> io::Result<bool> {
    // 设置页的动作都带 `settings.` 前缀，字段名即动作名后缀
    if let Some(field) = method.strip_prefix("settings.") {
        match field {
            "open" => {
                app.view = View::Settings;
                conn.respond_ok(id, json!({}))?;
                app.publish(conn, true)?;
                return Ok(true);
            }
            "back" => {
                app.view = View::Ask;
                conn.respond_ok(id, json!({}))?;
                app.publish(conn, true)?;
                return Ok(true);
            }
            "save" => {
                app.cfg.save(conn);
                app.status = app.strings().saved.to_owned();
                conn.respond_ok(id, json!({}))?;
                app.publish(conn, false)?;
                return Ok(true);
            }
            "test" => {
                conn.respond_ok(id, json!({}))?;
                run_connection_test(app, conn)?;
                return Ok(true);
            }
            "lang" => {
                app.cfg.lang = if app.cfg.lang.eq_ignore_ascii_case("en") {
                    "zh".to_owned()
                } else {
                    "en".to_owned()
                };
                app.cfg.save(conn);
                app.status = app.strings().idle.to_owned();
                conn.respond_ok(id, json!({}))?;
                app.publish(conn, true)?;
                return Ok(true);
            }
            other => {
                app.cfg.set_field(other, &value_of(params));
                conn.respond_ok(id, json!({}))?;
                return Ok(true);
            }
        }
    }

    match method {
        // 输入框文字变化（回车前先攒着，按钮点击时才有值可用）
        "draft" => {
            app.draft = value_of(params);
            conn.respond_ok(id, json!({}))?;
        }
        "ask" => {
            conn.respond_ok(id, json!({}))?;
            run_ask(app, conn, value_of(params))?;
        }
        "clear" => {
            app.turns.clear();
            app.answer.clear();
            app.draft.clear();
            app.status = app.strings().cleared.to_owned();
            conn.respond_ok(id, json!({}))?;
            app.publish(conn, false)?;
        }
        // 快捷提问：点一下直接把该槽的文案发出去，不用打字
        "preset.0" | "preset.1" | "preset.2" => {
            let idx = method.trim_start_matches("preset.").parse::<usize>().unwrap_or(0);
            let question = app.cfg.preset(idx).to_owned();
            conn.respond_ok(id, json!({}))?;
            run_ask(app, conn, question)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// 提问：先落状态，再走宿主的 `http.fetch`。
fn run_ask<R: BufRead, W: Write>(
    app: &mut App,
    conn: &mut Conn<R, W>,
    submitted: String,
) -> io::Result<()> {
    let question = if submitted.trim().is_empty() { app.draft.clone() } else { submitted };
    let question = question.trim().to_owned();

    if question.is_empty() {
        app.status = app.strings().empty_question.to_owned();
        return app.publish(conn, false);
    }
    if !app.cfg.is_ready() {
        app.status = app.strings().need_config.to_owned();
        return app.publish(conn, false);
    }

    app.status = app.strings().busy.to_owned();
    app.publish(conn, false)?;

    match rag::ask(conn, &app.cfg, &app.turns, &question) {
        Ok(answer) => {
            app.turns.push(Message { role: Role::User, text: question });
            app.answer = answer.clone();
            app.turns.push(Message { role: Role::Assistant, text: answer });
            app.draft.clear();
            app.status = app.strings().idle.to_owned();
        }
        Err(err) => {
            eprintln!("[game-buddy] 提问失败：{err:?}");
            app.status = err.describe(app.strings());
        }
    }
    app.publish(conn, false)
}

/// 设置页的「测试连接」。
fn run_connection_test<R: BufRead, W: Write>(
    app: &mut App,
    conn: &mut Conn<R, W>,
) -> io::Result<()> {
    if !app.cfg.is_ready() {
        app.status = app.strings().need_config.to_owned();
        return app.publish(conn, false);
    }

    app.status = app.strings().testing.to_owned();
    app.publish(conn, false)?;

    app.status = match rag::test_connection(conn, &app.cfg) {
        Ok(()) => app.strings().test_ok.to_owned(),
        Err(err) => {
            eprintln!("[game-buddy] 连接测试失败：{err:?}");
            err.describe(app.strings())
        }
    };
    app.publish(conn, false)
}

/// 带值动作的载荷：`{"value":"…"}`；无值动作的 `params` 是 `null`。
fn value_of(params: &Value) -> String {
    params
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
