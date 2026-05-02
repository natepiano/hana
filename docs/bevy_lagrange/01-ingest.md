# Phase 1: Ingest `bevy_lagrange` into the `bevy_hana` workspace

**Status:** plan, not yet implemented.
**Depends on:** nothing (this is the entry phase).
**Unblocks:** [Phase 2](./02-input-refactor.md) (input contract refactor) and [Phase 3](./03-fairy-dust-adoption.md) (fairy_dust adoption).
**Owner:** natepiano.

## Why this phase exists

Today `bevy_lagrange` lives in a standalone git repo at `~/rust/bevy_lagrange` and is consumed from crates.io as version `0.0.3`. Phase 2 wants to make a substantial API change (public `OrbitCamInput` contract, pluggable input sources, lifecycle events). Doing that in the standalone repo means cross-repo round trips for every iteration, no shared workspace lints, no shared nightly style tooling, no atomic refactor across `bevy_lagrange` ↔ `bevy_diegetic` ↔ `fairy_dust`.

Phase 1 moves `bevy_lagrange` into the workspace at `crates/bevy_lagrange/` so all subsequent work happens in-tree. **No API changes happen in this phase** — pure relocation.

## Goals

- Match the existing `bevy_diegetic` workspace pattern: thin per-crate `Cargo.toml`, shared `[workspace.dependencies]`, shared `[workspace.package]`, shared `[workspace.lints]`.
- Trim the Bevy feature set on the production path so workspace builds stay fast.
- Preserve git history so `git blame` and `git log --follow` keep working on the moved files.
- Keep nightly style runs operating on `bevy_lagrange` and `bevy_diegetic` in parallel.

## Steps

### 1. Ingest with history preserved

From `~/rust/bevy_hana`, use `git subtree add` directly against the local path (no named remote needed):

```bash
git subtree add --prefix=crates/bevy_lagrange ~/rust/bevy_lagrange main
```

`git subtree add` (without `--squash`) rewrites every `bevy_lagrange` commit so its paths live under `crates/bevy_lagrange/`, and merges that history into `bevy_hana`. After ingest, `git log --follow crates/bevy_lagrange/src/lib.rs` shows the full original commit history; `git blame` works inside the moved files.

**Tags do not transfer.** The standalone bevy_lagrange repo carries 60+ historical tags, many of which refer to a prior project's history that was reused as a starting point. Tags from the source repo are intentionally not preserved — they don't refer to artifacts in the new repo's namespace. New `bevy_lagrange-X.Y.Z` tags will be created in the `bevy_hana` repo at publish time. Verify post-merge with `git log --follow crates/bevy_lagrange/src/lib.rs` before continuing.

### 2. Restructure `crates/bevy_lagrange/Cargo.toml` to match `bevy_diegetic`

- Strip `[package].edition / license / readme / repository / version` in favor of `*.workspace = true` references where the workspace already provides them.
- Update `[package].repository` to point at the bevy_hana repo URL (the source-of-truth move; see step 7).
- Move shared dependency versions (`bevy`, `bevy_egui`, etc.) up into `[workspace.dependencies]` in the root `Cargo.toml`.
- Replace per-crate `[lints]` with `workspace = true`.
- **Bump `[package].version` from `0.0.3-dev` to `0.0.4-dev`** immediately on ingest — the source repo is at `0.0.3-dev`, and the next published release will be `0.0.4`, so the in-tree version starts there.

### 3. Minimize Bevy feature set on the production path

Audit `bevy_lagrange/src/` for every `use bevy::...` and translate the union into the smallest `features = [...]` list. Compare against `bevy_diegetic`'s feature list as a starting point and trim/extend as needed. Goal: workspace `cargo build` doesn't drag in Bevy's UI/audio/asset/sprite features unless bevy_lagrange genuinely uses them.

**Verification gate** — the trim is not done until all of the following pass:

- `cargo build -p bevy_lagrange --no-default-features` succeeds (catches anything that snuck in via a default).
- `cargo build -p bevy_lagrange --all-features` succeeds (catches feature interactions, especially `fit_overlay` + `bevy_egui`).
- `cargo nextest run --workspace` passes with zero new warnings.
- The `fit_overlay` example still renders gizmos correctly (manual smoke test — no automated gizmo rendering test today).
- The `world_text` example in `bevy_diegetic` still launches and orbits correctly.

Required minimum bevy sub-features (starting list, refine during audit): `bevy_camera`, `bevy_core_pipeline`, `bevy_log`, `bevy_window`, `bevy_render`, `bevy_winit`, `bevy_input`. The `fit_overlay` feature additionally needs `bevy_gizmos`. List the final set explicitly in the per-crate `Cargo.toml` and explain non-obvious entries with a comment.

### 4. Switch the workspace `bevy_lagrange` entry to `path + version`

Replace the registry-only `bevy_lagrange = "0.0.3"` in the workspace root `[workspace.dependencies]` with:

```toml
bevy_lagrange = { path = "crates/bevy_lagrange", version = "0.0.4-dev" }
```

Consumers (`crates/bevy_diegetic/Cargo.toml` dev-dep and `crates/fairy_dust/Cargo.toml` regular dep) keep the `bevy_lagrange = { workspace = true }` form unchanged.

**Version coordination rule:** the per-crate `[package].version` and the workspace-root `version =` constraint must always satisfy each other. Locally during development they are both `0.0.4-dev`. At publish time, both move together to `0.0.4`. If they ever drift (e.g. per-crate at `0.0.4-dev` while workspace constraint says `version = "0.0.3"`), `cargo publish` fails with a confusing version-mismatch error. The workspace dep entry is the single point of truth that consumers see; bump it in lockstep with the per-crate manifest version.

Cargo uses the path during local workspace builds and emits the `version` constraint into the published manifest at `cargo publish` time, so end users from crates.io still get the registry release. This decouples semver per crate while keeping a single source of truth during local development. This is the standard pattern used by bevy itself, tokio, and embassy.

### 5. Verify build + tests green

Run: `cargo build --workspace`, `cargo nextest run --workspace`, and confirm the `world_text` example still launches cleanly. **No behavior changes yet** — Phase 1 is pure relocation, and any divergence here is a bug in the ingestion.

### 6. Update nightly style config

`~/.claude/scripts/nightly/nightly-rust.conf` currently has `bevy_diegetic=bevy_hana/crates/bevy_diegetic`. Add:

```
bevy_lagrange=bevy_hana/crates/bevy_lagrange
```

So the nightly clean/build/style-eval/style-fix flow processes both crates in parallel. Confirm `style-fix-worktrees.sh` and friends pick up the new entry without further changes (they are config-driven).

### 7. Resolve the publication source-of-truth move

Concrete steps:

- **Archive the GitHub repo** at `github.com/natepiano/bevy_lagrange` via the GitHub UI (Settings → "Archive this repository"). This makes it read-only and signals the move to anyone who finds it.
- **Update `[package].repository`** in the in-tree `crates/bevy_lagrange/Cargo.toml` to point at the bevy_hana repo URL. Done as part of step 2.
- **Decide on `0.0.3` yank.** Two options: (a) leave `0.0.3` published (it works, no urgent reason to retract); (b) `cargo yank --version 0.0.3 bevy_lagrange` if the API differences from `0.0.4` are large enough that we want to discourage new adopters from picking it up. **Recommend (a)** — yank is for security/correctness issues, not API churn.
- **Keep the `~/rust/bevy_lagrange` directory on disk for archival.** Stop pushing to its origin. Future commits land in `bevy_hana` only.
- **Per-crate CHANGELOG** — `bevy_lagrange/CHANGELOG.md` stays per-crate (it's part of what crates.io users see). Add an entry at the top: `## [Unreleased] — moved into bevy_hana workspace; will be published as 0.0.4 from there.`

## Out of scope for Phase 1

- API changes — the `OrbitCamInput` refactor, the input-source pluggability, and the interaction events all happen in [Phase 2](./02-input-refactor.md).
- Removing `bevy_egui` or `fit_overlay` features that bevy_lagrange currently exposes. Audit and trim as a separate concern.
- Any `fairy_dust` changes — those land in [Phase 3](./03-fairy-dust-adoption.md).

## Definition of done

- `crates/bevy_lagrange/` exists with full git history reachable via `git log --follow`.
- Per-crate and workspace `Cargo.toml` match the `bevy_diegetic` pattern.
- Bevy feature set is minimized; verification gate passes.
- Workspace dep entry is `{ path = ..., version = "0.0.4-dev" }`; consumers use `workspace = true`.
- `cargo build --workspace` and `cargo nextest run --workspace` green.
- Nightly style config processes `bevy_lagrange` alongside `bevy_diegetic`.
- GitHub repo archived; `Cargo.toml` `repository` field updated; CHANGELOG updated.
