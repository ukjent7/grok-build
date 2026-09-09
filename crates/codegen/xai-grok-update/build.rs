fn main() {
    // FORK(byok): BYOK_RELEASE is stamped into the binary as its update
    // identity. It is written into a generated source file instead of
    // option_env!: a tag-only change must alter the compiled sources,
    // or cargo/sccache would reuse an object stamped with the previous tag.
    println!("cargo:rerun-if-env-changed=BYOK_RELEASE");
    println!("cargo:rerun-if-env-changed=GROK_BYOK_REPO");
    let decl = match std::env::var("BYOK_RELEASE").ok().filter(|v| !v.is_empty()) {
        Some(tag) => format!("pub const RELEASE: Option<&str> = Some({tag:?});"),
        None => "pub const RELEASE: Option<&str> = None;".to_string(),
    };
    let out = std::path::PathBuf::from(
        std::env::var("OUT_DIR").expect("OUT_DIR is always set by cargo"),
    )
    .join("byok_release.rs");
    std::fs::write(out, decl).expect("write byok_release.rs");
}
