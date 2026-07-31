# Crate rename migration — session handoff

Working doc for the `bevy_*` → `hana_*` crate rename effort. Delete when the
migration is finished.

## Completed: bevy_liminal → hana_liminal (2026-07-30)

Published and pushed. Nothing left to do here.

- `hana_liminal 0.1.0` on crates.io; tag `hana_liminal-v0.1.0`; branch
  `release-hana_liminal-0.1.0`; GitHub release created.
- `crates/bevy_liminal/` → `crates/hana_liminal/`; main at `0.2.0-dev`
  (commit `14f6d9c3`, pushed).
- `bevy_liminal 0.0.6` published as a deprecated re-export shim
  (`pub use hana_liminal::*;`, dep `hana_liminal = "0.1"`). Source lives at
  `~/rust/bevy_liminal-shim/`, deliberately outside any repo — crates.io is the
  only other copy. Publish-once, never republish.
- Old versions 0.0.1–0.0.5 left unyanked. Tag `bevy_liminal-v0.0.5` is what the
  shim's `repository`/`homepage` point at.
- Config/doc references updated: `~/.claude/scripts/clean-fix/clean-fix.conf`
  (via `project_rename.py`, which also migrated
  `~/rust/nate_style/.history/hana_liminal.jsonl`),
  `~/.claude/commands/issue.md`, `~/.claude/commands/focused_eval.md`, and the
  hanadocs vault (`issues - liminal.base` filters, issue `project:` frontmatter,
  `release order.md`). Diary entries intentionally left as historical record.

## In progress: nateroids

`~/rust/nateroids`, branch `main`, **uncommitted** (not committed per standing
rule — user must ask):

- `bevy_kana` 0.2.0 → 0.3.0
- `bevy_liminal 0.0.4` → `hana_liminal 0.1.0`
- `src/camera/{mod,game,selection}.rs` use statements

`cargo check` passes. nateroids' own code needed no changes for the kana 0.2→0.3
jump.

**Unresolved:** bevy_kana copies went 2 → 3 because nateroids left
`bevy_clerestory 0.2.0` alone on kana 0.2:

| bevy_kana | pulled by |
|---|---|
| 0.1.0 | `bevy_lagrange 0.2.0` |
| 0.2.0 | `bevy_clerestory 0.2.0` |
| 0.3.0 | `hana_liminal 0.1.0` + nateroids |

No published clerestory or lagrange can reach kana 0.3 (`bevy_clerestory 0.2.0`
and `bevy_lagrange 0.3.0` both cap at `^0.2.0`). Releasing hana_clerestory and
hana_lagrange against kana 0.3.0 collapses nateroids to a single copy — that is
the point of the next phase.

## Completed: bevy_clerestory → hana_clerestory (2026-07-30)

Published and pushed. Nothing left to do here.

- `hana_clerestory 0.3.0` on crates.io; tag `hana_clerestory-v0.3.0`; branch
  `release-hana_clerestory-0.3.0`; GitHub release created. Main at `0.4.0-dev`
  (commit `7b23ea39`, pushed).
- `bevy_clerestory 0.2.1` published as a deprecated re-export shim, source at
  `~/rust/bevy_clerestory-shim/` (outside any repo). All six features forward —
  `monitor-probe` plus `workaround-winit-{3124,4341,4440,4443,4445}` — with the
  dep declared `default-features = false` and the shim's own `default` listing
  the five workarounds, so upstream defaults are reproduced exactly.
- README compatibility table split (`hana_clerestory 0.3` vs
  `bevy_clerestory 0.1 – 0.2`); feature example bumped to `"0.3"`.
- `.claude/config/release.toml` gained a second `[[publish_path_pins]]` entry,
  `hana_clerestory = 0.3.0` — root `Cargo.toml` declares it path-only, which
  `cargo publish` rejects, so the hana_lagrange release needs this pin.
- `clean-fix.conf` renamed via `project_rename.py` (both `[build]` and
  `[projects]`, plus style-history keys).

## Next: bevy_lagrange → hana_lagrange

Local version `bevy_lagrange 0.4.0-dev` → release as 0.4.0; shim
`bevy_lagrange 0.3.1`. Continue the number (matches clerestory and hana_liminal).

**In-workspace dependents to update:**

- `bevy_lagrange` ← `hana_conduit`, `hana_lading`, `hana_prosody`,
  `hana_diegetic`, `hana_valence`, `fairy_dust`; dev-deps in `bevy_kana` and
  `hana_liminal`
- `bevy_clerestory` ← `bevy_lagrange`, `fairy_dust`, `hana_diegetic`; dev-dep in
  `hana_liminal`

**Decision made:** shims for both old names, *with feature forwarding*. This is
the new work vs hana_liminal, which had no `[features]` table at all:

- `bevy_lagrange` shim must re-declare `fit_overlay` →
  `["hana_lagrange/fit_overlay"]`. Requested by `hana_conduit`,
  `hana_diegetic`, `hana_liminal` dev-dep, and nateroids.
- `bevy_clerestory` shim must mirror `monitor-probe` plus five
  `workaround-winit-{3124,4341,4440,4443,4445}` features, and reproduce
  `default = [...]` exactly — a miss changes downstream behavior silently
  instead of failing to compile.

**Shim sources go outside the repo**, same as `~/rust/bevy_liminal-shim/`.

## Blocked / deferred: bevy_kana rename

User initially chose to rename kana in the same pass, then flagged a conflict.
Verified: worktree `~/rust/bevy_hana_rubric`, branch `feature/rubric` (through
phase 9, latest `e1b72761`) deletes 391 lines from `crates/bevy_kana/src/input/`
— `action.rs`, `bind_action_system.rs`, `event.rs`, `keybindings.rs`,
`platform_shortcut_mode.rs`, plus `mod.rs`/`lib.rs`/`prelude.rs`/`Cargo.toml`
edits. That subsystem moves into `crates/hana_rubric`.

Renaming bevy_kana now moves every one of those files and guarantees merge
conflicts with that branch.

**DECIDED (user confirmed): bevy_kana is dropped from this pass.** Release
clerestory and lagrange against the already-published `bevy_kana 0.3.0`. Revisit
the kana rename after `feature/rubric` merges and kana's file layout settles.

Related, and also confirmed as the plan: nateroids requests
`bevy_kana = { features = ["input"] }` — the exact feature `feature/rubric`
removes. When that branch lands, nateroids moves to `hana_rubric` for input.

## Mechanics reference (from the hana_liminal run)

1. `git mv crates/<old> crates/<new>`, then replace the crate name across the
   directory. Watch for: wgsl `#define_import_path` / `#import` pairs, tracing
   target string constants, doc-comment `use` examples, README badge URLs,
   `homepage` field. Root `Cargo.toml` needs no edit — `members = ["crates/*"]`
   is a glob.
2. Add a README rename banner and a CHANGELOG `[Unreleased] → Changed` entry
   naming what does *not* carry through a re-export: reflected `TypePath`
   strings and shader import paths.
3. `cargo build -p <crate> --all-targets` (unsandboxed — hana_prosody's speech
   crate build script dies under the sandbox), then `cargo +nightly fmt`.
4. Release via `/release <crate> <version>` — **not** raw `cargo publish`. Root
   `bevy_kana = { path = ... }` is path-only, which crates.io rejects;
   `.claude/config/release.toml` `[[publish_path_pins]]` rewrites it to
   `bevy_kana = "0.3.0"` on the throwaway release branch only. Requires a clean
   tree (`pre_release_checks.sh` runs `git status`).
5. Publish the shim afterward with
   `cargo publish --allow-dirty --manifest-path <shim>/Cargo.toml`, once the new
   crate is on the index. Shim dep uses a caret req (`"0.1"`), never `=` — a hard
   pin makes cargo fail to unify for anyone mid-migration who depends on both
   names.
6. Update `clean-fix.conf` via
   `python3 ~/.claude/scripts/clean-fix/project_rename.py <old> <new>` — never by
   hand; `[projects]` entries are style-history keys.

## Immediate next step

**bevy_lagrange → hana_lagrange. Rename DONE and uncommitted on `main`
(258 files); waiting on commit approval. Steps 1–4 below are complete.**

1. ~~`git mv crates/bevy_lagrange crates/hana_lagrange` and
   `git mv docs/bevy_lagrange docs/hana_lagrange`.~~ DONE
2. ~~Blanket `bevy_lagrange` → `hana_lagrange` across the repo, excluding
   `Cargo.lock` (cargo regenerates) and this handoff doc (its prose names the
   old crate deliberately).~~ DONE — 195 files rewritten via `sed -i ''`
   (needed `dangerouslyDisableSandbox`).
3. ~~README rename banner + compatibility-table split, CHANGELOG
   `[Unreleased] → Changed` entry naming the `TypePath` caveat.~~ DONE
4. ~~`cargo build --workspace --all-targets` (unsandboxed) then
   `cargo +nightly fmt`.~~ DONE — BUILD_EXIT=0, FMT_EXIT=0.
   `rg -l bevy_lagrange -g '!Cargo.lock'` returns exactly three files: this
   doc, `crates/hana_lagrange/README.md`, `crates/hana_lagrange/CHANGELOG.md`.
5. **← YOU ARE HERE.** Commit (ask first — standing rule), then
   `/release hana_lagrange 0.4.0`.
6. Shim at `~/rust/bevy_lagrange-shim/`, version `0.3.1`, modeled on
   `~/rust/bevy_clerestory-shim/`: `pub use hana_lagrange::*;`, dep
   `hana_lagrange = "0.4"`, and the `fit_overlay` feature forwarded. Check
   `crates/hana_lagrange/Cargo.toml` `[features]` for anything else and mirror
   `default` exactly if one exists.
7. `python3 ~/.claude/scripts/clean-fix/project_rename.py bevy_hana/crates/bevy_lagrange bevy_hana/crates/hana_lagrange`
8. Add a `[[publish_path_pins]]` entry `hana_lagrange = 0.4.0` **after** the
   release, never before — root `Cargo.toml:69` is path-only, and pinning the
   crate being released makes the release branch resolve its own not-yet-
   published version (same trap as the `bevy_kana` self-release note). The two
   existing pins (`bevy_kana 0.3.0`, `hana_clerestory 0.3.0`) already cover
   every versioned dep of `hana_lagrange`; the remaining path-only dev-deps
   (`fairy_dust`, `hana_diegetic`) get stripped by `cargo publish`.
9. Finally update nateroids to the two new names, which collapses it to a single
   bevy_kana copy, and commit its still-uncommitted changes.
10. Delete this doc.
