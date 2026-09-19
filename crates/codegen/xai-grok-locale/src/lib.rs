//! Centralized UI localization for Grok Build.
//!
//! Four embedded catalogs back the fork's zh-CN interface, reached through two
//! lookup paths:
//!
//! * **English-keyed** — [`LocaleContext::tr`], [`LocaleContext::tr_static`] and
//!   [`LocaleContext::tr_format`] look the English literal itself up in
//!   `en-to-zh.json`. There is no id to invent or re-baseline, so upstream
//!   rewording simply misses the map and falls back to English. Nearly every
//!   call site takes this path; `scripts/i18n/wraps.jsonl` carries the counts.
//! * **id-keyed** — [`LocaleContext::named_text`], [`LocaleContext::named_static_text`]
//!   and [`LocaleContext::format_named`] resolve a stable id against
//!   `zh-CN-metadata.json` first and then `zh-CN.json` (a small typed catalog
//!   whose ids mirror `en-US.json`). Only about 140 call sites still take this
//!   path: labels whose id is built from a runtime key (settings, slash
//!   commands, tutorial topics, skill catalog entries) plus a handful of
//!   literal ids. Everything else moved to the English-keyed path during the
//!   `tr()` migration.
//!
//! Both paths take the upstream English literal as an explicit fallback, so a
//! missing catalog entry renders English instead of blank. That fallback is why
//! an id-keyed entry with no call site is invisible: nothing complains, and no
//! test can, because the lookup never happens. The migration to `tr()` left it to
//! a manual prune to clear 2882 such entries, so `scripts/i18n/catalog-check.py`
//! now gates reachability in CI and `scripts/i18n/wrap-check.py` gates the wraps
//! and the catalog coverage.
//!
//! How far to trust that gate: `catalog-check.py` also counts an id as reachable
//! when a `format!`/`concat!` template proves its namespace is built at runtime,
//! which clears a whole namespace on one anchor. It used to account for 341 of
//! the catalog's ids. The two namespaces that made up all but 128 of those —
//! `settings.setting.*` and `tutorial.topic.*` — are now checked per id against
//! the settings registry and the tutorial topic list, and the ids that still
//! ride a blanket anchor (`slash.command.*`, `extensions.catalog.skill.*`) are
//! enumerated in `catalog-check.py`'s header.
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
use std::sync::{LazyLock, OnceLock};

const EN_US_SOURCE: &str = include_str!("../locales/en-US.json");
const ZH_CN_SOURCE: &str = include_str!("../locales/zh-CN.json");
const ZH_CN_METADATA_SOURCE: &str = include_str!("../locales/zh-CN-metadata.json");
// English-keyed overlay for migrated call sites. `tr("literal")` looks up the
// English text itself so upstream rewording falls back to English without an
// id to re-baseline. Seeded from high-value sentence templates only;
// fragments and identifiers stay in English by design.
const EN_TO_ZH_SOURCE: &str = include_str!("../locales/en-to-zh.json");

static EN_US: LazyLock<BTreeMap<String, String>> =
    LazyLock::new(|| parse_catalog("en-US", EN_US_SOURCE));
static ZH_CN: LazyLock<BTreeMap<String, String>> =
    LazyLock::new(|| parse_catalog("zh-CN", ZH_CN_SOURCE));
static ZH_CN_METADATA: LazyLock<BTreeMap<String, String>> =
    LazyLock::new(|| parse_catalog("zh-CN metadata", ZH_CN_METADATA_SOURCE));
static EN_TO_ZH: LazyLock<BTreeMap<String, String>> =
    LazyLock::new(|| parse_catalog("en-to-zh", EN_TO_ZH_SOURCE));

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

/// Immutable localization context resolved once at the composition root.
#[derive(Clone, Debug)]
pub struct LocaleContext {
    locale: UiLocale,
}

impl Default for LocaleContext {
    fn default() -> Self {
        Self::ENGLISH
    }
}

impl LocaleContext {
    /// The English context: what a process that never called [`init`] serves, and
    /// what [`english`] hands to a surface that must not follow `[ui].locale`.
    pub const ENGLISH: Self = Self {
        locale: UiLocale::EnUs,
    };

    pub const fn new(locale: UiLocale) -> Self {
        Self { locale }
    }

    pub const fn locale(&self) -> UiLocale {
        self.locale
    }

    /// True when UI copy should come from the Chinese catalogs.
    pub const fn is_zh_cn(&self) -> bool {
        matches!(self.locale, UiLocale::ZhCn)
    }

    /// Catalog value for `id` in the active locale, if any.
    ///
    /// zh-CN consults the metadata catalog first (it is the large upstream-owned
    /// surface); both typed catalogs carry the same key set, pinned by
    /// `catalogs_have_matching_keys_and_placeholders`, so the zh-CN path needs no
    /// extra English lookup.
    fn lookup(&self, id: &str) -> Option<&'static str> {
        let found: Option<&'static String> = if self.is_zh_cn() {
            ZH_CN_METADATA.get(id).or_else(|| ZH_CN.get(id))
        } else {
            EN_US.get(id)
        };
        found.map(String::as_str)
    }

    /// English-keyed catalog value, if any. Under en-US the literal *is* the
    /// translation, so there is nothing to look up.
    ///
    /// This path deliberately never consults the id-keyed catalogs, and
    /// [`Self::lookup`] never consults `EN_TO_ZH`: a `tr` site keys on English
    /// copy, an id site keys on an invented id. Nothing enforces that the two key
    /// spaces stay disjoint, so a metadata id that happened to equal some English
    /// sentence would not be picked up by `tr` -- by design, since `tr` callers can
    /// pass runtime strings that would then translate unpredictably.
    fn en_text(&self, english: &str) -> Option<&'static str> {
        if self.is_zh_cn() {
            EN_TO_ZH.get(english).map(String::as_str)
        } else {
            None
        }
    }

    /// Shared `{placeholder}` substitution behind [`Self::format_named`] and
    /// [`Self::tr_format`], which differ only in which of the two key spaces their
    /// template came from.
    ///
    /// Single pass: each recognized `{name}` is substituted once and the scan
    /// continues *after* the substituted value, so an argument value containing
    /// `{placeholder}`-shaped text is never itself re-substituted. Unknown
    /// `{names}` stay literal; missing arguments remain visible.
    fn expand(template: Cow<'_, str>, arguments: &[(&str, &str)]) -> String {
        let mut output = String::with_capacity(template.len());
        let mut rest = template.as_ref();
        while let Some(open) = rest.find('{') {
            let after_open = &rest[open + 1..];
            let Some(close) = after_open.find('}') else {
                // Unterminated `{`: nothing from here on can be a placeholder.
                output.push_str(rest);
                return output;
            };
            let name = &after_open[..close];
            let end = open + 1 + close + 1;
            match arguments.iter().find(|(argument, _)| *argument == name) {
                Some((_, value)) => {
                    output.push_str(&rest[..open]);
                    output.push_str(value);
                }
                // Unknown name (or a bare `{}`): keep the whole `{name}` literal.
                None => output.push_str(&rest[..end]),
            }
            rest = &rest[end..];
        }
        output.push_str(rest);
        output
    }

    /// Localized display label for a stable setting key. The key itself remains
    /// the canonical config/TOML identifier and is never translated.
    ///
    /// The per-call `format!` is deliberate: the catalogs are flat, and rebuilding
    /// them into a nested index to save one short `String` per rendered
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
        Cow::Borrowed(self.lookup(id).unwrap_or(english))
    }

    /// Static variant for UI metadata stored in structures that borrow their
    /// labels (for example modal shortcut rows). Both built-in catalogs and
    /// the English fallback live for the duration of the process.
    pub fn named_static_text(&self, id: &str, english: &'static str) -> &'static str {
        self.lookup(id).unwrap_or(english)
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
        Self::expand(self.named_text(id, english), arguments)
    }

    /// English-keyed lookup for migrated call sites. No invented id: upstream
    /// rewording misses the map and falls back to `english`, same as `named_*`.
    /// Only sentence templates belong here; fragments and identifiers stay in
    /// English by design (see `en-to-zh.json`).
    pub fn tr<'a>(&self, english: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(self.en_text(english).unwrap_or(english))
    }

    /// Static variant of [`Self::tr`] for labels stored in borrowed structures.
    pub fn tr_static(&self, english: &'static str) -> &'static str {
        self.en_text(english).unwrap_or(english)
    }

    /// English-keyed format with named `{placeholder}` substitution.
    /// Unknown arguments are ignored; missing ones stay visible.
    /// Convention is single-name: the translation must not introduce a
    /// `{name}` the English template lacks (gated in CI). Dropping a name
    /// the English has (e.g. the `{s}` plural suffix, absent in Chinese)
    /// is safe.
    pub fn tr_format(&self, english: &str, arguments: &[(&str, &str)]) -> String {
        Self::expand(self.tr(english), arguments)
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
static DEFAULT_CONTEXT: LocaleContext = LocaleContext::ENGLISH;

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

/// The English context, for a surface that must not follow the UI locale.
///
/// Same instance [`ctx`] falls back to before [`init`]. A caller reaches for this
/// instead of [`ctx`] when its output leaves the TUI for a machine reader — the
/// `grok export` transcript and headless stdout JSON — because `[ui].locale` must
/// not change what a script parses. It is also the value a locale-dependent
/// function takes in tests that pin the English path.
pub fn english() -> &'static LocaleContext {
    &DEFAULT_CONTEXT
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
    fn a_configured_value_resolves_or_falls_back_to_english() {
        // What the composition root actually does with `[ui].locale`: a recognised
        // value wins, and a blank or unsupported one leaves English in place rather
        // than aborting startup. `UiLocale::parse` maps both to `None`.
        let resolve = |raw: &str| LocaleContext::new(UiLocale::parse(raw).unwrap_or_default());
        assert_eq!(resolve("zh-CN").locale(), UiLocale::ZhCn);
        assert_eq!(resolve("zh_CN.UTF-8").locale(), UiLocale::ZhCn);
        for rejected in ["", " ", "fr-FR", "unsupported"] {
            assert_eq!(
                resolve(rejected).locale(),
                UiLocale::EnUs,
                "{rejected:?} must not select a locale"
            );
        }
    }

    #[test]
    fn global_context_falls_back_to_english_catalog_until_initialized() {
        // No `init` call in this process path: ctx() must serve the English
        // default and pass the English fallback through untouched.
        assert_eq!(ctx().locale(), UiLocale::EnUs);
        assert_eq!(
            ctx().named_text("ctx.fallback.probe", "fallback"),
            "fallback"
        );
    }

    #[test]
    fn english_context_ignores_the_ui_locale() {
        // The escape hatch for a surface that must not follow `[ui].locale`: the export
        // transcript and headless stdout JSON. Same instance `ctx()` falls back to.
        assert_eq!(english().locale(), UiLocale::EnUs);
        assert_eq!(english().tr("(no matches)"), "(no matches)");
        assert_eq!(english().named_text("context.tokens", "tokens"), "tokens");
        assert_eq!(
            english().tr_format("({count} matches)", &[("count", "3")]),
            "(3 matches)"
        );
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
    fn zh_cn_and_metadata_key_sets_are_disjoint() {
        // `lookup` consults the metadata catalog first, so an id carried by both
        // catalogs can only ever resolve to the metadata copy: the zh-CN.json
        // entry is unreachable dead weight and silently rots. Fail loudly, naming
        // the offenders.
        let shared: Vec<&str> = ZH_CN
            .keys()
            .filter(|id| ZH_CN_METADATA.contains_key(*id))
            .map(String::as_str)
            .collect();
        assert!(
            shared.is_empty(),
            "ids present in both zh-CN.json and zh-CN-metadata.json (metadata wins, the zh-CN.json copies are unreachable): {shared:?}"
        );
    }

    #[test]
    fn structured_setting_lookup_localizes_display_text_without_touching_identity() {
        let context = LocaleContext::new(UiLocale::ZhCn);
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
        // Mode names stay English by design (they are canonical identifiers also
        // shown in the prompt info-line); only the surrounding sentences translate.
        assert_eq!(
            context.named_text("mode.always_approve.label", "always-approve"),
            "always-approve"
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
    fn expansion_does_not_rescan_substituted_values() {
        // An argument value that itself contains `{placeholder}`-shaped text (a
        // runtime error string, a label copied from elsewhere) must be inserted
        // verbatim, not re-substituted by a later argument.
        let context = LocaleContext::default();
        assert_eq!(
            context.tr_format(
                "{label}: {value}",
                &[("label", "x {value} y"), ("value", "on")]
            ),
            "x {value} y: on"
        );
    }

    #[test]
    fn formatting_preserves_opaque_dynamic_values() {
        let context = LocaleContext::new(UiLocale::ZhCn);
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
    fn english_keyed_lookup_falls_back_without_an_id() {
        let zh = LocaleContext::new(UiLocale::ZhCn);
        assert_eq!(zh.tr("(no matches)"), "（无匹配项）");
        assert_eq!(
            zh.tr_format("({count} matches)", &[("count", "3")]),
            "（3 个匹配项）"
        );
        // Unknown English stays verbatim; English locale never translates.
        assert_eq!(zh.tr("untranslated sentence"), "untranslated sentence");
        assert_eq!(LocaleContext::default().tr("(no matches)"), "(no matches)");
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
            context.format_named(
                "no.such.id",
                "No agents in state `{}`",
                &[("state", "paused")]
            ),
            "No agents in state `{}`"
        );
    }

    #[test]
    fn chinese_composer_and_shortcut_labels_are_catalog_backed() {
        let context = LocaleContext::new(UiLocale::ZhCn);
        assert_eq!(context.tr("Build anything"), "告诉我你想做些什么…");
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
