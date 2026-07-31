# Delegate handoff — stopped at Phase 14's gate

## Authorization state (must survive compaction)

- `MODE = verbose`, `AUTO_WINDOW = none`. **Phase 14 IS authorized** — the user
  said "approve and proceed" at its `<VerbosePrePhaseGate/>`, approving the
  briefing as given, which included the recommendation that
  `InteractionContext::compute` be **total** via an explicit resting variant.
  That resolves Phase 14's `**Pending decision:**` in favour of the resting
  variant. **Authorization covers Phase 14 only** — implementation, dual review,
  fixes, phase review, checkpoint commit. Stop at Phase 15's gate afterwards.
- Prior authorizing turn was "approved - commit phase 12, approved split,
  /plan:delegate auto through phase 13b", now fully consumed.
- **Still unanswered:** the Phase 13b shipped-defaults validation gap (item 1
  below). The user's "approve and proceed" addressed the Phase 14 gate; it did
  not answer that offer. Do not treat it as declined — it is recorded in the plan
  and still needs an answer.
- Committed this stretch: Phase 13a `869bb7ef` (+`8468496f`), **Phase 13b
  `7a7b9f77`**, plan doc + architect review `f8f67d08`, editor pass `bc33273d`.
- `SESSION_DIR = /tmp/claude/delegate/fed7907c-3d58-49c3-8c53-8cd85d658f2c`
  — **reuse it; do NOT run `prepare_session.sh`, which wipes it.**
- `WORKING_DIR = /Users/natemccoy/rust/bevy_hana_rubric`, branch `feature/rubric`.
- `FIX_PASS` resets to 0 for Phase 14. Every delegate dispatch needs
  `dangerouslyDisableSandbox: true`; `implement.sh`/`review.sh` arg order is
  `<session_dir> <working_dir> <prompt_file> <task> <role_description>` and the
  4th arg is a sub-task key (`implementation`|`review`|`mechanical`|`escalation`).

## Phase 13b — closed

Code at `7a7b9f77`: the hand-written schema checker is gone, replaced by the
`jsonschema` crate as the crate's first dev-dependency. Verified independently by
the orchestrator (the delegate's first report claimed five green gates on a tree
that did not compile): **160 library tests**, the `keymap_companion_files`
integration target passes, lint and `check fairy_dust` green.

Editor pass: **Zed passed all bullets** — relative `$schema` resolves,
completions with no workspace settings, comments produce no diagnostics, a
misspelled member is flagged. **VS Code was not tested; the user declined and
does not have it installed.** Recorded as such in the retrospective and in Phase
17's constraint.

## What was told to the user and is still owed

Two things raised at the Phase 14 gate that have no decision yet:

1. **The shipped-defaults validation gap.** Phase 13b's gate claimed "the shipped
   defaults pass" validation, but `correct_document_passes_draft_seven_validation`
   uses a five-line hand-written document, not `examples/keymap_demo.jsonc`.
   Closing it is small — validate the example asset against the published schema
   inside `crates/hana_rubric/tests/keymap_companion_files.rs`, which already has
   `jsonschema` available. Offered to the user as "before Phase 14, or leave it";
   **no answer yet.**
2. **The Phase 14 pending decision — does hana boot deaf?** A computed state whose
   `compute` returns `None` leaves hana with no keyboard input at all, globals
   included. The plan currently permits a partial `compute`. Recommendation: make
   it total via a resting variant. Written as a `**Pending decision:**` block in
   Phase 14's Work Order; it must be resolved before Phase 14 dispatches.

## Exact next step

Deliver the Phase 14 `<VerbosePrePhaseGate/>` briefing (why the phase exists, the
work, the types/APIs it introduces), surface the pending decision above, and
**wait**. Do not dispatch anything.

Delete this file once Phase 14 is approved and dispatched.

## Standing items owed to the user

- When the separate `/clippy` batch gate reaches them, tell the user to **reject
  clippy items 1 and 3** — narrowing `DiskWorkerChannels::take_message` or
  `KeymapRuntime::set_event_source` to `pub(super)` breaks Phase 12's
  out-of-module call sites.
- Phase 21 publishes `bevy_kana` and migrates `nateroids` and hana — hana meaning
  `../hana_tool_graph`, not `../hana`.
- Unresolved `**Pending decision:**` blocks remain in Phases 15 (re-affirm
  pin-not-publish), 16/18 (held-prefix symmetric drop), 17 (the 23
  `bind_action_system!` call sites), 20 (remote hold-command latch).
- **Everything from Phase 14 onward targets a different repository** — hana on
  branch `init/hana_catalyst` at `/Users/natemccoy/rust/hana_tool_graph` — and
  sits under a banner requiring a reconciliation pass against that branch before
  dispatch, because catalyst rewrote the files those phases touch. Phase 14 is
  the one hana phase with no collision (an entirely new file), but its **Files**
  and **Spec** still need checking against that checkout.
