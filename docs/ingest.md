# Ingest an external crate into the `bevy_hana` workspace

A reusable recipe for moving a Cargo crate from a standalone git repo into `crates/<crate_name>/` alongside the existing workspace members. Pure relocation — no API changes, no version bump, no publishing.

Throughout, `<crate_name>` is the name of the incoming crate (e.g. `bevy_lagrange`, `bevy_diegetic`) and `<source_repo_path>` is the path to its standalone git checkout (e.g. `~/rust/<crate_name>`).

## Steps

### 1. Pre-flight the source repo

`cd <source_repo_path>`, confirm clean working tree on `main` and in sync with origin.

### 2. Ingest with history preserved

From `~/rust/bevy_hana`:

```bash
git subtree add --prefix=crates/<crate_name> <source_repo_path> main
```

No `--squash`. After ingest, `git log --follow crates/<crate_name>/src/lib.rs` shows the full original history; `git blame` works inside the moved files. Tags from the source repo do not transfer (intentional — `git subtree add` doesn't import tags by default; new `<crate_name>-X.Y.Z` tags get created in the workspace repo at publish time if you ever publish from here).

### 3. Post-subtree cleanup

The subtree pulls in files that conflict with workspace-level config or are no longer relevant inside a sub-crate. Delete:

- `crates/<crate_name>/Cargo.lock` — workspace has one root lock; per-crate locks inside a workspace are inert and confusing.
- `crates/<crate_name>/taplo.toml` — workspace-level `taplo.toml` wins. Diff first if you want to preserve any setting.
- `crates/<crate_name>/rustfmt.toml` — workspace-level `rustfmt.toml` wins. Diff first if you want to preserve any setting.
- `crates/<crate_name>/.github/` — the standalone repo's CI workflows are inert inside a sub-crate. The workspace's root-level `.github/workflows/ci.yml` is the active CI; everything that used to run for the standalone crate now runs as part of the workspace CI.

Keep `LICENSE-APACHE`, `LICENSE-MIT`, `README.md`, `CHANGELOG.md`, `NOTICE` — these are per-crate files that crates.io shows.

### 4. Restructure `crates/<crate_name>/Cargo.toml` to match the workspace pattern

Match the existing workspace-member pattern (e.g. `bevy_diegetic`): thin per-crate manifest, workspace-shared values via `*.workspace = true`, per-crate-distinct values stay declared per-crate. Concretely:

- `edition.workspace = true`, `license.workspace = true`, `readme.workspace = true`, `[lints] workspace = true`, `authors.workspace = true`.
- `name`, `version`, `description`, `keywords`, `categories`, `homepage`, `repository` stay per-crate (the workspace defaults don't match what each crate advertises on crates.io).
- Move shared dep version declarations (`bevy`, etc.) up into root `[workspace.dependencies]`; per-crate `Cargo.toml` then references them via `dep = { workspace = true, features = [...] }`.
- Remove the dev-dep self-reference (`<crate_name> = { path = ".", ... }`) if present — examples and tests inside the crate access it by name automatically once it's a workspace member.

### 5. Minimize the Bevy feature set on the production path

Audit `rg "use bevy::" crates/<crate_name>/src` and translate the union into the smallest `features = [...]` list. Goal: workspace `cargo build` doesn't drag in Bevy's UI/audio/asset/sprite features unless the crate uses them. Compare against existing workspace members' feature lists as a starting point.

### 6. Switch the workspace `<crate_name>` entry to a path dep

If the workspace had a registry entry for the crate (`<crate_name> = "X.Y.Z"` in `[workspace.dependencies]`), replace it with a path dep:

```toml
<crate_name> = { path = "crates/<crate_name>" }
```

Existing consumers (other workspace crates that already declared `<crate_name> = { workspace = true }`) keep their dep lines unchanged and now resolve to the in-tree crate.

### 7. Verify

- `cargo build --workspace` green.
- `cargo build --workspace --examples` green (covers the new crate's own examples plus existing workspace examples).
- `cargo nextest run --workspace` green.
- Smoke-test any example that exercises the new crate end-to-end.

Regenerate `Cargo.lock` and commit.

### 8. Update nightly style config

`~/.claude/scripts/nightly/nightly-rust.conf` lists every workspace crate the nightly clean/build/style-eval/style-fix flow processes. Add an entry for the new crate, matching the existing `<name>=<workspace-path>` format:

```
<crate_name>=bevy_hana/crates/<crate_name>
```

### 9. Archive the standalone repo

- Push a final pointer commit on the standalone repo's `README.md`: "moved to `bevy_hana/crates/<crate_name>/`".
- Archive `github.com/<owner>/<crate_name>` via the GitHub UI.
- Stop pushing to `<source_repo_path>`. Future commits land in `bevy_hana`.

## Definition of done

- `crates/<crate_name>/` exists with full git history (`git log --follow` works; `git blame` resolves through the move).
- Per-crate `Cargo.toml` matches the workspace pattern; inner `Cargo.lock`/`taplo.toml`/`rustfmt.toml`/`.github/` removed.
- Bevy feature set minimized; workspace builds + examples + tests all green.
- Workspace dep entry is `{ path = "crates/<crate_name>" }`; consumers use `workspace = true`.
- Nightly style config processes `<crate_name>` alongside the existing workspace crates.
- Standalone GitHub repo archived with pointer commit on README.
