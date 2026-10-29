//! `sideui/v1` 声明与 `state` 载荷。
//!
//! 两条硬规矩（宿主 ADR-0017 §3）：
//! - 文本槽只接受 `@key`（宿主词典）或 `{path}`（数据绑定），**裸字面量一律被拒**；
//! - 所以本模块的每一句自有文案都经 `state` 的扁平顶层键送达，声明里只写占位符。
//!
//! 视觉值一律写语义 token（`accent` / `text.muted` / `small` …），不写 `#RRGGBB` 或字号数字。

use crate::config::Config;
use crate::i18n::Strings;
use crate::rag::Source;
use serde_json::{json, Value};

/// 当前显示的界面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Ask,
    Settings,
}

/// 对话里一条消息的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    /// OpenAI 兼容接口用的角色名。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

/// 一条对话记录。
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

/// 问答视图的声明。
fn ask_declaration() -> Value {
    json!({
        "schema": "sideui/v1",
        "id": "game-buddy",
        "title": "{t_title}",
        "category": "interactive",
        "body": {
            "type": "Stack",
            "props": { "spacing": 8, "margin": 8 },
            "children": [
                { "type": "Text",
                  "props": { "text": "{status}", "size": "small", "color": "text.muted", "wrap": true } },
                { "type": "Markdown", "props": { "text": "{answer}" } },
                { "type": "Collapsible", "if": "{has_sources}",
                  "props": { "label": "{t_sources}", "open": false },
                  "children": [
                      { "type": "Repeat",
                        "repeat": { "source": "sources", "as": "s" },
                        "props": { "spacing": 2 },
                        "children": [
                            { "type": "Text",
                              "props": { "text": "{s.label}", "size": "small", "color": "text.muted", "trim": true } }
                        ] }
                  ] },
                { "type": "Repeat",
                  "repeat": { "source": "history", "as": "m" },
                  "props": { "spacing": 2 },
                  "children": [
                      { "type": "Text",
                        "props": { "text": "{m.text}", "size": "small", "color": "text.secondary", "trim": true } }
                  ] },
                { "type": "Stack",
                  "props": { "orientation": "horizontal", "spacing": 6 },
                  "children": [
                      { "type": "Button", "if": "{p0_on}",
                        "props": { "label": "{p0_label}", "size": "small" },
                        "events": { "click": "preset.0" } },
                      { "type": "Button", "if": "{p1_on}",
                        "props": { "label": "{p1_label}", "size": "small" },
                        "events": { "click": "preset.1" } },
                      { "type": "Button", "if": "{p2_on}",
                        "props": { "label": "{p2_label}", "size": "small" },
                        "events": { "click": "preset.2" } }
                  ] },
                { "type": "TextBox",
                  "props": { "placeholder": "{t_ask_placeholder}", "size": "body" },
                  "events": { "input": "draft", "submit": "ask" } },
                { "type": "Stack",
                  "props": { "orientation": "horizontal", "spacing": 6 },
                  "children": [
                      { "type": "Button",
                        "props": { "label": "{t_send}", "color": "accent" },
                        "events": { "click": "ask" } },
                      { "type": "Button",
                        "props": { "label": "{t_clear}" },
                        "events": { "click": "clear" } },
                      { "type": "Button",
                        "props": { "label": "{t_settings}" },
                        "events": { "click": "settings.open" } }
                  ] }
            ]
        }
    })
}

/// 设置视图的声明。
fn settings_declaration() -> Value {
    json!({
        "schema": "sideui/v1",
        "id": "game-buddy",
        "title": "{t_settings}",
        "category": "interactive",
        "body": {
            "type": "Stack",
            "props": { "spacing": 6, "margin": 8 },
            "children": [
                { "type": "Text", "props": { "text": "{t_field_base}", "size": "small", "color": "text.muted" } },
                { "type": "TextBox",
                  "props": { "text": "{baseUrl}", "placeholder": "{t_hint_base}", "size": "small" },
                  "events": { "input": "settings.baseUrl" } },

                { "type": "Text", "props": { "text": "{t_field_key}", "size": "small", "color": "text.muted" } },
                { "type": "TextBox",
                  "props": { "text": "{apiKey}", "placeholder": "{t_hint_key}", "size": "small" },
                  "events": { "input": "settings.apiKey" } },

                { "type": "Text", "props": { "text": "{t_field_model}", "size": "small", "color": "text.muted" } },
                { "type": "TextBox",
                  "props": { "text": "{model}", "placeholder": "{t_hint_model}", "size": "small" },
                  "events": { "input": "settings.model" } },

                { "type": "Text", "props": { "text": "{t_field_system}", "size": "small", "color": "text.muted" } },
                { "type": "TextBox",
                  "props": { "text": "{system}", "placeholder": "{t_hint_system}", "size": "small" },
                  "events": { "input": "settings.system" } },

                { "type": "Text", "props": { "text": "{t_field_preset}", "size": "small", "color": "text.muted" } },
                { "type": "TextBox",
                  "props": { "text": "{preset0}", "size": "small" },
                  "events": { "input": "settings.preset0" } },
                { "type": "TextBox",
                  "props": { "text": "{preset1}", "size": "small" },
                  "events": { "input": "settings.preset1" } },
                { "type": "TextBox",
                  "props": { "text": "{preset2}", "size": "small" },
                  "events": { "input": "settings.preset2" } },

                { "type": "Text", "props": { "text": "{t_field_rag}", "size": "small", "color": "text.muted" } },
                { "type": "TextBox",
                  "props": { "text": "{ragUrl}", "placeholder": "{t_hint_rag}", "size": "small" },
                  "events": { "input": "settings.ragUrl" } },
                { "type": "TextBox",
                  "props": { "text": "{ragUser}", "placeholder": "{t_hint_rag_user}", "size": "small" },
                  "events": { "input": "settings.ragUser" } },
                { "type": "TextBox",
                  "props": { "text": "{ragPass}", "placeholder": "{t_hint_rag_pass}", "size": "small" },
                  "events": { "input": "settings.ragPass" } },
                { "type": "TextBox",
                  "props": { "text": "{ragTopK}", "placeholder": "{t_hint_rag_topk}", "size": "small" },
                  "events": { "input": "settings.ragTopK" } },

                { "type": "Text", "props": { "text": "{status}", "size": "small", "color": "text.muted", "wrap": true } },

                { "type": "Stack",
                  "props": { "orientation": "horizontal", "spacing": 6 },
                  "children": [
                      { "type": "Button",
                        "props": { "label": "{t_save}", "color": "accent" },
                        "events": { "click": "settings.save" } },
                      { "type": "Button",
                        "props": { "label": "{t_test}" },
                        "events": { "click": "settings.test" } },
                      { "type": "Button",
                        "props": { "label": "{t_lang_switch}" },
                        "events": { "click": "settings.lang" } },
                      { "type": "Button",
                        "props": { "label": "{t_back}" },
                        "events": { "click": "settings.back" } }
                  ] }
            ]
        }
    })
}

/// 按视图取声明（整集替换，宿主收到即重绘）。
pub fn declaration(view: View) -> Value {
    match view {
        View::Ask => ask_declaration(),
        View::Settings => settings_declaration(),
    }
}

/// 组装 `state` 载荷。
///
/// 顶层键就是 `{path}` 能取的**全部**词表：宿主不按点嵌套解析，所以文案键一律扁平。
/// 布尔必须是字符串 `"true"`/`"false"`——JSON 布尔经宿主序列化会变成 `"True"`，与它按
/// Ordinal 比较的判据对不上。
///
/// `history` 只摊**用户提问**（助手回答已经由 `answer` 单条展示，不重复列）。
pub fn state(
    strings: &Strings,
    cfg: &Config,
    answer: &str,
    status: &str,
    turns: &[Message],
    sources: &[Source],
) -> Value {
    let source_items: Vec<Value> = sources
        .iter()
        .filter(|s| !s.text.trim().is_empty())
        .map(|s| json!({ "label": s.label(), "url": s.url }))
        .collect();
    let has_sources = if source_items.is_empty() { "false" } else { "true" };
    let history: Vec<Value> = turns
        .iter()
        .filter(|m| m.role == Role::User)
        .map(|m| json!({ "text": m.text }))
        .collect();

    json!({
        "t_title": strings.title,
        "t_ask_placeholder": strings.ask_placeholder,
        "t_send": strings.send,
        "t_clear": strings.clear,
        "t_settings": strings.settings,
        "t_back": strings.back,
        "t_save": strings.save,
        "t_test": strings.test,
        "t_field_base": strings.field_base,
        "t_field_key": strings.field_key,
        "t_field_model": strings.field_model,
        "t_field_system": strings.field_system,
        "t_field_preset": strings.field_preset,
        "t_field_rag": strings.field_rag,
        "t_hint_rag": strings.hint_rag,
        "t_hint_rag_user": strings.hint_rag_user,
        "t_hint_rag_pass": strings.hint_rag_pass,
        "t_hint_rag_topk": strings.hint_rag_topk,
        "t_sources": strings.sources,
        "t_field_lang": strings.field_lang,
        "t_hint_base": strings.hint_base,
        "t_hint_key": strings.hint_key,
        "t_hint_model": strings.hint_model,
        "t_hint_system": strings.hint_system,
        "t_lang_switch": strings.lang_switch,

        "answer": answer,
        "status": status,
        "busy": "false",
        "history": history,
        "sources": source_items,
        "has_sources": has_sources,

        // 快捷提问：pN_on 决定按钮显示与否（空串隐藏），pN_label 是按钮文案
        "p0_on": on_off(cfg.preset(0)),
        "p0_label": cfg.preset(0),
        "p1_on": on_off(cfg.preset(1)),
        "p1_label": cfg.preset(1),
        "p2_on": on_off(cfg.preset(2)),
        "p2_label": cfg.preset(2),

        "baseUrl": cfg.base_url,
        "apiKey": cfg.api_key,
        "model": cfg.model,
        "system": cfg.system,
        "preset0": cfg.preset(0),
        "preset1": cfg.preset(1),
        "preset2": cfg.preset(2),
        "ragUrl": cfg.rag_url,
        "ragUser": cfg.rag_user,
        "ragPass": cfg.rag_pass,
        "ragTopK": cfg.rag_top_k.to_string(),
    })
}

/// 布尔槽必须是字符串：宿主按 Ordinal 比 `"true"`，JSON 布尔会序列化成 `"True"`。
fn on_off(value: &str) -> &'static str {
    if value.trim().is_empty() {
        "false"
    } else {
        "true"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 声明不含裸字面量文本槽() {
        let decl = declaration(View::Ask);
        let text = decl["body"]["children"].to_string();
        assert!(text.contains("{status}"));
        assert!(text.contains("{t_ask_placeholder}"));
        assert!(!text.contains("\"text\":\"就绪\""));
    }

    #[test]
    fn 状态键扁平且文案齐全() {
        let s = crate::i18n::strings("zh");
        let cfg = Config::default();
        let v = state(s, &cfg, "", s.idle, &[], &[]);
        assert_eq!(v["t_title"], "游戏助手");
        assert_eq!(v["busy"], "false");
        assert_eq!(v["status"], "就绪");
        assert!(v["history"].is_array());
        assert!(v["t_send"].is_string());
        assert!(v["t_clear"].is_string());
    }

    #[test]
    fn 快捷提问空槽隐藏按钮() {
        let s = crate::i18n::strings("zh");
        let cfg = Config {
            presets: vec!["问一".into(), String::new(), "问三".into()],
            ..Default::default()
        };
        let v = state(s, &cfg, "", s.idle, &[], &[]);
        assert_eq!(v["p0_on"], "true");
        assert_eq!(v["p0_label"], "问一");
        assert_eq!(v["p1_on"], "false", "空槽要隐藏按钮");
        assert_eq!(v["p2_on"], "true");
        assert_eq!(v["preset2"], "问三", "设置页绑定的是同一份值");
    }

    #[test]
    fn 历史只列用户提问() {
        let s = crate::i18n::strings("zh");
        let turns = vec![
            Message { role: Role::User, text: "问一".into() },
            Message { role: Role::Assistant, text: "答一".into() },
        ];
        let v = state(s, &Config::default(), "答一", s.idle, &turns, &[]);
        let history = v["history"].as_array().unwrap();
        assert_eq!(history.len(), 1, "助手回答不该重复出现在历史里");
        assert_eq!(history[0]["text"], "问一");
    }
}
