//! 内置文案。
//!
//! 宿主的 `@key` 只能指向它自己的词典（模块没有自带词典的运行时通道），所以模块自有的
//! 每一句文案都必须经 `state` 送到界面，用扁平键绑 `{t_xxx}`——本文件就是这些文案的源头。

/// 一套界面文案。
pub struct Strings {
    pub title: &'static str,
    pub ask_placeholder: &'static str,
    pub send: &'static str,
    pub settings: &'static str,
    pub back: &'static str,
    pub save: &'static str,
    pub test: &'static str,
    pub field_base: &'static str,
    pub field_key: &'static str,
    pub field_model: &'static str,
    pub field_system: &'static str,
    pub field_lang: &'static str,
    pub hint_base: &'static str,
    pub hint_key: &'static str,
    pub hint_model: &'static str,
    pub hint_system: &'static str,
    pub idle: &'static str,
    pub busy: &'static str,
    pub need_config: &'static str,
    pub saved: &'static str,
    pub testing: &'static str,
    pub test_ok: &'static str,
    pub empty_question: &'static str,
    pub lang_switch: &'static str,
}

const ZH: Strings = Strings {
    title: "游戏助手",
    ask_placeholder: "问点什么：这个 Boss 怎么打？",
    send: "提问",
    settings: "设置",
    back: "返回",
    save: "保存",
    test: "测试连接",
    field_base: "接口地址",
    field_key: "API Key",
    field_model: "模型",
    field_system: "系统提示词",
    field_lang: "界面语言",
    hint_base: "如 https://api.example.com/v1",
    hint_key: "sk-...",
    hint_model: "如 gpt-4o-mini",
    hint_system: "如：你是游戏攻略助手，答案要具体到数值与时机",
    idle: "就绪",
    busy: "正在检索…",
    need_config: "请先在设置里填写接口地址、API Key 与模型",
    saved: "已保存",
    testing: "正在测试连接…",
    test_ok: "连接正常",
    empty_question: "先输入问题",
    lang_switch: "English",
};

const EN: Strings = Strings {
    title: "Game Buddy",
    ask_placeholder: "Ask anything: how do I beat this boss?",
    send: "Ask",
    settings: "Settings",
    back: "Back",
    save: "Save",
    test: "Test",
    field_base: "Endpoint",
    field_key: "API key",
    field_model: "Model",
    field_system: "System prompt",
    field_lang: "Language",
    hint_base: "e.g. https://api.example.com/v1",
    hint_key: "sk-...",
    hint_model: "e.g. gpt-4o-mini",
    hint_system: "e.g. You are a game strategy assistant; be concrete about numbers and timing",
    idle: "Ready",
    busy: "Retrieving…",
    need_config: "Set the endpoint, API key and model in Settings first",
    saved: "Saved",
    testing: "Testing connection…",
    test_ok: "Connection OK",
    empty_question: "Type a question first",
    lang_switch: "中文",
};

/// 按语言取文案；非 `en` 一律中文。
pub fn strings(lang: &str) -> &'static Strings {
    if lang.eq_ignore_ascii_case("en") {
        &EN
    } else {
        &ZH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 语言回落中文() {
        assert_eq!(strings("en").send, "Ask");
        assert_eq!(strings("zh").send, "提问");
        assert_eq!(strings("").send, "提问");
        assert_eq!(strings("EN").send, "Ask");
    }
}
