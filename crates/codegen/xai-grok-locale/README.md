# xai-grok-locale

Embedded UI localization for Grok Build. Catalogs are compiled into the binary
via `include_str!`; there is no runtime file dependency.

## Switching languages

Set `[ui] locale = "zh-CN"` in `config.toml` (user config at `$GROK_HOME/config.toml`).
Unset (or any value outside the `en`/`zh` families) keeps the upstream English
interface. The value is resolved once at startup (`xai-grok-pager::init_locale_from_config`)
and published process-wide; render code reads it through `xai_grok_locale::ctx()`.

## Catalog files

| File | Purpose |
|---|---|
| `locales/en-US.json` | Typed catalog (`TextKey`), small, must stay key-identical with `zh-CN.json` |
| `locales/zh-CN.json` | Chinese counterpart of the typed catalog |
| `locales/zh-CN-metadata.json` | Large free-form catalog (`id` → Chinese) addressed by string ids at call sites |

## Call-site patterns

```rust
// expression expecting &str
&crate::locale::ctx().named_text("dashboard.loading_sessions", "Loading sessions…")
// expression expecting String
crate::locale::ctx().named_text("id", "english").into_owned()
// &'static str slots (struct fields, constants)
crate::locale::ctx().named_static_text("id", "english")
// templates with {name} placeholders
crate::locale::ctx().format_named("id", "text {name}", &[("name", &value)])
```

Missing ids fall back to the English original at the call site, so upstream
merges and catalog drift are always safe — worst case a string renders in
English again.

Structured settings labels use the `setting_label` / `setting_description` /
`setting_choice_label` helpers, which derive ids
(`settings.setting.<key>.label`, `.choice.<value>.label`) from the stable
config key; slash command descriptions derive ids
(`slash.command.<canonical>.description`) from the command's canonical name at
the snapshot build site.

## Adding / updating entries

1. Check whether the English text (or a related term) already exists in
   `zh-CN-metadata.json` — reuse that id and translation for consistency.
2. Otherwise append `"semantic.id": "中文"` to `zh-CN-metadata.json`. Keep
   `{name}` placeholders from the English template intact.
3. Do not hard-code Chinese in `.rs` files; the English original stays at the
   call site as fallback and lookup identity, which keeps upstream merges
   minimal.

`locales/pending/` holds per-area fragments from bulk localization runs; they
are merged into `zh-CN-metadata.json` and kept only as a record.

## Tests

`cargo test -p xai-grok-locale` verifies typed-catalog key/placeholder parity
between `en-US` and `zh-CN`, placeholder consistency, and resolution
precedence. (CI runs this; no local toolchain required.)
