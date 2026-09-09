<div align="center">

<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://media.x.ai/v1/website/spacexai-symbol-white-transparent-0c31957f.png">
    <source media="(prefers-color-scheme: light)" srcset="https://media.x.ai/v1/website/spacexai-symbol-black-transparent-6435cf42.png">
    <img alt="SpaceXAI logo" src="https://media.x.ai/v1/website/spacexai-symbol-black-transparent-6435cf42.png" width="96">
  </picture>
  <br>
  Grok Build (<code>grok</code>)
</h1>

**Grok Build** is SpaceXAI's terminal-based AI coding agent. It runs as a
full-screen TUI that understands your codebase, edits files, executes shell
commands, searches the web, and manages long-running tasks — interactively,
headlessly for scripting/CI, or embedded in editors via the Agent Client
Protocol (ACP).

[Installing the released binary](#installing-the-released-binary) ·
[Building from source](#building-from-source) ·
[Documentation](#documentation) ·
[Repository layout](#repository-layout) ·
[Development](#development) ·
[Contributing](#contributing) ·
[License](#license)

![Grok Build TUI](https://media.x.ai/v1/website/universe-tui-screenshot-6f7a0837.png)

**Learn more about Grok Build at [x.ai/cli](https://x.ai/cli)**

This repository contains the Rust source for the `grok` CLI/TUI and its agent
runtime. It is synced periodically from the SpaceXAI monorepo.

A small `SOURCE_REV` file at the root records the full monorepo commit SHA
for the version of the code present in this tree.

</div>

---

## Installing the released binary

Prebuilt binaries are published for macOS, Linux, and Windows:

```sh
curl -fsSL https://x.ai/cli/install.sh | bash   # macOS / Linux / Git Bash
irm https://x.ai/cli/install.ps1 | iex          # Windows PowerShell
grok --version
```

See the [changelog](https://x.ai/build/changelog) for the latest fixes,
features, and improvements in each release.

### BYOK fork builds (this repo)

Prebuilt Windows/macOS binaries with third-party endpoint support,
published as GitHub Releases (`byok-v*` tags):

```powershell
irm https://cdn.jsdelivr.net/gh/ukjent7/grok-build@main/install.ps1 | iex
```

Pinned version (replace `@main` with a tag):

```powershell
irm https://cdn.jsdelivr.net/gh/ukjent7/grok-build@byok-v0.1.2/install.ps1 | iex
```

#### Updates (Ctrl+U / `grok update`)

Fork installs (`installer = "byok"`) track this repo's `byok-v*` releases via
the GitHub API — never the official channel, so `Ctrl+U` can no longer pull
the official build over the fork build. The bare semver after `byok-v` is the
comparable version, which means a new tag triggers an update even when the
upstream Cargo version is unchanged. `grok update --version` accepts
`byok-v0.1.4`, `v0.1.4` and `0.1.4` alike.

One-time note: installs from before this change persist
`installer = "internal"`. Reinstall once with the script above (it rewrites
the marker to `"byok"`); tagged binaries also self-correct without it.

#### Secure defaults for third-party endpoints

The installer seeds these into `~/.grok/config.toml` on a fresh install
(missing keys only — your explicit values always win):

```toml
[features]
web_fetch = true            # real local fetching; the one web tool that works on BYOK
telemetry = false           # product analytics: opt-in on a fork
image_gen = false           # xAI-only Imagine endpoint
video_gen = false           # xAI-only Imagine endpoint
feedback = false            # feedback goes to xAI; useless on a fork

[telemetry]
trace_upload = false        # trace payload upload: opt-in on a fork
```

Search is left off by leaving `[models] web_search` unmapped (no key resolves,
so the tool never registers). Do NOT set a global `disable_web_search = true`
instead — that kill-switch also disables `web_fetch`. Likewise, only point
`[models] web_search` at an endpoint that really executes server-side search;
pointing it at a plain chat gateway gives confident ungrounded answers.

`image_edit` has no `[features]` key upstream (env-only), so the installer
sets User-level `GROK_IMAGE_EDIT=0` instead — again only when you have not
set it yourself. Re-enable later with:

```powershell
[Environment]::SetEnvironmentVariable('GROK_IMAGE_EDIT', '1', 'User')
```

`web_fetch` (real client-side fetching) keeps working; point `[models]
web_search` / `session_summary` at a BYOK-catalog model if you need them.

#### Cutting a release

Merging upstream into `main` never publishes anything (it only runs CI).
To ship, tag the tested commit and push the tag — `BYOK Build` compiles and
attaches the binaries automatically:

```powershell
git tag byok-v0.1.4
git push origin byok-v0.1.4
```

#### Checksums (`checksums/`)

Each tag build publishes per-binary `.sha256` files to the Release and then
commits copies into `checksums/<tag>/` on `main`, plus a `checksums/latest`
pointer holding the newest tag. `install.ps1` reads the checksum from that
tree over jsDelivr (reachable without direct GitHub access) and only falls
back to the release-asset copies. The `latest` pointer advances on semver
only, so backfilling an older tag never moves it backwards.

#### Fork baseline (for the next upstream sync)

Upstream does not accept PRs; this fork carries its patches on top of the
last verified sync point. `SOURCE_REV` records the upstream monorepo SHA but
that object is not in this clone's history — the verifiable base here is the
newest `Synced from monorepo` commit (currently `75810042`). To sync: fetch
the upstream tree read-only, rebase the fork commits above, resolve
`auto_update` / `client` / `config` first, then tag a new `byok-v*`.

## Building from source

Requirements:

- **Rust** — the toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml);
  `rustup` installs it automatically on first build.
- **[DotSlash](https://dotslash-cli.com)** — required so hermetic tools under
  [`bin/`](bin/) (notably [`bin/protoc`](bin/protoc)) can download and run.
  Install it and ensure `dotslash` is on your `PATH` **before** building:

  ```sh
  cargo install dotslash
  # or: prebuilt packages — https://dotslash-cli.com/docs/installation/
  /usr/bin/env dotslash --help   # sanity check
  ```

- **protoc** — proto codegen resolves [`bin/protoc`](bin/protoc) via DotSlash,
  or falls back to a `protoc` on `PATH` / `$PROTOC`.
- macOS and Linux are supported build hosts; Windows builds are best-effort
  and not currently tested from this tree.

```sh
cargo run -p xai-grok-pager-bin              # build + launch the TUI
cargo build -p xai-grok-pager-bin --release  # release binary: target/release/xai-grok-pager
cargo check -p xai-grok-pager-bin            # fast validation
```

The binary artifact is named `xai-grok-pager`; official installs ship it as
`grok`. On first launch it opens your browser to authenticate — see the
[authentication guide](crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md).

## Documentation

Full online documentation is available at
[docs.x.ai/build/overview](https://docs.x.ai/build/overview).

The user guide ships with the pager crate:
[`crates/codegen/xai-grok-pager/docs/user-guide/`](crates/codegen/xai-grok-pager/docs/user-guide/)
— getting started, keyboard shortcuts, slash commands, configuration, theming,
MCP servers, skills, plugins, hooks, headless mode, sandboxing, and more.

## Repository layout

| Path | Contents |
|------|----------|
| `crates/codegen/xai-grok-pager-bin` | Composition-root package; builds the `xai-grok-pager` binary |
| `crates/codegen/xai-grok-pager` | The TUI: scrollback, prompt, modals, rendering |
| `crates/codegen/xai-grok-shell` | Agent runtime + leader/stdio/headless entry points |
| `crates/codegen/xai-grok-tools` | Tool implementations (terminal, file edit, search, ...) |
| `crates/codegen/xai-grok-workspace` | Host filesystem, VCS, execution, checkpoints |
| `crates/codegen/...` | The rest of the CLI crate closure (config, MCP, markdown, sandbox, ...) |
| `crates/common/`, `crates/build/`, `prod/mc/` | Small shared leaf crates pulled in by the closure |
| `third_party/` | Vendored upstream source (Mermaid diagram stack) — see below |

> [!IMPORTANT]
> The root `Cargo.toml` (workspace members, dependency versions, lints,
> profiles) is **generated** — treat it as read-only. Prefer editing per-crate
> `Cargo.toml` files.

## Development

```sh
cargo check -p <crate>        # always target specific crates; full-workspace builds are slow
cargo test -p xai-grok-config # per-crate tests
cargo clippy -p <crate>       # lint config: clippy.toml at the repo root
cargo fmt --all               # rustfmt.toml at the repo root
```

## Contributing

> [!NOTE]
> External contributions are not accepted. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

First-party code in this repository is licensed under the **Apache License,
Version 2.0** — see [`LICENSE`](LICENSE).

Third-party and vendored code remains under its original licenses. See:

- [`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES) — crates.io / git dependencies,
  bundled UI themes, and **in-tree source ports** (including openai/codex and
  sst/opencode tool implementations)
- [`crates/codegen/xai-grok-tools/THIRD_PARTY_NOTICES.md`](crates/codegen/xai-grok-tools/THIRD_PARTY_NOTICES.md)
  — crate-local notice for the codex and opencode ports (license texts +
  Apache §4(b) change notice)
- [`third_party/NOTICE`](third_party/NOTICE) — vendored Mermaid-stack index
