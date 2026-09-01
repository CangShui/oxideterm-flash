use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use serde_json::Value;

const EN_PARTS: &[&str] = &[
    include_str!("../locales/en/common.json"),
    include_str!("../locales/en/menu.json"),
    include_str!("../locales/en/sidebar.json"),
    include_str!("../locales/en/settings.json"),
    include_str!("../locales/en/settings_view.json"),
    include_str!("../locales/en/sessionManager.json"),
    include_str!("../locales/en/modals.json"),
    include_str!("../locales/en/connections.json"),
    include_str!("../locales/en/profiler.json"),
    include_str!("../locales/en/forwards.json"),
    include_str!("../locales/en/sftp.json"),
    include_str!("../locales/en/ssh.json"),
    include_str!("../locales/en/terminal.json"),
    include_str!("../locales/en/mosh.json"),
    include_str!("../locales/en/launcher.json"),
    include_str!("../locales/en/graphics.json"),
];
const ZH_CN_PARTS: &[&str] = &[
    include_str!("../locales/zh-CN/common.json"),
    include_str!("../locales/zh-CN/menu.json"),
    include_str!("../locales/zh-CN/sidebar.json"),
    include_str!("../locales/zh-CN/settings.json"),
    include_str!("../locales/zh-CN/settings_view.json"),
    include_str!("../locales/zh-CN/sessionManager.json"),
    include_str!("../locales/zh-CN/modals.json"),
    include_str!("../locales/zh-CN/connections.json"),
    include_str!("../locales/zh-CN/profiler.json"),
    include_str!("../locales/zh-CN/forwards.json"),
    include_str!("../locales/zh-CN/sftp.json"),
    include_str!("../locales/zh-CN/ssh.json"),
    include_str!("../locales/zh-CN/terminal.json"),
    include_str!("../locales/zh-CN/mosh.json"),
    include_str!("../locales/zh-CN/launcher.json"),
    include_str!("../locales/zh-CN/graphics.json"),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Locale {
    En,
    ZhCn,
}

#[derive(Clone, Debug)]
pub struct I18n {
    locale: Locale,
    fallback_locale: Locale,
    catalogs: Arc<RwLock<HashMap<Locale, LocaleCatalog>>>,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        let i18n = Self {
            locale,
            fallback_locale: Locale::En,
            catalogs: Arc::new(RwLock::new(HashMap::new())),
        };
        // Preload the visible locale and English fallback so the startup UI does
        // not parse every catalog, while the first render has its two hot paths.
        i18n.ensure_catalog(Locale::En);
        i18n.ensure_catalog(locale);
        i18n
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn set_locale(&mut self, locale: Locale) {
        // Locale changes are synchronous in the settings flow; preloading here
        // avoids moving JSON parsing into the next render pass.
        self.ensure_catalog(locale);
        self.locale = locale;
    }

    pub fn t(&self, key: &str) -> String {
        self.catalog_message(self.locale, key)
            .or_else(|| self.catalog_message(self.fallback_locale, key))
            .unwrap_or_else(|| key.to_string())
    }

    fn ensure_catalog(&self, locale: Locale) {
        if self
            .catalogs
            .read()
            .expect("native locale catalog lock poisoned")
            .contains_key(&locale)
        {
            return;
        }
        let mut catalogs = self
            .catalogs
            .write()
            .expect("native locale catalog lock poisoned");
        catalogs
            .entry(locale)
            .or_insert_with(|| LocaleCatalog::from_json_parts(locale_parts(locale)));
    }

    fn catalog_message(&self, locale: Locale, key: &str) -> Option<String> {
        self.ensure_catalog(locale);
        self.catalogs
            .read()
            .expect("native locale catalog lock poisoned")
            .get(&locale)
            .and_then(|catalog| catalog.get(key).map(str::to_string))
    }

    #[cfg(test)]
    fn loaded_catalog_count(&self) -> usize {
        self.catalogs
            .read()
            .expect("native locale catalog lock poisoned")
            .len()
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(Locale::ZhCn)
    }
}

#[derive(Clone, Debug)]
struct LocaleCatalog {
    messages: HashMap<String, String>,
}

impl LocaleCatalog {
    fn from_json_parts(parts: &[&str]) -> Self {
        let mut messages = HashMap::new();
        for source in parts {
            let value: Value =
                serde_json::from_str(source).expect("invalid native locale catalog part");
            flatten_json("", &value, &mut messages);
        }
        Self { messages }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.messages.get(key).map(String::as_str)
    }
}

fn locale_parts(locale: Locale) -> &'static [&'static str] {
    match locale {
        Locale::En => EN_PARTS,
        Locale::ZhCn => ZH_CN_PARTS,
    }
}

fn flatten_json(prefix: &str, value: &Value, messages: &mut HashMap<String, String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let key = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_json(&key, child, messages);
            }
        }
        Value::String(message) => {
            let previous = messages.insert(prefix.to_string(), message.clone());
            assert!(previous.is_none(), "duplicate native locale key: {prefix}");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_active_locale() {
        let mut i18n = I18n::default();
        assert_eq!(i18n.t("menu.new_terminal"), "新建终端");

        i18n.set_locale(Locale::En);
        assert_eq!(i18n.t("menu.new_terminal"), "New Terminal");
    }

    #[test]
    fn falls_back_to_english_then_key() {
        let i18n = I18n::new(Locale::ZhCn);
        assert_eq!(i18n.t("missing.key"), "missing.key");
    }

    #[test]
    fn split_catalogs_keep_expected_domains() {
        let i18n = I18n::new(Locale::ZhCn);
        assert_eq!(i18n.t("ssh.form.title"), "新建连接");
        assert_eq!(i18n.t("sidebar.panels.sessions"), "活动会话");
        assert_eq!(i18n.t("terminal.local_terminal"), "本地终端");
        assert_eq!(i18n.t("terminal.trzsz.completed_title"), "传输已完成");
    }

    #[test]
    fn loads_only_active_locale_and_fallback_until_switch() {
        let mut i18n = I18n::new(Locale::ZhCn);
        assert_eq!(i18n.loaded_catalog_count(), 2);

    }

    #[test]
    #[should_panic(expected = "duplicate native locale key")]
    fn duplicate_keys_are_rejected() {
        let _ = LocaleCatalog::from_json_parts(&[
            r#"{"menu":{"copy":"Copy"}}"#,
            r#"{"menu":{"copy":"Duplicate"}}"#,
        ]);
    }

    #[test]
    fn locale_catalogs_have_the_same_complete_key_set() {
        use std::collections::BTreeSet;

        let locales = [Locale::En, Locale::ZhCn];
        let english_keys: BTreeSet<_> = LocaleCatalog::from_json_parts(EN_PARTS)
            .messages
            .into_keys()
            .collect();

        for locale in locales {
            let localized_keys: BTreeSet<_> = LocaleCatalog::from_json_parts(locale_parts(locale))
                .messages
                .into_keys()
                .collect();
            let missing: Vec<_> = english_keys.difference(&localized_keys).collect();
            let unexpected: Vec<_> = localized_keys.difference(&english_keys).collect();

            assert!(
                missing.is_empty() && unexpected.is_empty(),
                "{locale:?} catalog differs from English; missing: {missing:?}, unexpected: {unexpected:?}"
            );
        }
    }

    #[test]
    fn language_names_are_autonyms_in_every_locale() {
        let expected = [
            ("language.english", "English"),
            ("language.simplified_chinese", "简体中文"),
        ];
        let locales = [Locale::En, Locale::ZhCn];

        for locale in locales {
            let i18n = I18n::new(locale);
            for (key, value) in expected {
                assert_eq!(i18n.t(key), value);
            }
        }
    }
}
