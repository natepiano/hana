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

## Next: bevy_clerestory → hana_clerestory, bevy_lagrange → hana_lagrange

**Order is forced:** `bevy_lagrange` depends on `bevy_clerestory`, so clerestory
renames and releases first.

Local versions: `bevy_clerestory 0.3.0-dev`, `bevy_lagrange 0.4.0-dev`. Continue
those numbers on release (matches how hana_liminal continued `0.1.0-dev` → 0.1.0).

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

**bevy_clerestory → hana_clerestory is IN PROGRESS on `main`, uncommitted.**

Done so far:
- `git mv crates/bevy_clerestory crates/hana_clerestory`
- `git mv docs/bevy_clerestory docs/hana_clerestory`
- Blanket `bevy_clerestory` → `hana_clerestory` across the repo, excluding
  `Cargo.lock` (cargo regenerates) and this handoff doc (its prose names the old
  crate deliberately). 22 files touched, including root `Cargo.toml`'s
  `[workspace.dependencies]` entry, `crates/bevy_lagrange/Cargo.toml`,
  `fairy_dust`, `hana_diegetic`, `hana_liminal` examples, and several docs.
- README rename banner and CHANGELOG `[Unreleased] → Changed` entry added.
  Version stays `0.3.0-dev`; release as `0.3.0`. Shim will be
  `bevy_clerestory 0.2.1`.

Remaining for clerestory:
1. `cargo build --workspace --all-targets` (unsandboxed) then
   `cargo +nightly fmt`. Verify `rg -l bevy_clerestory` returns only
   `Cargo.lock` and this doc.
2. Commit the rename (ask first — standing rule).
3. `/release hana_clerestory 0.3.0`.
4. Shim at `~/rust/bevy_clerestory-shim/`: `pub use hana_clerestory::*;`,
   dep `hana_clerestory = "0.3"`, and **all six features mirrored** —
   `monitor-probe` plus `workaround-winit-{3124,4341,4440,4443,4445}`, with
   `default = [the five workaround features]` reproduced exactly. Each forwards
   as `["hana_clerestory/<name>"]`. `monitor-probe` is `["dep:tracing"]`
   upstream, so in the shim it must forward, not re-declare the dep.
5. `python3 ~/.claude/scripts/clean-fix/project_rename.py bevy_clerestory bevy_hana/crates/hana_clerestory`
6. Then repeat the whole sequence for `bevy_lagrange` → `hana_lagrange`
   (version `0.4.0-dev` → 0.4.0, shim `bevy_lagrange 0.3.1`, forward
   `fit_overlay`).
7. Finally update nateroids to the two new names, which collapses it to a single
   bevy_kana copy, and commit its still-uncommitted changes.
