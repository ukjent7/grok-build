//! `FEATURES` is the source of truth and the operator tables are hand-maintained mirrors with no compile-time check of their own.
//! This test is that check.

use std::path::Path;

use xai_grok_shell::agent::config::FEATURES;

// FORK: docs/internal/ is not carried in the extracted crate subtree, so
// read at runtime and skip when absent instead of include_str! (which would
// fail compilation). Where the docs exist the assertions run unchanged.
fn read_internal_doc(name: &str) -> Option<String> {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/internal")
            .join(name),
    )
    .ok()
}

#[test]
fn every_registered_feature_reaches_the_operator() {
    let (Some(enterprise), Some(env_vars)) = (
        read_internal_doc("25-enterprise.md"),
        read_internal_doc("22-environment-variables.md"),
    ) else {
        eprintln!("skipping: docs/internal not present in this subtree");
        return;
    };
    for spec in FEATURES {
        assert!(
            enterprise.contains(&format!("`{}`", spec.key)),
            "{} has no row in the 25-enterprise.md pinning table",
            spec.key,
        );
        assert!(
            env_vars.contains(&format!("`{}`", spec.env)),
            "{} is undocumented in 22-environment-variables.md",
            spec.env,
        );
    }
}
