//! 知识库检索：调 wow-rag 的 `/v1/retrieve`，拿回可引用的片段。
//!
//! 走宿主 `http.fetch`（与对话同一套 SSRF 卫兵与上限），鉴权用 Basic——
//! `Authorization` 正好在宿主的请求头白名单里（sidedesk-win#850）。

use crate::config::Config;
use crate::protocol::{CallError, Conn};
use crate::rag::{rpc_code, rpc_message, RagError, Source};
use base64::Engine;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// 检索端点未配置时返回空片段（调用方据此退化成普通对话）。
pub fn retrieve<R: BufRead, W: Write>(
    conn: &mut Conn<R, W>,
    cfg: &Config,
    question: &str,
) -> Result<Vec<Source>, RagError> {
    if !cfg.is_rag_enabled() {
        return Ok(Vec::new());
    }

    let payload = json!({
        "query": question,
        "top_k": cfg.rag_top_k.clamp(1, 20),
    });

    let mut headers = serde_json::Map::new();
    headers.insert("Content-Type".to_owned(), json!("application/json"));
    if !cfg.rag_user.trim().is_empty() {
        let token = basic_token(&cfg.rag_user, &cfg.rag_pass);
        headers.insert("Authorization".to_owned(), json!(format!("Basic {token}")));
    }

    let params = json!({
        "url": cfg.rag_url.trim(),
        "method": "POST",
        "headers": Value::Object(headers),
        "body": payload.to_string(),
    });

    let result = conn.call("http.fetch", params).map_err(|e| match e {
        CallError::Closed => RagError::Host {
            code: 0,
            message: "宿主连接已断开".to_owned(),
        },
        CallError::Rpc(err) => RagError::Host {
            code: rpc_code(&err),
            message: rpc_message(&err),
        },
    })?;

    let status = result.get("status").and_then(Value::as_i64).unwrap_or(0);
    let body = result.get("body").and_then(Value::as_str).unwrap_or_default();
    if status != 200 {
        let brief: String = body.chars().take(160).collect();
        return Err(RagError::Host {
            code: 0,
            message: format!("检索接口 HTTP {status}：{brief}"),
        });
    }
    parse_sources(body)
}

/// 解析 wow-rag 的响应（纯函数，便于单测）。
pub fn parse_sources(body: &str) -> Result<Vec<Source>, RagError> {
    let value: Value =
        serde_json::from_str(body).map_err(|e| RagError::Shape(format!("检索响应不是 JSON：{e}")))?;
    let results = value
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| RagError::Shape("检索响应缺少 results".to_owned()))?;

    Ok(results
        .iter()
        .map(|r| Source {
            title: field(r, "title"),
            section: field(r, "section"),
            site: field(r, "site"),
            url: field(r, "url"),
            text: field(r, "text"),
        })
        .filter(|s| !s.text.trim().is_empty())
        .collect())
}

fn field(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

/// `Basic base64(user:pass)`。
pub fn basic_token(user: &str, password: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 解析检索结果() {
        let body = r#"{"count":1,"results":[{"title":"地下城","section":"血槌熔炉","site":"wx-wow","url":"https://x/1","text":"首领掉落……"},{"title":"空","text":""}]}"#;
        let sources = parse_sources(body).unwrap();
        assert_eq!(sources.len(), 1, "空正文的条目要丢掉");
        assert_eq!(sources[0].section, "血槌熔炉");
        assert_eq!(sources[0].site, "wx-wow");
    }

    #[test]
    fn 响应形状不符报错() {
        assert!(parse_sources("not json").is_err());
        assert!(parse_sources(r#"{"count":0}"#).is_err());
    }

    #[test]
    fn basic_凭据编码() {
        // base64("wow:pass") = d293OnBhc3M=
        assert_eq!(basic_token("wow", "pass"), "d293OnBhc3M=");
    }
}
