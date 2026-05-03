# Phase 1: Ingest `bevy_lagrange` into the `bevy_hana` workspace

**Status:** plan, not yet implemented.
**Owner:** natepiano.

## Goal

Move `bevy_lagrange` from `~/rust/bevy_lagrange` into `crates/bevy_lagrange/` alongside `crates/bevy_diegetic/`. Pure relocation — no API changes, no version bump, no publishing. Subsequent work happens in-tree.

## Steps

### 1. Pre-flight the source repo

`cd ~/rust/bevy_lagrange`, confirm clean working tree on `main` and in sync with origin.

### 2. Ingest with history preserved

From `~/rust/bevy_hana`:

```bash
git subtree add --prefix=crates/bevy_lagrange ~/rust/bevy_lagrange main
```

No `--squash`. After ingest, `git log --follow crates/bevy_lagrange/src/lib.rs` shows the full original history; `git blame` works inside the moved files. Tags from the source repo do not transfer (intentional — many of them refer to a prior project's history).

### 3. Post-subtree cleanup

The subtree pulls in files that conflict with workspace-level config. Delete:

- `crates/bevy_lagrange/Cargo.lock` (workspace has one root lock)
- `crates/bevy_lagrange/taplo.toml` (workspace-level wins)
- `crates/bevy_lagrange/rustfmt.toml` (workspace-level wins; diff first if you want to preserve any setting)

Keep `LICENSE-APACHE`, `LICENSE-MIT`, `README.md`, `CHANGELOG.md`, `NOTICE` — these are per-crate.

### 4. Restructure `crates/bevy_lagrange/Cargo.toml` to match `bevy_diegetic`

Match the `bevy_diegetic` pattern: thin per-crate manifest, workspace-shared values via `*.workspace = true`, per-crate-distinct values stay declared per-crate. Concretely:

- `edition.workspace = true`, `license.workspace = true`, `readme.workspace = true`, `[lints] workspace = true`.
- `name`, `version`, `description`, `keywords`, `categories`, `authors`, `homepage`, `repository` stay per-crate (the workspace defaults don't match).
- Move shared dep version declarations (`bevy`, `bevy_egui`, etc.) up into root `[workspace.dependencies]`; per-crate `Cargo.toml` then references them via `dep = { workspace = true, features = [...] }`.
- Remove the dev-dep self-reference (`bevy_lagrange = { path = ".", ... }`) if present — examples and tests inside the crate access it by name automatically.

### 5. Minimize the Bevy feature set on the production path

Audit `rg "use bevy::" crates/bevy_lagrange/src` and translate the union into the smallest `features = [...]` list. Goal: workspace `cargo build` doesn't drag in Bevy's UI/audio/asset/sprite features unless bevy_lagrange uses them. Standalone `bevy_lagrange/Cargo.toml` already enables `bevy_camera`, `bevy_core_pipeline`, `bevy_log`, `bevy_window` — that's the starting list.

### 6. Switch the workspace `bevy_lagrange` entry to a path dep

In the workspace root `Cargo.toml`, replace the registry entry with a path dep:

```toml
bevy_lagrange = { path = "crates/bevy_lagrange" }
```

Consumers (`bevy_diegetic` dev-dep, `fairy_dust` regular dep) keep `bevy_lagrange = { workspace = true }` unchanged.

### 7. Verify

- `cargo build --workspace` green.
- `cargo build --workspace --examples` green (covers bevy_lagrange's own examples plus `bevy_diegetic`'s).
- `cargo nextest run --workspace` green.
- `world_text` example launches and orbits correctly (manual smoke test).

Regenerate `Cargo.lock` and commit.

### 8. Update nightly style config

`~/.claude/scripts/nightly/nightly-rust.conf` currently has `bevy_diegetic=bevy_hana/crates/bevy_diegetic`. Add:

```
bevy_lagrange=bevy_hana/crates/bevy_lagrange
```

So nightly clean/build/style-eval/style-fix runs on both crates in parallel.

### 9. Archive the standalone repo

- Push a final pointer commit on the standalone repo's `README.md`: "moved to `bevy_hana/crates/bevy_lagrange/`".
- Archive `github.com/natepiano/bevy_lagrange` via the GitHub UI.
- Stop pushing to `~/rust/bevy_lagrange`. Future commits land in `bevy_hana`.

## Definition of done

- `crates/bevy_lagrange/` exists with full git history (`git log --follow` works).
- Per-crate `Cargo.toml` matches the `bevy_diegetic` pattern; inner `Cargo.lock`/`taplo.toml`/`rustfmt.toml` removed.
- Bevy feature set minimized; workspace builds + examples + tests all green.
- Workspace dep entry is `{ path = "crates/bevy_lagrange" }`; consumers use `workspace = true`.
- Nightly style config processes `bevy_lagrange` alongside `bevy_diegetic`.
- Standalone GitHub repo archived with pointer commit on README.
