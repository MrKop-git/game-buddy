//! 内置文案。
//!
//! 宿主的 `@key` 只能指向它自己的词典（模块没有自带词典的运行时通道），所以模块自有的
//! 每一句文案都必须经 `state` 送到界面，用扁平键绑 `{t_xxx}`——本文件就是这些文案的源头。

/// 一套界面文案。
pub struct Strings {
    pub title: &'static str,
    pub ask_placeholder: &'static str,
    pub send: &'static str,
    pub clear: &'static str,
    pub settings: &'static str,
    pub back: &'static str,
    pub save: &'static str,
    pub test: &'static str,
    pub field_base: &'static str,
    pub field_key: &'static str,
    pub field_model: &'static str,
    pub field_system: &'static str,
    pub field_preset: &'static str,
    pub field_rag: &'static str,
    pub hint_rag: &'static str,
    pub hint_rag_user: &'static str,
    pub hint_rag_pass: &'static str,
    pub hint_rag_topk: &'static str,
    pub sources: &'static str,
    pub retrieval_failed: &'static str,
    pub field_lang: &'static str,
    pub hint_base: &'static str,
    pub hint_key: &'static str,
    pub hint_model: &'static str,
    pub hint_system: &'static str,
    pub idle: &'static str,
    pub busy: &'static str,
    pub need_config: &'static str,
    pub need_grant: &'static str,
    pub saved: &'static str,
    pub testing: &'static str,
    pub test_ok: &'static str,
    pub empty_question: &'static str,
    pub cleared: &'static str,
    pub lang_switch: &'static str,
    pub err_auth: &'static str,
    pub err_not_found: &'static str,
    pub err_rate: &'static str,
    pub err_server: &'static str,
    pub err_timeout: &'static str,
    pub err_too_large: &'static str,
    /// 未配置时使用的三个快捷提问默认值。
    pub default_presets: [&'static str; 3],
}

const ZH: Strings = Strings {
    title: "游戏助手",
    ask_placeholder: "问点什么：这个 Boss 怎么打？",
    send: "提问",
    clear: "清空",
    settings: "设置",
    back: "返回",
    save: "保存",
    test: "测试连接",
    field_base: "接口地址",
    field_key: "API Key",
    field_model: "模型",
    field_system: "系统提示词",
    field_preset: "快捷提问（留空则该按钮隐藏）",
    field_rag: "知识库检索（留空则不检索）",
    hint_rag: "如 https://gw.mrkop.com/wow-rag/v1/retrieve",
    hint_rag_user: "Basic 用户名",
    hint_rag_pass: "Basic 口令",
    hint_rag_topk: "每次取几条片段",
    sources: "参考来源",
    retrieval_failed: "知识库检索失败",
    field_lang: "界面语言",
    hint_base: "如 https://api.example.com/v1",
    hint_key: "sk-...",
    hint_model: "如 gpt-4o-mini",
    hint_system: "如：你是游戏攻略助手，答案要具体到数值与时机",
    idle: "就绪",
    busy: "正在检索…",
    need_config: "请先在设置里填写接口地址、API Key 与模型",
    need_grant: "缺少能力授权：请到「设置 → 插件页 → 模块授权」给本模块授予 store.* 与 http.*",
    saved: "已保存",
    testing: "正在测试连接…",
    test_ok: "连接正常",
    empty_question: "先输入问题",
    cleared: "已清空对话",
    lang_switch: "English",
    err_auth: "鉴权被拒：API Key 可能无效；若宿主版本较旧，也可能是不支持 Authorization 请求头",
    err_not_found: "地址可能不对：接口地址应填到 /v1 这一级",
    err_rate: "请求过于频繁或额度不足，稍后再试",
    err_server: "服务端错误，稍后再试",
    err_timeout: "请求超时（宿主上限 10 秒）：换更快的模型，或把问题拆小",
    err_too_large: "响应超过宿主 2 MiB 上限",
    default_presets: ["这个 BOSS 怎么打？", "这个道具/机制有什么用？", "接下来该做什么？"],
};

const EN: Strings = Strings {
    title: "Game Buddy",
    ask_placeholder: "Ask anything: how do I beat this boss?",
    send: "Ask",
    clear: "Clear",
    settings: "Settings",
    back: "Back",
    save: "Save",
    test: "Test",
    field_base: "Endpoint",
    field_key: "API key",
    field_model: "Model",
    field_system: "System prompt",
    field_preset: "Quick questions (empty hides the button)",
    field_rag: "Knowledge base (empty disables retrieval)",
    hint_rag: "e.g. https://gw.mrkop.com/wow-rag/v1/retrieve",
    hint_rag_user: "Basic auth user",
    hint_rag_pass: "Basic auth password",
    hint_rag_topk: "Chunks per query",
    sources: "Sources",
    retrieval_failed: "Knowledge base lookup failed",
    field_lang: "Language",
    hint_base: "e.g. https://api.example.com/v1",
    hint_key: "sk-...",
    hint_model: "e.g. gpt-4o-mini",
    hint_system: "e.g. You are a game strategy assistant; be concrete about numbers and timing",
    idle: "Ready",
    busy: "Retrieving…",
    need_config: "Set the endpoint, API key and model in Settings first",
    need_grant: "Missing capability grant: open Settings → Plugins → Module authorization and grant store.* and http.*",
    saved: "Saved",
    testing: "Testing…",
    test_ok: "Connection OK",
    empty_question: "Type a question first",
    cleared: "Conversation cleared",
    lang_switch: "中文",
    err_auth: "Rejected: the API key may be invalid, or this host build cannot send the Authorization header",
    err_not_found: "Check the endpoint: it should include /v1",
    err_rate: "Rate limited or out of quota; try again later",
    err_server: "Server error; try again later",
    err_timeout: "Request timed out (host cap is 10s): use a faster model or split the question",
    err_too_large: "Response exceeds the host's 2 MiB cap",
    default_presets: ["How do I beat this boss?", "What does this item do?", "What should I do next?"],
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

    #[test]
    fn 两套文案都不为空() {
        for s in [&ZH, &EN] {
            assert!(!s.need_grant.is_empty());
            assert!(!s.err_auth.is_empty());
            assert!(!s.clear.is_empty());
        }
    }
}
