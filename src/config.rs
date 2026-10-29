//! 配置：经宿主 `store.*` 能力持久化（插件专属 SQLite，卸载即删）。

use crate::protocol::{CallError, Conn};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// `store.*` 的键名。
const KEY_BASE_URL: &str = "baseUrl";
const KEY_API_KEY: &str = "apiKey";
const KEY_MODEL: &str = "model";
const KEY_SYSTEM: &str = "system";
const KEY_LANG: &str = "lang";

/// 模块配置。空 `base_url` / `api_key` 视为未配置。
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub system: String,
    /// `"zh"` 或 `"en"`，空 = 跟随默认（中文）。
    pub lang: String,
}

impl Config {
    /// 是否具备发起请求的最低条件。
    pub fn is_ready(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.api_key.trim().is_empty() && !self.model.trim().is_empty()
    }

    /// `chat/completions` 的完整地址（容忍用户把 `/v1` 写成带尾斜杠）。
    pub fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim().trim_end_matches('/'))
    }

    /// 从宿主存储读取；缺失的键保持默认值。
    pub fn load<R: BufRead, W: Write>(conn: &mut Conn<R, W>) -> Self {
        let mut cfg = Self::default();
        cfg.base_url = get(conn, KEY_BASE_URL);
        cfg.api_key = get(conn, KEY_API_KEY);
        cfg.model = get(conn, KEY_MODEL);
        cfg.system = get(conn, KEY_SYSTEM);
        cfg.lang = get(conn, KEY_LANG);
        cfg
    }

    /// 写回宿主存储。失败只记 stderr，不影响后续使用（内存里的值仍然生效）。
    pub fn save<R: BufRead, W: Write>(&self, conn: &mut Conn<R, W>) {
        for (key, value) in [
            (KEY_BASE_URL, &self.base_url),
            (KEY_API_KEY, &self.api_key),
            (KEY_MODEL, &self.model),
            (KEY_SYSTEM, &self.system),
            (KEY_LANG, &self.lang),
        ] {
            if let Err(err) = conn.call("store.set", json!({ "key": key, "value": value })) {
                eprintln!("[game-buddy] store.set {key} 失败: {err}");
            }
        }
    }

    /// 按动作名写入单个字段（设置页的 `input` 事件用）。未知字段返回 false。
    pub fn set_field(&mut self, field: &str, value: &str) -> bool {
        let slot = match field {
            KEY_BASE_URL => &mut self.base_url,
            KEY_API_KEY => &mut self.api_key,
            KEY_MODEL => &mut self.model,
            KEY_SYSTEM => &mut self.system,
            KEY_LANG => &mut self.lang,
            _ => return false,
        };
        *slot = value.to_owned();
        true
    }
}

/// 读一个键；失败与缺失都回落空串（模块不该因为存储不可用就起不来）。
fn get<R: BufRead, W: Write>(conn: &mut Conn<R, W>, key: &str) -> String {
    match conn.call("store.get", json!({ "key": key })) {
        Ok(result) => result
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        Err(CallError::Closed) => String::new(),
        Err(err) => {
            eprintln!("[game-buddy] store.get {key} 失败: {err}");
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 就绪判定与地址拼接() {
        let mut cfg = Config::default();
        assert!(!cfg.is_ready());

        cfg.base_url = " https://api.example.com/v1/ ".into();
        cfg.api_key = "sk-x".into();
        cfg.model = "m".into();
        assert!(cfg.is_ready());
        assert_eq!(cfg.chat_url(), "https://api.example.com/v1/chat/completions");
    }

    #[test]
    fn 按字段名写入() {
        let mut cfg = Config::default();
        assert!(cfg.set_field("baseUrl", "u"));
        assert!(cfg.set_field("lang", "en"));
        assert_eq!(cfg.base_url, "u");
        assert_eq!(cfg.lang, "en");
        assert!(!cfg.set_field("nope", "x"));
    }
}
