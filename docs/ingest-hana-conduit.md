# Ingest `hana_conduit` into the `bevy_hana` workspace

Use this for moving the sibling `../hana_conduit` repo into
`crates/hana_conduit/`.

This is a crate relocation only: preserve history, do not squash, do not make
API changes, and do not retire the standalone repo until the workspace import is
green and the user explicitly approves the retirement step.

## Current source shape

Checked from `/Users/natemccoy/rust/bevy_hana` on 2026-06-22:

- source path: `../hana_conduit`
- branch: `main`
- remote: `https://github.com/natepiano/hana_conduit.git`
- status: clean against `origin/main`
- crate version: `0.1.0`
- standalone cleanup files present: `Cargo.lock`, `rustfmt.toml`,taplo.toml`,
  `.github/workflows/ci.yml`, `.gitignore`
- source-owned files to keep initially: `README.md`, `LICENSE-APACHE`,
  `LICENSE-MIT`, `assets/models/power_plug.glb`, `examples/playground/`,
  `tests/`, and `plan/*.md`

Nothing in the source layout requires a special import path. The only
post-import check that is easy to miss is the playground asset root; see
[Playground asset check](#playground-asset-check).

## 1. Pre-flight

From `~/rust/bevy_hana`:

```bash
git switch main
git pull --ff-only
git status --short

git -C ../hana_conduit switch main
git -C ../hana_conduit pull --ff-only
git -C ../hana_conduit status --short --branch
```

The source status should be clean and on `main...origin/main`. Stop if either
repo has unrelated dirty work.

## 2. Import with history preserved

From `~/rust/bevy_hana`:

```bash
git subtree add --prefix=crates/hana_conduit ../hana_conduit main
```

Do not pass `--squash`. After the import:

```bash
git log --follow crates/hana_conduit/src/lib.rs
```

should show the original crate history.

## 3. Remove standalone repo files

Delete the files that are now workspace-level concerns:

```bash
git rm -r crates/hana_conduit/.github
git rm crates/hana_conduit/.gitignore
git rm crates/hana_conduit/Cargo.lock
git rm crates/hana_conduit/rustfmt.toml
git rm crates/hana_conduit/taplo.toml
```

`crates/hana_conduit/.gitignore` only ignores `/target`, and the workspace root
already has that rule.

Keep `assets/models/power_plug.glb`; the playground example loads it. Keep
`plan/ARCHITECTURE.md` and `plan/CROSS_SECTION_PROFILES.md` for the initial
import. If they should become durable workspace docs, move them later as a
separate docs cleanup.

## 4. Normalize the workspace manifest

Add the crate to root `[workspace.dependencies]`:

```toml
hana_conduit = { path = "crates/hana_conduit" }
```

The root already has the other dependencies `hana_conduit` needs:

- `bevy`
- `bevy_brp_extras`
- `bevy_kana`
- `bevy_lagrange`

Do not expect the existing registry `bevy_kana 0.1.0` package to disappear from
`Cargo.lock` just because `hana_conduit` now uses the workspace `bevy_kana`;
`bevy_brp_extras` still depends on that registry version.

## 5. Normalize `crates/hana_conduit/Cargo.toml`

Convert the package metadata and lint tables to the workspace pattern:

```toml
[package]
authors.workspace    = true
categories           = ["game-development", "rendering"]
description          = "Physics-based 3D cable routing with catenary geometry and automatic pathfinding for Bevy"
edition.workspace    = true
homepage             = "https://github.com/natepiano/hana/tree/main/crates/hana_conduit"
keywords             = ["3d", "bevy", "cable", "catenary", "routing"]
license.workspace    = true
name                 = "hana_conduit"
readme               = "README.md"
repository.workspace = true
version              = "0.1.0"

[lints]
workspace = true

[dependencies]
bevy = { workspace = true, features = [
  "bevy_asset",
  "bevy_gizmos",
  "bevy_mesh",
  "bevy_pbr",
  "bevy_render",
] }
bevy_kana = { workspace = true }

[dev-dependencies]
bevy            = { workspace = true, features = ["default"] }
bevy_brp_extras = { workspace = true }
bevy_lagrange   = { workspace = true, features = ["fit_overlay"] }

[[example]]
name = "playground"
path = "examples/playground/main.rs"
```

Keep `version = "0.1.0"` during the pure ingest. Any version-policy change is a
separate release-management decision.

## 6. Playground asset check

The playground loads:

```rust
asset_server.load("models/power_plug.glb#Scene0")
```

The asset exists at `crates/hana_conduit/assets/models/power_plug.glb` after
ingest. Smoke-test the example from the workspace root:

```bash
cargo run -p hana_conduit --example playground
```

If the model does not load from the workspace root, keep the existing
`WindowPlugin` customization and add an explicit asset folder to that same
`DefaultPlugins` chain:

```rust
DefaultPlugins
    .set(WindowPlugin {
        primary_window: Some(Window {
            title: PLAYGROUND_WINDOW_TITLE.into(),
            ..default()
        }),
        ..default()
    })
    .set(bevy::asset::AssetPlugin {
        file_path: "crates/hana_conduit/assets".into(),
        ..default()
    })
```

Make that change only if the workspace-root smoke test shows the asset lookup is
wrong.

## 7. Refresh the crate README

The ingested `README.md` still points at the standalone repo for CI and clone
instructions. During ingest, update it for the workspace location:

- CI badge points at `https://github.com/natepiano/hana/actions/workflows/ci.yml`
- install/try commands use the workspace shape:

  ```bash
  git clone https://github.com/natepiano/hana.git
  cd hana
  cargo run -p hana_conduit --example playground
  ```

The source README also says "pre-release (0.0.x)" while the manifest is
`0.1.0`; fix that wording while updating the README.

Do not replace the standalone repo README with the "moved" pointer yet. That
belongs to the retirement step after workspace verification.

## 8. Verify

Use the workspace commands from `~/rust/bevy_hana`:

```bash
cargo +nightly fmt --all -- --check
cargo build --workspace
cargo build --workspace --examples
cargo nextest run --workspace
```

Then smoke-test the catenary playground:

```bash
cargo run -p hana_conduit --example playground
```

Regenerate `Cargo.lock` as part of the manifest change and include it in the
import commit.

## 9. Enroll in clean-fix

`~/.claude/scripts/clean-fix/clean-fix.conf` currently has:

- `[build]` already includes `hana_conduit`
- `[build]` already includes `bevy_hana`
- `[projects]` includes the existing `bevy_hana/crates/*` members

During ingest, add the new workspace member under `[projects]`:

```text
bevy_hana/crates/hana_conduit
```

Do not remove the standalone `hana_conduit` build entry yet. Remove it only
when the standalone repo is retired and the local checkout is deleted.

## 10. Stop before repo retirement

After steps 1-9 are complete, stop and summarize:

- subtree import landed at `crates/hana_conduit/`
- standalone config files removed
- manifest normalized to workspace deps
- README updated for the workspace
- playground asset lookup verified or fixed
- build, examples, tests, and playground smoke-test results
- clean-fix project entry added

Proceed to standalone repo retirement only after explicit approval. Retirement
means replacing the standalone repo README with a moved pointer, pushing it,
archiving `natepiano/hana_conduit`, then deleting the local
`../hana_conduit` checkout and removing its standalone clean-fix entries.

## Definition of done

- `git log --follow crates/hana_conduit/src/lib.rs` shows the original history.
- `crates/hana_conduit/Cargo.toml` uses workspace package metadata, lints, and
  dependencies.
- Workspace root has `hana_conduit = { path = "crates/hana_conduit" }`.
- `Cargo.lock` is regenerated.
- `Cargo.lock`, `.github/`, `.gitignore`, `rustfmt.toml`, and `taplo.toml` are
  removed from `crates/hana_conduit/`.
- `cargo +nightly fmt --all -- --check`, `cargo build --workspace`,
  `cargo build --workspace --examples`, and `cargo nextest run --workspace` pass.
- `cargo run -p hana_conduit --example playground` runs and loads the plug
  model.
- `clean-fix.conf` has `bevy_hana/crates/hana_conduit` under `[projects]`.
- Standalone repo archival and local checkout deletion are not done until after
  the separate approval gate.
