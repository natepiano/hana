# Delegate handoff — `docs/hana_rubric/v1.md`, stopped after Phase 24

Written 2026-08-05 at the user's request, to pause the run and resume it in a
fresh session. **Read this first, then delete it once Phase 25 is committed.**

## Where the run stands

**Phase 24 is complete and committed — `43742343`.** Working tree was clean
before this handoff; the only uncommitted changes are this file and the Phase 25
edits to `v1.md` described below.

**Phase 25 is the next phase and is not yet authorized.** It has a complete,
dispatchable Work Order (`docs/hana_rubric/v1.md:1681`), written by the
post-Phase-24 architect review against the shipped code.

Nothing is running. No background dispatches, no monitors, no live worktrees.

## Authorization state (this is the part a summary loses)

- **Mode:** `verbose`. Every phase gets a pre-phase briefing and an explicit
  approval before dispatch.
- **Auto window:** none. No phase is pre-approved.
- **Last gate reached:** the Phase 24 post-phase report was delivered. The
  Phase 25 pre-phase gate has **not** been approved — do not dispatch it on
  resume without asking.
- **Never push. Never create a branch. One checkpoint commit per phase.**

## Run identifiers

- `SESSION_DIR = /tmp/claude/delegate/40cc6ea4-a454-4a4a-bfec-b56eddfda560`
  (`/tmp` is cleared periodically — if it is gone, start a fresh session dir with
  `prepare_session.sh`; nothing in the old one is still needed)
- `WORKING_DIR = /Users/natemccoy/rust/bevy_hana_rubric` (branch `feature/rubric`)
- Review passes 1–8 are consumed in that session dir; the next is 9.
- Commit chain: 23a `dc8575e4` · rename `a6ae835b` · 23b `837f8044` ·
  smoke docs `5e8bde82` · 23c `06bcbad4` · **24 `43742343`**

## The two checkouts — do not conflate them

| Path | What it is | Remote |
| --- | --- | --- |
| `/Users/natemccoy/rust/bevy_hana_rubric` | the **library** workspace (`hana_rubric`, `fairy_dust`, `hana_diegetic`, …) — where this plan doc lives | `github.com/natepiano/hana.git` |
| `/Users/natemccoy/rust/hana_rubric` | the **hana application** — consumes `hana_rubric` by path, branch `feature/rubric` | `github.com/hanallc/hana.git` |

Similar names, different repos.

## What changed in `v1.md` in this final turn

Phase 25's Work Order carried one `**Pending decision:**` block — whether the
phase could bump hana's `hana_diegetic` git pin, which would have required
pushing the library commits first. **The user resolved it on 2026-08-05: hana
takes `hana_diegetic` from the local checkout by path. The pin is not bumped and
nothing is pushed.** The pending-decision block is replaced with the decision and
its consequences; two downstream references (the Files list entry for
`Cargo.toml:89`, and the no-manifest-change constraint) were updated to match, and
the phase's stale "raw material only, not dispatchable" banner was corrected.

Backup of the pre-edit doc: `/private/tmp/claude/v1.md.pre-pin-decision`.

## The one thing the user must do before Phase 25 is dispatched

**Merge hana `main`'s `hana_diegetic` into
`/Users/natemccoy/rust/bevy_hana_rubric/crates/hana_diegetic`.** The user said
they would do this. Phase 25 cannot start until it has landed, and the check is
one command:

```
rg -l ImeReplacePanelTree /Users/natemccoy/rust/bevy_hana_rubric/crates/hana_diegetic/src
```

If that finds nothing, the merge has not happened — stop and say so.

## Exact next step on resume

1. Confirm the merge above landed.
2. Deliver the Phase 25 pre-phase briefing (verbose mode: why the phase exists,
   the work, the types/APIs it introduces) and **wait for approval**.
3. On approval, dispatch Phase 25 per `/plan:delegate`.

Phase 25 in one line: hana grows its own registry-driven command palette,
modeled on `crates/fairy_dust/src/command_palette/` in behavior and appearance
but taking no `fairy_dust` dependency — 12 Spec items, 17 acceptance tests. Its
first real risk is the dependency swap, not the palette: five crates in hana's
graph (`hana_conduit`, `hana_lading`, `hana_prosody`, `hana_valence`,
`hana_clerestory`) depend on `hana_diegetic` themselves, so a bare path swap on
one line yields two copies of the crate and duplicate-identity type errors. The
Work Order says to redirect at the source with a `[patch]` section and verify
with `cargo tree -p hana_diegetic`.

## Still open, for the final gate (after the last phase)

- `verify.sh final` plus the `clippy` skill with `auto-proceed`. **Tell the user
  to reject clippy items 1 and 3** — narrowing `DiskWorkerChannels::take_message`
  or `KeymapRuntime::set_event_source` to `pub(super)` breaks Phase 12's
  out-of-module call sites.
- **A human must press Cmd+P, type into the palette, and run a command.** IME
  text entry cannot be driven over BRP, so this smoke step has been deferred
  since Phase 23b and is still owed.
- `/plan:compact` is paused and unfinished — backup at
  `<scratchpad>/v1.md.pre-compact.md`, nothing was spliced. The doc is 1943
  lines, so compaction is worth running before the next long stretch.

## Phase 24's public-API note, for the record

Three changes went beyond the Work Order's letter, all following from Spec item
2: `#[non_exhaustive]` on both `KeystrokeRouting` variants, `#[must_use]` on
`take_for_text_entry` / `release`, and narrowing `KeystrokeRouting::text_entry`
from `pub` to `pub(crate)`. That last one closed a hole — `#[non_exhaustive]`
blocks variant construction but left a public constructor through which an
outside crate could hand itself the keyboard over whoever already held it.
`text_entry` shipped in this same phase and was never released, so nothing broke.
