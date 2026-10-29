//! OpenAI 兼容的对话客户端。
//!
//! 模块没有 `ai.*` 能力（宿主不提供），出网一律走宿主 `http.fetch`（ADR-0023）：
//! 有 SSRF 黑名单、10s 超时、2 MiB 响应上限与逐次审计。鉴权头靠 #850 的
//! 逐头白名单（`authorization` / `content-type`）。
//!
//! 检索（RAG 的「R」）由后端完成——把基址指向 Dify / RAGFlow / OneAPI / 任意带知识库的
//! OpenAI 兼容端点即可，本模块只负责提问与展示。

use crate::config::Config;
use crate::protocol::{CallError, Conn};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// 调用失败的分类（文案由调用方决定，这里只带事实）。
#[derive(Debug)]
pub enum RagError {
    /// 配置不完整。
    NotReady,
    /// 宿主能力调用失败（未授权 / 超时 / 出网被拒 …）。
    Host(String),
    /// 端点返回了非 200。
    Http { status: i64, body: String },
    /// 响应不是预期的 OpenAI 形状。
    Shape(String),
}

impl std::fmt::Display for RagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReady => write!(f, "配置不完整"),
            Self::Host(e) => write!(f, "{e}"),
            Self::Http { status, body } => {
                let brief: String = body.chars().take(200).collect();
                write!(f, "HTTP {status}：{brief}")
            }
            Self::Shape(e) => write!(f, "响应无法解析：{e}"),
        }
    }
}

/// 向端点提问，返回助手的回答正文。
pub fn ask<R: BufRead, W: Write>(
    conn: &mut Conn<R, W>,
    cfg: &Config,
    question: &str,
) -> Result<String, RagError> {
    if !cfg.is_ready() {
        return Err(RagError::NotReady);
    }

    let system = if cfg.system.trim().is_empty() {
        "你是游戏助手，回答要具体、可操作，涉及数值与时机时给出确切条件。".to_owned()
    } else {
        cfg.system.clone()
    };

    let body = json!({
        "model": cfg.model,
        "stream": false,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": question }
        ]
    });

    let params = json!({
        "url": cfg.chat_url(),
        "method": "POST",
        "headers": {
            "Authorization": format!("Bearer {}", cfg.api_key),
            "Content-Type": "application/json"
        },
        "body": body.to_string()
    });

    let result = conn.call("http.fetch", params).map_err(|e| match e {
        CallError::Closed => RagError::Host("宿主连接已断开".into()),
        CallError::Rpc(err) => RagError::Host(rpc_message(&err)),
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
    ask(conn, cfg, "ping").map(|_| ())
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

/// 宿主 `error` 帧里最有信息量的一句。
fn rpc_message(err: &Value) -> String {
    let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
    let message = err.get("message").and_then(Value::as_str).unwrap_or("宿主拒绝");
    match code {
        2002 => "宿主未授权 http.* 能力：请到「设置 → 插件页 → 模块授权」里授予".to_owned(),
        2004 => "请求超时（宿主上限 10 秒）：可以换更快的模型或缩短问题".to_owned(),
        2005 => "响应超过宿主 2 MiB 上限".to_owned(),
        2009 => "出网域未授权".to_owned(),
        _ => format!("{message}（{code}）"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
