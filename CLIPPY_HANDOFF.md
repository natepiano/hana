# /clippy run handoff — 2026-07-31

Delete this file once the run is finished.

## What this run is

`/clippy` on branch `feature/rubric`, HEAD `609b1980`. Nothing committed.

## Done

1. **cargo mend --fix** (3 passes, run unsandboxed — the `speech` crate's swiftc
   build script dies under the Claude sandbox). Applied **88 import rewrites**
   across 12 files in `crates/hana_rubric/`:
   - `inline-path-qualified-type` (58) — `fmt::Formatter` → `Formatter` + `use`
   - `replace-deep-super-import` (20) — `super::super::x` → `crate::disk::x`
   - `prefer-module-import` (8) — import module, qualify call site
2. **cargo mend --fix-compiler** deleted 3 now-unused imports. Two correct.
   The third **broke the build** — see below. Already fixed.
3. **Fixed the mend breakage**: `crates/hana_rubric/src/disk/worker/watch.rs`
   now has `#[cfg(test)] use std::sync::mpsc::SyncSender;` after
   `use std::sync::mpsc;` (line 6). `SyncSender<()>` is used only inside
   `#[cfg(test)] struct TestWatcher`; mend added the import un-gated, the lib
   target saw it unused, `--fix-compiler` deleted it, test target hit E0425.
4. **clippy**: EXIT=0, zero warnings, whole workspace, `--all-targets
   --all-features -- -D warnings`.
5. **cargo doc**: EXIT=101, 2 rustdoc errors (still open, see below).
6. **Style review**: walked all 39 checklist rules. One violation —
   forbidden words. Two hits:
   - `docs/hana_rubric/v1.md:1876` — **FIXED**, reworded to
     "matching how the existing builder methods chain by value."
   - `docs/hana_rubric/DELEGATE_HANDOFF.md:90` — **false positive, left as-is**.
     The line is a prohibition list that quotes the banned word in order to ban
     it (`No "honest", no "plain language", ...`). Editing it destroys the
     instruction.

## Left to do

1. **STEP 8** — run `~/.claude/scripts/clippy/lint fmt` (unsandboxed,
   background). Not yet run.
2. **Present the batch decision gate** for the 5 remaining issues below and
   WAIT for the user to say proceed / change / stop. Do not edit before that.

## The 5 open issues for the batch gate

Three mend errors (not auto-fixable) + two rustdoc errors:

| # | Src | File:line | Lint | Fix |
|---|---|---|---|---|
| 1 | mend | crates/hana_rubric/src/disk/worker/channels.rs:72 | forbidden_pub_crate | `pub(crate) fn take_message` → `pub(super)` |
| 2 | mend | crates/hana_rubric/src/keymap/mod.rs:7 | review_pub_mod | `pub(crate) mod runtime;` needs narrowing or allowlisting |
| 3 | mend | crates/hana_rubric/src/keymap/runtime/held.rs:133 | forbidden_pub_crate | `pub(crate) fn set_event_source` → `pub(super)` |
| 4 | doc | crates/hana_rubric/src/condition.rs:30 | private_intra_doc_links | public docs on `ConditionName` link to private `ConditionRegistry` |
| 5 | doc | crates/hana_rubric/src/condition.rs:31 | private_intra_doc_links | public docs on `ConditionName` link to private `ConditionHandle` |

Rule lookup not yet run for these — do
`grep -l "^lint:.*\bforbidden_pub_crate\b" ~/rust/nate_style/rust/*.md docs/style/*.md`
etc. before proposing fixes. Use LSP `findReferences` before any visibility
narrowing (items 1-3 change visibility).

## Known cosmetic residue from mend (not errors, no action decided)

Mend de-qualified types only halfway in places: `const CAPABILITY: crate::Capability = Capability::OneShot;`
in `command/registry.rs` (6×), `fmt::Result` left qualified in `command/id.rs`
while `Display`/`Formatter` were un-qualified, `io::Result` in
`disk/companion_files.rs`. Compiles fine. `cargo +nightly fmt` will re-sort the
appended imports but will not touch these.

## Environment gotchas hit this run

- Every cargo command here must run with `dangerouslyDisableSandbox: true` —
  the `speech` crate build script calls swiftc, which the sandbox blocks with
  `sandbox-exec: sandbox_apply: Operation not permitted`.
- Do not pipe the cargo command whose exit code matters; capture `$?` directly.
