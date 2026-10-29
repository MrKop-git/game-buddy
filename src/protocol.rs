//! sidecar/v1 线格式：一行一帧的 JSON-RPC 2.0（NDJSON）。
//!
//! 硬纪律（来自宿主 ADR-0018）：
//! - stdout **只写帧**，任何诊断一律走 stderr；
//! - 模块**先发** `initialize`，且 `id` 必须是字符串；
//! - 每个宿主请求必须回**恰好一帧**，`id` 原样回传。

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{self, BufRead, Write};

/// 我们支持的协议版本。
pub const PROTOCOL_VERSION: &str = "sidecar/v1";

/// 本模块在清单与握手里的 id。
pub const MODULE_ID: &str = "game-buddy";

/// 宿主发来的一帧。
#[derive(Debug, Clone)]
pub enum Incoming {
    /// 带 `id` + `method`：宿主请求，必须回一帧。
    Request { id: String, method: String, params: Value },
    /// 只有 `method`：通知，**不得**回包。
    Notification { method: String, params: Value },
    /// 只有 `id`：宿主对我们请求的应答。
    Response { id: String, result: Option<Value>, error: Option<Value> },
}

/// 解析一行；空行与非法 JSON 返回 `None`（宿主侧同样只记 Debug 不致命）。
pub fn parse_line(line: &str) -> Option<Incoming> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(trimmed).ok()?;
    let id = v.get("id").and_then(Value::as_str).map(str::to_owned);
    let method = v.get("method").and_then(Value::as_str).map(str::to_owned);
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    match (id, method) {
        (Some(id), Some(method)) => Some(Incoming::Request { id, method, params }),
        (None, Some(method)) => Some(Incoming::Notification { method, params }),
        (Some(id), None) => Some(Incoming::Response {
            id,
            result: v.get("result").cloned(),
            error: v.get("error").cloned(),
        }),
        (None, None) => None,
    }
}

/// 一次调用失败的原因。
#[derive(Debug)]
pub enum CallError {
    /// 进程/管道断开。
    Closed,
    /// 宿主回了 `error` 帧。
    Rpc(Value),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "宿主连接已断开"),
            Self::Rpc(e) => {
                let code = e.get("code").and_then(Value::as_i64).unwrap_or(0);
                let message = e.get("message").and_then(Value::as_str).unwrap_or("");
                write!(f, "宿主拒绝（{code}）：{message}")
            }
        }
    }
}

/// 双向连接：读帧、写帧，并在等待应答期间把不相干的帧排队。
pub struct Conn<R: BufRead, W: Write> {
    reader: R,
    writer: W,
    queue: VecDeque<Incoming>,
    next_id: u64,
}

impl<R: BufRead, W: Write> Conn<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer, queue: VecDeque::new(), next_id: 0 }
    }

    /// 写一帧并立即 flush（宿主是按行读的，攒着就等于不回）。
    fn write_frame(&mut self, frame: &Value) -> io::Result<()> {
        let line = serde_json::to_string(frame).map_err(io::Error::other)?;
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }

    /// 模块 → 宿主的通知（无 `id`，不期待回包）。
    pub fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        self.write_frame(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    /// 请求的应答：成功。
    pub fn respond_ok(&mut self, id: &str, result: Value) -> io::Result<()> {
        self.write_frame(&json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    /// 请求的应答：业务失败（`error` 帧是正常终态，不是崩溃）。
    pub fn respond_err(&mut self, id: &str, code: i64, message: &str) -> io::Result<()> {
        self.write_frame(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        }))
    }

    /// 阻塞读下一帧；EOF 时返回 `None`。
    fn read_frame(&mut self) -> Option<Incoming> {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) => {
                    if let Some(frame) = parse_line(&line) {
                        return Some(frame);
                    }
                }
                Err(err) => {
                    eprintln!("[game-buddy] stdin 读取失败: {err}");
                    return None;
                }
            }
        }
    }

    /// 向宿主发一次请求并等它的应答。
    ///
    /// 等待期间收到的**其它**帧先入队，由主循环稍后处理——宿主对能力调用是串行的
    /// （ADR-0023 §4），这里不会把它自己的请求饿死。
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        self.next_id += 1;
        let id = format!("gb-{}", self.next_id);
        let frame = json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        });
        self.write_frame(&frame).map_err(|_| CallError::Closed)?;

        loop {
            match self.read_frame() {
                None => return Err(CallError::Closed),
                Some(Incoming::Response { id: rid, result, error }) if rid == id => {
                    return match error {
                        Some(err) => Err(CallError::Rpc(err)),
                        None => Ok(result.unwrap_or(Value::Null)),
                    };
                }
                Some(other) => self.queue.push_back(other),
            }
        }
    }

    /// 取下一帧：优先取队列里被推迟的，再读 stdin。
    pub fn next(&mut self) -> Option<Incoming> {
        self.queue.pop_front().or_else(|| self.read_frame())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 解析三类帧() {
        let req = parse_line(r#"{"jsonrpc":"2.0","id":"r1","method":"ask","params":{"value":"x"}}"#)
            .expect("应解析为请求");
        assert!(matches!(req, Incoming::Request { ref id, ref method, .. } if id == "r1" && method == "ask"));

        let note = parse_line(r#"{"jsonrpc":"2.0","method":"ui.lifecycle","params":{"event":"opened"}}"#)
            .expect("应解析为通知");
        assert!(matches!(note, Incoming::Notification { ref method, .. } if method == "ui.lifecycle"));

        let resp = parse_line(r#"{"jsonrpc":"2.0","id":"gb-1","result":{"ok":true}}"#)
            .expect("应解析为应答");
        assert!(matches!(resp, Incoming::Response { ref id, .. } if id == "gb-1"));
    }

    #[test]
    fn 空行与坏行被丢弃() {
        assert!(parse_line("").is_none());
        assert!(parse_line("   ").is_none());
        assert!(parse_line("not json").is_none());
        assert!(parse_line(r#"{"jsonrpc":"2.0"}"#).is_none());
    }
}
