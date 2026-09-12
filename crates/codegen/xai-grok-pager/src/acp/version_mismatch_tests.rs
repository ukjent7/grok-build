use super::{is_version_mismatch_banner, version_mismatch_banner, VERSION_MISMATCH_MARKER_ZH};
use crate::glyphs::sanitize_toast_message;
use crate::locale::{LocaleContext, LocaleSource, ResolvedLocale, UiLocale};

fn zh_cn() -> LocaleContext {
    LocaleContext::new(ResolvedLocale {
        locale: UiLocale::ZhCn,
        source: LocaleSource::ProductDefault,
    })
}

fn expected_banner(client: &str, leader: &str) -> String {
    sanitize_toast_message(&format!(
        "⚠ Version mismatch: client {client}, leader {leader}. Restart grok to match"
    ))
    .into_owned()
}

#[test]
fn formats_both_versions_and_ignores_wire_message() {
    assert_eq!(
        version_mismatch_banner(
            r#"{"clientVersion":"0.1.157","leaderVersion":"0.1.150","message":"ignore me"}"#
        ),
        Some(expected_banner("0.1.157", "0.1.150"))
    );
}

#[test]
fn formats_without_message_field() {
    assert_eq!(
        version_mismatch_banner(r#"{"clientVersion":"0.2.1","leaderVersion":"0.2.0"}"#),
        Some(expected_banner("0.2.1", "0.2.0"))
    );
}

#[test]
fn rejects_unusable_payloads() {
    for params in [
        "{}",
        r#"{"message":"only a message\nwith\nnewlines"}"#,
        r#"{"clientVersion":"0.1.157"}"#,
        r#"{"leaderVersion":"0.1.150"}"#,
        r#"{"clientVersion":"","leaderVersion":"0.1.150"}"#,
        r#"{"clientVersion":"0.1.157","leaderVersion":""}"#,
        r#"{"clientVersion":"\n\t","leaderVersion":"0.1.150"}"#,
        r#"{"clientVersion":"   ","leaderVersion":"0.1.150"}"#,
        r#"{"clientVersion":"0.1.157","leaderVersion":"\n\t"}"#,
        r#"{"clientVersion":1,"leaderVersion":"0.1.150"}"#,
        r#""not-an-object""#,
        "null",
        "",
        "[]",
    ] {
        assert_eq!(version_mismatch_banner(params), None, "{params}");
    }
}

#[test]
fn scrubs_control_chars_in_versions() {
    let text = version_mismatch_banner(
        r#"{"clientVersion":"0.1.157\n\u0007x","leaderVersion":"0.1.150\r\n"}"#,
    )
    .expect("toastable after scrub");
    assert_eq!(text, expected_banner("0.1.157  x", "0.1.150  "));
    assert!(
        !text.chars().any(char::is_control),
        "control chars must not reach toast: {text:?}"
    );
}

#[test]
fn full_banner_matches_sanitize_toast_message() {
    let text = version_mismatch_banner(r#"{"clientVersion":"0.1.157","leaderVersion":"0.1.150"}"#)
        .expect("banner");
    assert_eq!(text, expected_banner("0.1.157", "0.1.150"));
    assert!(
        is_version_mismatch_banner(&text),
        "marker must survive glyph fallback: {text:?}"
    );
    assert!(is_version_mismatch_banner(
        "! Version mismatch: client x, leader y"
    ));
}

/// The catalog translation and [`VERSION_MISMATCH_MARKER_ZH`] are two halves of one
/// contract: `is_version_mismatch_banner` only recognizes the translated banner while
/// the catalog still carries that exact marker. Rewording the catalog entry without
/// updating the constant would silently disable detection, so pin them together here.
#[test]
fn chinese_catalog_keeps_the_detection_marker() {
    let ctx = zh_cn();
    let banner = ctx.named_text(
        "acp.version_mismatch",
        "⚠ Version mismatch: client {client_version}, leader {leader_version}. Restart grok to match",
    );
    assert!(
        banner.contains(VERSION_MISMATCH_MARKER_ZH),
        "zh-CN banner must keep {VERSION_MISMATCH_MARKER_ZH:?} or it stops being detectable: {banner:?}"
    );
}

#[test]
fn detects_the_localized_banner() {
    let localized = sanitize_toast_message(&format!(
        "⚠ {VERSION_MISMATCH_MARKER_ZH} client 0.1.157, leader 0.1.150"
    ))
    .into_owned();
    assert!(
        is_version_mismatch_banner(&localized),
        "localized banner must still be detected after glyph sanitization: {localized:?}"
    );
}
