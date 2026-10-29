//! OpenAI 兼容的对话客户端。
//!
//! 模块没有 `ai.*` 能力（宿主不提供），出网一律走宿主 `http.fetch`（ADR-0023）：
//! 有 SSRF 黑名单、10s 超时、2 MiB 响应上限与逐次审计。鉴权头靠 sidedesk-win#850 的
//! 逐头白名单（`authorization` / `content-type`）。
//!
//! 检索（RAG 的「R」）由后端完成——把基址指向 Dify / RAGFlow / OneAPI / 任意带知识库的
//! OpenAI 兼容端点即可，本模块只负责提问与展示。

use crate::config::Config;
use crate::i18n::Strings;
use crate::protocol::{CallError, Conn};
use crate::ui::Message;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// 随请求携带的历史预算（字节）。宿主 `http.fetch` 的请求体上限是 64 KiB，
/// 这里给系统提示词、当前问题与 JSON 结构留出余量。
pub const CONTEXT_BUDGET_BYTES: usize = 48 * 1024;

/// 调用失败的分类（文案由调用方决定，这里只带事实）。
#[derive(Debug)]
pub enum RagError {
    /// 配置不完整。
    NotReady,
    /// 宿主能力调用失败（未授权 / 超时 / 出网被拒 …）。`code` 是宿主错误码，0 = 连接断开。
    Host { code: i64, message: String },
    /// 端点返回了非 200。
    Http { status: i64, body: String },
    /// 响应不是预期的 OpenAI 形状。
    Shape(String),
}

impl RagError {
    /// 面向用户的文案。
    pub fn describe(&self, s: &Strings) -> String {
        match self {
            Self::NotReady => s.need_config.to_owned(),
            Self::Host { code, message } => match code {
                // 2001 未声明 / 2002 未授权：都指向同一处「去授权窗」
                2001 | 2002 => s.need_grant.to_owned(),
                2004 => s.err_timeout.to_owned(),
                2005 => s.err_too_large.to_owned(),
                _ => message.clone(),
            },
            Self::Http { status, body } => describe_http_error(*status, s, body),
            Self::Shape(e) => format!("{}：{e}", s.err_server),
        }
    }
}

/// 把 HTTP 状态映射成可操作的提示（纯函数，便于单测）。
pub fn describe_http_error(status: i64, s: &Strings, body: &str) -> String {
    match status {
        401 | 403 => s.err_auth.to_owned(),
        404 => s.err_not_found.to_owned(),
        429 => s.err_rate.to_owned(),
        500..=599 => s.err_server.to_owned(),
        _ => {
            let brief: String = body.chars().take(160).collect();
            format!("HTTP {status}：{brief}")
        }
    }
}

/// 系统提示词：用户没填就用内置的攻略助手口径。
fn system_prompt(cfg: &Config) -> String {
    if cfg.system.trim().is_empty() {
        "你是游戏助手。回答要具体、可操作；涉及数值、时机与位置时给出确切条件；\
         不确定就直说，不要编造。"
            .to_owned()
    } else {
        cfg.system.clone()
    }
}

/// 组装发往端点的 `messages`：system + 有预算上限的历史 + 当前问题。
///
/// 历史从**最新**往回取，超预算就停——宁可少带上下文，也不让请求体撞上宿主的 64 KiB 硬上限。
pub fn build_messages(cfg: &Config, turns: &[Message], question: &str) -> Vec<Value> {
    let mut messages = vec![json!({ "role": "system", "content": system_prompt(cfg) })];

    let mut budget = CONTEXT_BUDGET_BYTES;
    let mut picked: Vec<Value> = Vec::new();
    for turn in turns.iter().rev() {
        let cost = turn.text.len() + 32; // 32 字节留给 role / JSON 结构
        if cost > budget {
            break;
        }
        budget -= cost;
        picked.push(json!({ "role": turn.role.as_str(), "content": turn.text }));
    }
    picked.reverse();
    messages.extend(picked);

    messages.push(json!({ "role": "user", "content": question }));
    messages
}

/// 向端点提问，返回助手的回答正文。
pub fn ask<R: BufRead, W: Write>(
    conn: &mut Conn<R, W>,
    cfg: &Config,
    turns: &[Message],
    question: &str,
) -> Result<String, RagError> {
    if !cfg.is_ready() {
        return Err(RagError::NotReady);
    }

    let body = json!({
        "model": cfg.model,
        "stream": false,
        "messages": build_messages(cfg, turns, question)
    });
    let body_text = body.to_string();
    if body_text.len() > 60 * 1024 {
        return Err(RagError::Host { code: 0, message: "请求体过大：请清空对话后再试".to_owned() });
    }

    let params = json!({
        "url": cfg.chat_url(),
        "method": "POST",
        "headers": {
            "Authorization": format!("Bearer {}", cfg.api_key),
            "Content-Type": "application/json"
        },
        "body": body_text
    });

    let result = conn.call("http.fetch", params).map_err(|e| match e {
        CallError::Closed => RagError::Host { code: 0, message: "宿主连接已断开".to_owned() },
        CallError::Rpc(err) => RagError::Host { code: rpc_code(&err), message: rpc_message(&err) },
    })?;

    let status = result.get("status").and_then(Value::as_i64).unwrap_or(0);
    let text = result.get("body").and_then(Value::as_str).unwrap_or_default();

    if status != 200 {
        return Err(RagError::Http { status, body: text.to_owned() });
    }
    extract_answer(text)
}

/// 测试连接：发一句最小请求，只关心能不能拿到 200。
pub fn test_connection<R: BufRead, W: Write>(
    conn: &mut Conn<R, W>,
    cfg: &Config,
) -> Result<(), RagError> {
    ask(conn, cfg, &[], "ping").map(|_| ())
}

/// 从 OpenAI 兼容响应里取 `choices[0].message.content`（纯函数，便于单测）。
pub fn extract_answer(body: &str) -> Result<String, RagError> {
    let v: Value = serde_json::from_str(body).map_err(|e| RagError::Shape(e.to_string()))?;
    let content = v
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| RagError::Shape("缺少 choices[0].message.content".into()))?;
    Ok(content.to_owned())
}

/// 宿主 `error` 帧里的错误码（缺省 0）。
fn rpc_code(err: &Value) -> i64 {
    err.get("code").and_then(Value::as_i64).unwrap_or(0)
}

/// 宿主 `error` 帧里的原始消息（文案由 `RagError::describe` 按码决定）。
fn rpc_message(err: &Value) -> String {
    err.get("message")
        .and_then(Value::as_str)
        .unwrap_or("宿主拒绝")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Role;

    fn turn(role: Role, text: &str) -> Message {
        Message { role, text: text.to_owned() }
    }

    #[test]
    fn 取回答正文() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"先打断再绕后"}}]}"#;
        assert_eq!(extract_answer(body).unwrap(), "先打断再绕后");
    }

    #[test]
    fn 形状不符报错而不是空串() {
        assert!(extract_answer(r#"{"choices":[]}"#).is_err());
        assert!(extract_answer("not json").is_err());
    }

    #[test]
    fn 历史按时间顺序拼在问题之前() {
        let cfg = Config::default();
        let turns = vec![turn(Role::User, "第一问"), turn(Role::Assistant, "第一答")];
        let msgs = build_messages(&cfg, &turns, "第二问");
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["content"], "第一问");
        assert_eq!(msgs[2]["role"], "assistant");
        assert_eq!(msgs[3]["content"], "第二问");
    }

    #[test]
    fn 历史超预算时丢最旧的() {
        let cfg = Config::default();
        let big = "x".repeat(30 * 1024);
        let turns = vec![
            turn(Role::User, &big),
            turn(Role::Assistant, &big),
            turn(Role::User, &big),
        ];
        let msgs = build_messages(&cfg, &turns, "问题");
        // 预算 48 KiB：只能容下一整条 30 KiB 的历史，其余被丢弃
        assert_eq!(msgs.len(), 3, "system + 1 条历史 + 当前问题");
        assert_eq!(msgs[2]["content"], "问题");
    }

    #[test]
    fn 宿主错误码走文案表() {
        let s = crate::i18n::strings("zh");
        let host = |code| RagError::Host { code, message: "原始消息".to_owned() };
        assert!(host(2004).describe(s).contains("超时"));
        assert!(host(2005).describe(s).contains("2 MiB"));
        assert!(host(2002).describe(s).contains("授权"), "未授权要指向授权窗");
        assert_eq!(host(9999).describe(s), "原始消息", "未知码保留原始消息");
    }

    #[test]
    fn 错误映射可操作() {
        let s = crate::i18n::strings("zh");
        assert!(describe_http_error(401, s, "").contains("API Key"));
        assert!(describe_http_error(404, s, "").contains("/v1"));
        assert!(describe_http_error(429, s, "").contains("频繁"));
        assert!(describe_http_error(503, s, "").contains("服务端"));
        assert!(describe_http_error(418, s, "teapot").contains("418"));
    }
}
