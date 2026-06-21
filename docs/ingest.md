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

### 8. Update nightly clean-fix config

`~/.claude/scripts/clean-fix/clean-fix.conf` drives the nightly clean/build/mend and style-eval/fix flow. Paths are relative to `~/rust/`. Two independent allowlists, with **different granularity for workspaces**:

- `[build]` — directories to clean/build/mend. A workspace is listed by its **root** (one entry; the whole workspace shares one `target/`). Add the host workspace root if it isn't already present:

  ```
  bevy_hana
  ```

- `[targets]` — directories to style eval/review/fix. Workspace members are listed **individually** as `<workspace>/crates/<member>` (not the workspace root). Add the new crate as an active entry, alongside the other `bevy_hana/crates/*` members:

  ```
  bevy_hana/crates/<crate_name>
  ```

  Point members at the **primary** `bevy_hana` workspace, not the `bevy_hana_bevy_update` worktree — the primary is the canonical style target. Prefix a line with `#CLEAN_FIX_SKIP# ` only if you deliberately want to register a member but hold it out of the style flow.

Do **not** delete the standalone repo's own entries here yet — `[build] <crate_name>` and any `<crate_name>_bevy_update` rows still point at live checkouts until step 11. Removing them is part of repo retirement.

### 9. Checkpoint — summarize and confirm before retiring the standalone repo

Everything through step 8 is in-tree and reversible. Steps 10–11 are outward-facing (a push plus a GitHub archive) and destructive (a local delete), so stop here:

- Summarize what landed — crate at `crates/<crate_name>/` with history preserved, manifest on the workspace pattern, dep switched to path, build/examples/tests/smoke-test results, clean-fix.conf enrollment.
- Ask the user whether they are ready to retire the standalone repo: the `README.md` pointer commit + GitHub archive (step 10) and the local checkout deletion (step 11).
- Proceed past this point only on an explicit go-ahead.

### 10. Archive the standalone repo

- Push a final pointer commit on the standalone repo's `README.md`: "moved to `bevy_hana/crates/<crate_name>/`".
- Archive `github.com/<owner>/<crate_name>` via the GitHub UI.
- Stop pushing to `<source_repo_path>`. Future commits land in `bevy_hana`.

### 11. Remove the local standalone checkout

Only after everything above is done — workspace builds/tests green, `git log --follow` resolves the full history in-tree, and the GitHub repo is archived — delete the local source clone:

```bash
rm -rf <source_repo_path>
```

`git subtree add` does not transfer the source repo's reflog or tags, so `<source_repo_path>` is the only copy of those until it's archived. Do not remove it before step 10 completes.

Then prune the now-dead standalone entries from `~/.claude/scripts/clean-fix/clean-fix.conf` (added back in step 8's note): remove `[build] <crate_name>` plus any `<crate_name>_bevy_update` rows in `[build]`/`[targets]`. The crate's nightly coverage now comes from the `bevy_hana` workspace-root `[build]` entry and the `bevy_hana/crates/<crate_name>` `[targets]` entry.

## Definition of done

- `crates/<crate_name>/` exists with full git history (`git log --follow` works; `git blame` resolves through the move).
- Per-crate `Cargo.toml` matches the workspace pattern; inner `Cargo.lock`/`taplo.toml`/`rustfmt.toml`/`.github/` removed.
- Bevy feature set minimized; workspace builds + examples + tests all green.
- Workspace dep entry is `{ path = "crates/<crate_name>" }`; consumers use `workspace = true`.
- Nightly `clean-fix.conf` covers the crate: `bevy_hana` in `[build]` (workspace root) and `bevy_hana/crates/<crate_name>` in `[targets]` (per-member).
- Standalone GitHub repo archived with pointer commit on README.
- Local standalone checkout (`<source_repo_path>`) removed once history is confirmed in-tree and the repo is archived.
