//! Centralized UI localization for Grok Build.
//!
//! Two embedded catalogs back the fork's zh-CN interface:
//!
//! * `en-US.json` / `zh-CN.json` — a small typed catalog (`app.name`,
//!   `trust.question`, `reconnect.*`, …) shared by both locales.
//! * `zh-CN-metadata.json` — the large, upstream-owned metadata catalog
//!   (settings labels, command metadata, picker choices). Its ids are the ones
//!   the UI code already uses, so no parallel id space had to be invented.
//!
//! Consumers call [`LocaleContext::named_text`] / [`LocaleContext::named_static_text`]
//! with the upstream English literal as an explicit fallback. A missing catalog
//! entry therefore renders English instead of blank — which also means a dropped
//! translation is invisible at runtime, so `scripts/i18n/wrap-check.py` gates both
//! the wraps and the catalog coverage in CI.
//!
//! # Process-wide context
//!
//! The context is resolved once at the composition root and published through
//! [`init`]; render paths read it via [`ctx`], which falls back to the English
//! catalog when the process never initialized a locale.
//!
//! [`ctx`] is process-wide (a `OnceLock`), so a test in this process cannot switch
//! the locale behind a call site's back. A call site whose *behavior* depends on the
//! active locale must therefore take a `&LocaleContext` parameter instead of reaching
//! for [`ctx`] itself — see `xai-grok-pager`'s `reconnect_success_hides_mismatch` —
//! so the zh-CN path can be exercised with [`LocaleContext::new`] in a unit test.

use std::borrow::Cow;
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;
use std::fmt;
use std::sync::{LazyLock, OnceLock};

const EN_US_SOURCE: &str = include_str!("../locales/en-US.json");
const ZH_CN_SOURCE: &str = include_str!("../locales/zh-CN.json");
const ZH_CN_METADATA_SOURCE: &str = include_str!("../locales/zh-CN-metadata.json");

static EN_US: LazyLock<BTreeMap<String, String>> =
    LazyLock::new(|| parse_catalog("en-US", EN_US_SOURCE));
static ZH_CN: LazyLock<BTreeMap<String, String>> =
    LazyLock::new(|| parse_catalog("zh-CN", ZH_CN_SOURCE));
static ZH_CN_METADATA: LazyLock<BTreeMap<String, String>> =
    LazyLock::new(|| parse_catalog("zh-CN metadata", ZH_CN_METADATA_SOURCE));

fn parse_catalog(name: &str, source: &str) -> BTreeMap<String, String> {
    serde_json::from_str(source)
        .unwrap_or_else(|error| panic!("invalid built-in {name} locale catalog: {error}"))
}

/// UI locales shipped by the build.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum UiLocale {
    #[default]
    EnUs,
    ZhCn,
}

impl UiLocale {
    pub const fn as_bcp47(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::ZhCn => "zh-CN",
        }
    }

    /// Canonicalize common BCP-47 and POSIX spellings.
    ///
    /// This deliberately does not reuse the voice/STT language catalog: that
    /// catalog has different supported languages and wire semantics.
    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or_default()
            .replace('_', "-")
            .to_ascii_lowercase();
        let primary = normalized.split('-').next().unwrap_or_default();
        match primary {
            "en" => Some(Self::EnUs),
            "zh" => Some(Self::ZhCn),
            _ => None,
        }
    }
}

impl fmt::Display for UiLocale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_bcp47())
    }
}

/// Source that selected the effective locale, in descending precedence.
///
/// Only [`LocaleSource::Config`] and [`LocaleSource::ProductDefault`] are reached
/// today: the composition root (`xai-grok-pager`'s `init_locale_from_config`) feeds
/// [`LocalePreferences::config`] from `[ui].locale` and leaves every other layer
/// unset. The remaining variants keep the reference implementation's precedence
/// contract intact for a future `--locale` / environment / system-detection layer;
/// the resolver tests below pin that ordering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocaleSource {
    Requirement,
    Cli,
    Environment,
    Config,
    ManagedConfig,
    System,
    ProductDefault,
}

/// Inputs for deterministic locale resolution.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalePreferences<'a> {
    pub requirement: Option<&'a str>,
    pub cli: Option<&'a str>,
    pub environment: Option<&'a str>,
    pub config: Option<&'a str>,
    pub managed: Option<&'a str>,
    pub system: Option<&'a str>,
    pub product_default: Option<&'a str>,
}

/// Canonical locale plus the layer that selected it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedLocale {
    pub locale: UiLocale,
    pub source: LocaleSource,
}

impl ResolvedLocale {
    pub fn resolve(preferences: LocalePreferences<'_>) -> Self {
        let candidates = [
            (LocaleSource::Requirement, preferences.requirement),
            (LocaleSource::Cli, preferences.cli),
            (LocaleSource::Environment, preferences.environment),
            (LocaleSource::Config, preferences.config),
            (LocaleSource::ManagedConfig, preferences.managed),
            (LocaleSource::System, preferences.system),
            (LocaleSource::ProductDefault, preferences.product_default),
        ];
        candidates
            .into_iter()
            .find_map(|(source, value)| {
                value
                    .filter(|value| !value.trim().is_empty())
                    .and_then(UiLocale::parse)
                    .map(|locale| Self { locale, source })
            })
            .unwrap_or(Self {
                locale: UiLocale::EnUs,
                source: LocaleSource::ProductDefault,
            })
    }
}

/// Immutable localization context resolved once at the composition root.
#[derive(Clone, Debug)]
pub struct LocaleContext {
    resolved: ResolvedLocale,
}

impl Default for LocaleContext {
    fn default() -> Self {
        Self::new(ResolvedLocale {
            locale: UiLocale::EnUs,
            source: LocaleSource::ProductDefault,
        })
    }
}

impl LocaleContext {
    pub const fn new(resolved: ResolvedLocale) -> Self {
        Self { resolved }
    }

    pub const fn resolved(&self) -> ResolvedLocale {
        self.resolved
    }

    pub const fn locale(&self) -> UiLocale {
        self.resolved.locale
    }

    /// True when UI copy should come from the Chinese catalogs.
    pub const fn is_zh_cn(&self) -> bool {
        matches!(self.resolved.locale, UiLocale::ZhCn)
    }

    /// Catalog value for `id` in the active locale, if any.
    ///
    /// zh-CN consults the metadata catalog first (it is the large upstream-owned
    /// surface); both typed catalogs carry the same key set, pinned by
    /// `catalogs_have_matching_keys_and_placeholders`, so the zh-CN path needs no
    /// extra English lookup.
    fn lookup(&self, id: &str) -> Option<&'static String> {
        if self.is_zh_cn() {
            ZH_CN_METADATA.get(id).or_else(|| ZH_CN.get(id))
        } else {
            EN_US.get(id)
        }
    }

    /// Localized display label for a stable setting key. The key itself remains
    /// the canonical config/TOML identifier and is never translated.
    ///
    /// The per-call `format!` is deliberate: the catalogs are flat, and rebuilding
    /// ~3.5k entries into a nested index to save one short `String` per rendered
    /// settings row would cost more code (and memory) than the allocation.
    pub fn setting_label<'a>(&self, setting_key: &str, english: &'a str) -> Cow<'a, str> {
        self.named_text(&format!("settings.setting.{setting_key}.label"), english)
    }

    /// Localized help text for a stable setting key.
    pub fn setting_description<'a>(&self, setting_key: &str, english: &'a str) -> Cow<'a, str> {
        self.named_text(
            &format!("settings.setting.{setting_key}.description"),
            english,
        )
    }

    /// Localized display label for a setting choice. `canonical` is still the
    /// persisted/wire value; unknown runtime values deliberately fall back to
    /// their original display text.
    pub fn setting_choice_label<'a>(
        &self,
        setting_key: &str,
        canonical: &str,
        english: &'a str,
    ) -> Cow<'a, str> {
        let setting_key = setting_choice_catalog_key(setting_key);
        let canonical = if canonical.is_empty() {
            "_none"
        } else {
            canonical
        };
        self.named_text(
            &format!("settings.setting.{setting_key}.choice.{canonical}.label"),
            english,
        )
    }

    /// Localized explanatory text for a setting choice.
    pub fn setting_choice_description<'a>(
        &self,
        setting_key: &str,
        canonical: &str,
        english: &'a str,
    ) -> Cow<'a, str> {
        let setting_key = setting_choice_catalog_key(setting_key);
        let canonical = if canonical.is_empty() {
            "_none"
        } else {
            canonical
        };
        self.named_text(
            &format!("settings.setting.{setting_key}.choice.{canonical}.description"),
            english,
        )
    }

    /// Look up a catalog entry while retaining an explicit English fallback at the
    /// call site; falls back to `english` when the id is absent from every catalog.
    pub fn named_text<'a>(&self, id: &str, english: &'a str) -> Cow<'a, str> {
        self.lookup(id)
            .map(|value| Cow::Borrowed(value.as_str()))
            .unwrap_or_else(|| Cow::Borrowed(english))
    }

    /// Static variant for UI metadata stored in structures that borrow their
    /// labels (for example modal shortcut rows). Both built-in catalogs and
    /// the English fallback live for the duration of the process.
    pub fn named_static_text(&self, id: &str, english: &'static str) -> &'static str {
        self.lookup(id).map(String::as_str).unwrap_or(english)
    }

    /// Format a metadata-backed template using named placeholders such as
    /// `{provider}`.
    ///
    /// Unknown arguments are ignored. Missing arguments intentionally remain
    /// visible in the returned string so catalog mistakes cannot silently erase
    /// user-visible context. The `english` fallback is never run through positional
    /// formatting, so a bare `{}` in it stays literal — `scripts/i18n/wrap-check.py`
    /// rejects that in CI.
    pub fn format_named(&self, id: &str, english: &str, arguments: &[(&str, &str)]) -> String {
        let mut output = self.named_text(id, english).into_owned();
        for (name, value) in arguments {
            output = output.replace(&format!("{{{name}}}"), value);
        }
        output
    }
}

fn setting_choice_catalog_key(setting_key: &str) -> &str {
    match setting_key {
        "auto_dark_theme" | "auto_light_theme" => "theme",
        "fork_secondary_model" => "default_model",
        other => other,
    }
}

static CONTEXT: OnceLock<LocaleContext> = OnceLock::new();
static DEFAULT_CONTEXT: LocaleContext = LocaleContext::new(ResolvedLocale {
    locale: UiLocale::EnUs,
    source: LocaleSource::ProductDefault,
});

/// Publish the process-wide locale context. Only the first call wins; later
/// calls are ignored so test harnesses cannot race the composition root.
/// Returns `true` when this call installed the context.
pub fn init(context: LocaleContext) -> bool {
    CONTEXT.set(context).is_ok()
}

/// Process-wide localization context. Falls back to the English catalog when
/// [`init`] was never called (tests, tool subprocesses).
pub fn ctx() -> &'static LocaleContext {
    CONTEXT.get().unwrap_or(&DEFAULT_CONTEXT)
}

#[cfg(test)]
fn placeholders(template: &str) -> BTreeSet<&str> {
    let mut result = BTreeSet::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('}') else {
            break;
        };
        let name = &rest[..close];
        if !name.is_empty()
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            result.insert(name);
        }
        rest = &rest[close + 1..];
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one catalog value that is blank on purpose: English pluralizes a count
    /// noun with a trailing `s`, Chinese does not, so the zh-CN dictionary carries
    /// an empty suffix (`views/turn_status.rs` appends it verbatim). Every other
    /// blank value is a translation that failed to load, not a decision.
    const INTENTIONALLY_BLANK_METADATA_IDS: &[&str] = &["turn.watcher.plural"];

    #[test]
    fn canonicalizes_supported_bcp47_and_posix_forms() {
        assert_eq!(UiLocale::parse("en_US.UTF-8"), Some(UiLocale::EnUs));
        assert_eq!(UiLocale::parse("zh-Hans-CN"), Some(UiLocale::ZhCn));
        assert_eq!(UiLocale::parse("ZH_cn"), Some(UiLocale::ZhCn));
        assert_eq!(UiLocale::parse("fr-FR"), None);
    }

    #[test]
    fn requirement_wins_and_invalid_values_fall_through() {
        let resolved = ResolvedLocale::resolve(LocalePreferences {
            requirement: Some("en-US"),
            cli: Some("zh-CN"),
            ..LocalePreferences::default()
        });
        assert_eq!(resolved.locale, UiLocale::EnUs);
        assert_eq!(resolved.source, LocaleSource::Requirement);

        let resolved = ResolvedLocale::resolve(LocalePreferences {
            cli: Some("unsupported"),
            environment: Some("zh_CN.UTF-8"),
            ..LocalePreferences::default()
        });
        assert_eq!(resolved.locale, UiLocale::ZhCn);
        assert_eq!(resolved.source, LocaleSource::Environment);
    }

    #[test]
    fn every_locale_layer_obeys_declared_precedence() {
        let candidates = [
            (LocaleSource::Requirement, "requirement"),
            (LocaleSource::Cli, "cli"),
            (LocaleSource::Environment, "environment"),
            (LocaleSource::Config, "config"),
            (LocaleSource::ManagedConfig, "managed"),
            (LocaleSource::System, "system"),
            (LocaleSource::ProductDefault, "product"),
        ];
        for (selected_index, (expected_source, _)) in candidates.iter().enumerate() {
            let values = candidates.map(|_| Some("unsupported"));
            let mut values = values;
            values[selected_index] = Some("zh-CN");
            let resolved = ResolvedLocale::resolve(LocalePreferences {
                requirement: values[0],
                cli: values[1],
                environment: values[2],
                config: values[3],
                managed: values[4],
                system: values[5],
                product_default: values[6],
            });
            assert_eq!(resolved.locale, UiLocale::ZhCn);
            assert_eq!(resolved.source, *expected_source);
        }
    }

    #[test]
    fn invalid_candidates_fall_back_to_product_default_then_english() {
        let unsupported = LocalePreferences {
            requirement: Some(""),
            cli: Some("fr-FR"),
            environment: Some("unsupported"),
            config: Some(" "),
            managed: Some("de-DE"),
            system: Some("ja-JP"),
            product_default: Some("zh-CN"),
        };
        assert_eq!(
            ResolvedLocale::resolve(unsupported),
            ResolvedLocale {
                locale: UiLocale::ZhCn,
                source: LocaleSource::ProductDefault,
            }
        );
        assert_eq!(
            ResolvedLocale::resolve(LocalePreferences {
                product_default: None,
                ..unsupported
            }),
            ResolvedLocale {
                locale: UiLocale::EnUs,
                source: LocaleSource::ProductDefault,
            }
        );
    }

    #[test]
    fn global_context_falls_back_to_english_catalog_until_initialized() {
        // No `init` call in this process path: ctx() must serve the English
        // default and pass the English fallback through untouched.
        assert_eq!(ctx().locale(), UiLocale::EnUs);
        assert_eq!(ctx().named_text("ctx.fallback.probe", "fallback"), "fallback");
    }

    #[test]
    fn catalogs_have_matching_keys_and_placeholders() {
        let english: BTreeSet<&str> = EN_US.keys().map(String::as_str).collect();
        let chinese: BTreeSet<&str> = ZH_CN.keys().map(String::as_str).collect();
        assert_eq!(english, chinese, "locale catalog key drift");
        for id in english {
            assert_eq!(
                placeholders(EN_US.get(id).unwrap()),
                placeholders(ZH_CN.get(id).unwrap()),
                "placeholder drift for {}",
                id
            );
        }
    }

    #[test]
    fn structured_setting_lookup_localizes_display_text_without_touching_identity() {
        let context = LocaleContext::new(ResolvedLocale {
            locale: UiLocale::ZhCn,
            source: LocaleSource::Cli,
        });
        assert_eq!(
            context.setting_label("compact_mode", "Compact mode"),
            "紧凑模式"
        );
        assert_eq!(
            context.setting_choice_label("permission_mode", "always-approve", "Always approve"),
            "始终批准"
        );
        assert_eq!(
            context.setting_choice_label("default_model", "grok-4.5", "grok-4.5"),
            "grok-4.5"
        );
        assert_eq!(context.named_text("context.tokens", "tokens"), "Token");
        assert_eq!(
            context.named_text("mode.always_approve.label", "always-approve"),
            "始终批准"
        );
    }

    #[test]
    fn structured_metadata_ids_are_non_empty_and_blank_only_by_design() {
        assert!(!ZH_CN_METADATA.is_empty());
        for (id, value) in ZH_CN_METADATA.iter() {
            assert!(!id.trim().is_empty(), "blank metadata id");
            if INTENTIONALLY_BLANK_METADATA_IDS.contains(&id.as_str()) {
                assert!(
                    value.is_empty(),
                    "{id} suppresses an English plural suffix and must stay blank"
                );
                continue;
            }
            assert!(!value.trim().is_empty(), "blank metadata value for {id}");
        }
    }

    #[test]
    fn formatting_preserves_opaque_dynamic_values() {
        let context = LocaleContext::new(ResolvedLocale {
            locale: UiLocale::ZhCn,
            source: LocaleSource::Cli,
        });
        assert_eq!(
            context.format_named(
                "welcome.login_with",
                "Sign in with {provider}",
                &[("provider", "grok.com")],
            ),
            "使用 grok.com 登录"
        );
    }

    #[test]
    fn english_fallback_survives_an_unknown_id_verbatim() {
        let context = LocaleContext::default();
        assert_eq!(
            context.named_static_text("no.such.id", "Fallback copy"),
            "Fallback copy"
        );
        // The fallback is never positionally formatted: a bare `{}` inside it is
        // literal text (wrap-check rejects it in CI) and named arguments that the
        // catalog template does not consume leave the template untouched.
        assert_eq!(
            context.format_named("no.such.id", "No agents in state `{}`", &[("state", "paused")]),
            "No agents in state `{}`"
        );
    }

    #[test]
    fn chinese_composer_and_shortcut_labels_are_catalog_backed() {
        let context = LocaleContext::new(ResolvedLocale {
            locale: UiLocale::ZhCn,
            source: LocaleSource::Cli,
        });
        assert_eq!(
            context.named_text("prompt.placeholder.default", "Build anything"),
            "告诉我你想做些什么…"
        );
        assert_eq!(
            context.named_text("shortcut.clear_search", "clear search"),
            "清除搜索"
        );
        assert_eq!(
            context.named_text("shortcut.switch_tab", "switch tab"),
            "切换标签页"
        );
    }
}
