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
const KEY_PRESETS: &str = "presets";
const KEY_RAG_URL: &str = "ragUrl";
const KEY_RAG_USER: &str = "ragUser";
const KEY_RAG_PASS: &str = "ragPass";
const KEY_RAG_TOP_K: &str = "ragTopK";

/// 快捷提问槽位数量（界面是静态声明，槽位固定，每个槽一个动作名）。
pub const PRESET_SLOTS: usize = 3;
/// 知识库检索默认取几条片段。
pub const DEFAULT_RAG_TOP_K: usize = 5;

/// 模块配置。空 `base_url` / `api_key` / `model` 视为未配置。
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub system: String,
    /// `"zh"` 或 `"en"`，空 = 跟随默认（中文）。
    pub lang: String,
    /// 快捷提问文案，长度恒为 [`PRESET_SLOTS`]（空串 = 该槽不显示按钮）。
    pub presets: Vec<String>,
    /// 知识库检索端点（如 `https://gw.mrkop.com/wow-rag/v1/retrieve`，留空则不检索）。
    pub rag_url: String,
    /// 检索端点的 Basic Auth 用户名 / 口令（留空则不带凭据）。
    pub rag_user: String,
    pub rag_pass: String,
    /// 每次检索取几条。
    pub rag_top_k: usize,
}

/// 加载结果：配置本体 + 是否因为**缺少能力授权**而读不到。
///
/// 区分这两种情形很重要——读不到配置时用户看到的是空表单，若不点明「去授权」，
/// 会被误当成「模块坏了」。
#[derive(Debug, Clone, Default)]
pub struct LoadOutcome {
    pub cfg: Config,
    pub denied: bool,
}

impl Config {
    /// 是否启用了知识库检索。
    pub fn is_rag_enabled(&self) -> bool {
        !self.rag_url.trim().is_empty()
    }

    /// 是否具备发起请求的最低条件。
    pub fn is_ready(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.api_key.trim().is_empty() && !self.model.trim().is_empty()
    }

    /// `chat/completions` 的完整地址（容忍用户把 `/v1` 写成带尾斜杠）。
    pub fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim().trim_end_matches('/'))
    }

    /// 从宿主存储读取；缺失的键保持默认值，授权缺失单独标记。
    pub fn load<R: BufRead, W: Write>(conn: &mut Conn<R, W>) -> LoadOutcome {
        let mut denied = false;
        // 语言先读：快捷提问的默认文案按语言取
        let lang = get(conn, KEY_LANG, &mut denied);
        let mut cfg = Self {
            base_url: get(conn, KEY_BASE_URL, &mut denied),
            api_key: get(conn, KEY_API_KEY, &mut denied),
            model: get(conn, KEY_MODEL, &mut denied),
            system: get(conn, KEY_SYSTEM, &mut denied),
            lang,
            presets: Vec::new(),
            rag_url: get(conn, KEY_RAG_URL, &mut denied),
            rag_user: get(conn, KEY_RAG_USER, &mut denied),
            rag_pass: get(conn, KEY_RAG_PASS, &mut denied),
            rag_top_k: get(conn, KEY_RAG_TOP_K, &mut denied)
                .parse()
                .unwrap_or(DEFAULT_RAG_TOP_K),
        };
        cfg.presets = presets_from(get(conn, KEY_PRESETS, &mut denied), &cfg.lang);
        LoadOutcome { cfg, denied }
    }

    /// 写回宿主存储。失败只记 stderr，不影响后续使用（内存里的值仍然生效）。
    pub fn save<R: BufRead, W: Write>(&self, conn: &mut Conn<R, W>) {
        let presets_json = serde_json::to_string(&self.presets).unwrap_or_else(|_| "[]".to_owned());
        let rag_top_k = self.rag_top_k.max(1).to_string();
        for (key, value) in [
            (KEY_BASE_URL, &self.base_url),
            (KEY_API_KEY, &self.api_key),
            (KEY_MODEL, &self.model),
            (KEY_SYSTEM, &self.system),
            (KEY_LANG, &self.lang),
            (KEY_PRESETS, &presets_json),
            (KEY_RAG_URL, &self.rag_url),
            (KEY_RAG_USER, &self.rag_user),
            (KEY_RAG_PASS, &self.rag_pass),
            (KEY_RAG_TOP_K, &rag_top_k),
        ] {
            if let Err(err) = conn.call("store.set", json!({ "key": key, "value": value })) {
                eprintln!("[game-buddy] store.set {key} 失败: {err}");
            }
        }
    }

    /// 第 `i` 个快捷提问（越界或空串返回空）。
    pub fn preset(&self, i: usize) -> &str {
        self.presets.get(i).map(String::as_str).unwrap_or_default()
    }

    /// 按动作名写入单个字段（设置页的 `input` 事件用）。未知字段返回 false。
    pub fn set_field(&mut self, field: &str, value: &str) -> bool {
        if let Some(rest) = field.strip_prefix("preset") {
            let Ok(idx) = rest.parse::<usize>() else { return false };
            if idx >= PRESET_SLOTS {
                return false;
            }
            self.presets.resize(PRESET_SLOTS, String::new());
            self.presets[idx] = value.to_owned();
            return true;
        }

        if field == KEY_RAG_TOP_K {
            if let Ok(n) = value.trim().parse::<usize>() {
                self.rag_top_k = n.clamp(1, 20);
                return true;
            }
            return false;
        }

        let slot = match field {
            KEY_BASE_URL => &mut self.base_url,
            KEY_API_KEY => &mut self.api_key,
            KEY_MODEL => &mut self.model,
            KEY_SYSTEM => &mut self.system,
            KEY_LANG => &mut self.lang,
            KEY_RAG_URL => &mut self.rag_url,
            KEY_RAG_USER => &mut self.rag_user,
            KEY_RAG_PASS => &mut self.rag_pass,
            _ => return false,
        };
        *slot = value.to_owned();
        true
    }
}

/// 把存储里的 JSON 数组规整成固定槽位：缺的补空串、多的截断、解析失败回落默认值。
fn presets_from(raw: String, lang: &str) -> Vec<String> {
    let defaults = || -> Vec<String> {
        crate::i18n::strings(lang)
            .default_presets
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    };

    let mut slots = if raw.trim().is_empty() {
        defaults()
    } else {
        match serde_json::from_str::<Vec<String>>(&raw) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("[game-buddy] presets 不是合法的字符串数组（{err}），回落默认值");
                defaults()
            }
        }
    };
    slots.resize(PRESET_SLOTS, String::new());
    slots.truncate(PRESET_SLOTS);
    slots
}

/// 读一个键；失败与缺失都回落空串（模块不该因为存储不可用就起不来）。
fn get<R: BufRead, W: Write>(conn: &mut Conn<R, W>, key: &str, denied: &mut bool) -> String {
    match conn.call("store.get", json!({ "key": key })) {
        Ok(result) => result
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        Err(CallError::Closed) => String::new(),
        Err(CallError::Rpc(err)) => {
            // 2001 未声明 / 2002 未授权 —— 都指向「去授权窗」
            let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
            if code == 2001 || code == 2002 {
                *denied = true;
            } else {
                eprintln!("[game-buddy] store.get {key} 失败: {err}");
            }
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

    #[test]
    fn 缺字段就不算就绪() {
        let cfg = Config {
            base_url: "https://x/v1".into(),
            api_key: "k".into(),
            ..Default::default()
        };
        assert!(!cfg.is_ready(), "缺 model 时不该就绪");
    }

    #[test]
    fn 快捷提问槽位规整成固定数量() {
        let fresh = presets_from(String::new(), "zh");
        assert_eq!(fresh.len(), PRESET_SLOTS);
        assert_eq!(fresh[0], "这个 BOSS 怎么打？", "未配置时用内置默认");

        let short = presets_from(r#"["a"]"#.to_owned(), "zh");
        assert_eq!(short, vec!["a", "", ""], "缺的槽补空串");

        let long = presets_from(r#"["a","b","c","d"]"#.to_owned(), "zh");
        assert_eq!(long.len(), PRESET_SLOTS, "多的截断");

        let broken = presets_from("not json".to_owned(), "zh");
        assert_eq!(broken.len(), PRESET_SLOTS, "坏数据回落默认而不是清空");
    }

    #[test]
    fn 按槽位写快捷提问() {
        let mut cfg = Config::default();
        assert!(cfg.set_field("preset1", "自定义问题"));
        assert_eq!(cfg.preset(1), "自定义问题");
        assert_eq!(cfg.preset(0), "", "没写的槽是空的（界面据此隐藏按钮）");
        assert!(!cfg.set_field("preset9", "x"), "越界槽位要拒绝");
    }
}
