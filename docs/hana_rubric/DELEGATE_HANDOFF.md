# Delegate handoff — /plan:delegate @docs/hana_rubric/v1.md verbose

## Authorization state (most important — get this wrong and a phase runs unapproved)

- `MODE = verbose`. **`AUTO_WINDOW = none` — the "through phase 11c" window is
  CLOSED.** Phase 12 requires a fresh `<VerbosePrePhaseGate/>` approval.
- The user's last message was `continue` — the post-phase-11c gate control. It
  advanced to **assembling and showing Phase 12's briefing**. It does **NOT**
  authorize Phase 12.
- **Current position: waiting at Phase 12's `<VerbosePrePhaseGate/>`,** with
  three pending decisions presented alongside the briefing.
- `FIX_PASS = 0`. `APPLICATION_SMOKE_RESULT = not_run` (no application path
  until Phase 12).

## Session facts

- `SESSION_DIR = /tmp/claude/delegate/fed7907c-3d58-49c3-8c53-8cd85d658f2c`
  — **reuse it; do NOT re-run `prepare_session.sh`, which wipes it.**
- `WORKING_DIR = /Users/natemccoy/rust/bevy_hana_rubric`, branch `feature/rubric`.
- Delegate family **codex**: `gpt-5.6-terra:xhigh` implement, `gpt-5.6-sol:xhigh` review.
- Every `implement.sh` / `review.sh` dispatch needs `dangerouslyDisableSandbox: true`.
  Args: `<session_dir> <working_dir> <prompt_file> <task> <role_description>`.
  The 4th arg is a **sub-task key** (`implementation`|`review`|`mechanical`|`escalation`),
  not a free label — a bad value exits 1 with empty stdout.
- Each phase records its hash in a **follow-up commit**, never `--amend`.
- Prompt files are `*_p<N>_<n>.md` — older bare names exist and `Write` refuses
  to overwrite an unread file.

## Committed

P1 `523c3471`, P2 `27b33a1f`, P3 `daa7c6c0`, P4a `fd7e91e1`, P4b `3c5db62b`,
P4c `f876b36e`(+`6cad57d2`), plan-doc `8876cbd6`, P5 `d75ffe2e`(+`6701af3c`),
P6 `1cfad3fa`(+`ee3ec49a`), P7 decisions `fcf0f4ce`, P7 leftovers `c652089d`,
P7 `a95983bb`(+`ce001129`), P8 `2b663825`(+`3edab1f3`), P9 `ca2d1a94`(+`e1b72761`),
P10 `a182c2a9`(+`de2a8f60`), split `47f34e78`, P11a `4f4134a3`(+`e947dc00`),
P11b `f5fc90c0`(+`d3cb983d`), **P11c `a1e010f1`(+`609b1980`)**.
**HEAD = `609b1980`. Working tree clean.**

## Phase 11c — DONE, fully reviewed

Shipped `crates/hana_rubric/src/keymap/runtime/{mod,dispatch,held,key_edge}.rs`.
Blind review found 7 blocking findings (5 were tests that passed with the
mechanism deleted); all fixed in one pass; orchestrator re-verified all four gate
lines (136/136 tests, clippy `-D warnings` clean, `fairy_dust` clean).
Architect review returned 16 findings; all applied. Retrospective + `Phase 11c
Review` block are in `v1.md`.

## Phase 12 — three pending decisions, all unresolved

Presented to the user in the same turn as the briefing. **All three must be
answered before dispatch.**

1. **Decision A** — accept a builder-supplied protected keystroke
   (`.with_protected_keystroke(Keystroke)`). Recommended: take it.
2. **Decision B** — keep registry + condition validation all-or-nothing.
   Recommended: route 1 (degrade loudly, insert an empty registry, soften the
   shipped-default rule so one bad declaration is not fatal).
3. **NEW, added by the 11c architect pass** — does a failed context registration
   also silence *global* bindings? Today it does: `register_context`
   (`condition.rs:299-303`) clears the initialized bit before registering and
   nothing restores it on failure, so `route_input` returns at
   `keymap/runtime/dispatch.rs:41-43`. One missing `#[strum(message)]` = an app
   with **no keyboard input at all**. Recommended: call `enable_global()` on the
   failure path so global bindings still route.

## EXACT NEXT STEP

1. Wait for the user's answers to the three decisions **and** approval of Phase 12.
2. Record each decision into Phase 12's Work Order (replace the
   `**Pending decision:**` blocks with the resolved outcome).
3. Compose `implementation_prompt_p12.md` → dispatch `implement.sh … implementation`
   → arm the monitor → `<DualReview/>` → `<Synthesize/>` → phase review →
   checkpoint commit + hash follow-up.
4. Then stop at Phase 13's `<VerbosePrePhaseGate/>`.
5. Delete this file once Phase 12 is committed.

## Standing user constraints (verbatim, still in force)

Never commit unless asked (the delegate checkpoint commit IS asked for; never
push). **NEVER create a branch unless asked** — overrides the harness default of
branching off `main`. Always `cargo nextest run`, never `cargo test`.
`dangerouslyDisableSandbox: true` for `gh`, git branch-switching/worktree ops,
`taplo`, and anything launching `codex`. Prefer `rg` (`rg -r` is `--replace` —
never `rg -rn`). Prefer LSP over text search. Background long commands and end
the turn rather than polling. Ask the user to do type/field renames in the
editor. **Never make code changes or destructive git ops without explicit
go-ahead.** **Never git checkout/restore to discard changes without approval.**
Impossible designs and clear mistakes are not decisions — fix them, don't ask.
Never tell the user what scope is too big or small. Terse and technical, lead
with the answer. No "honest", no "plain language", no dramatic framing.
Do not call the Agent tool unless the user requested it (the `/plan:phase_review`
architect pass is authorized by the invoked workflow).

Two standing requests: (a) Phase 21 publishes `bevy_kana` and migrates
`nateroids` and hana — hana meaning `../hana_tool_graph`, not `../hana`;
(b) `/adhoc_review` results recorded in `docs/hana_rubric/v1.md`.

---

## Update — Phase 12 gate, decisions in progress (2026-07-31)

Still at Phase 12's `<VerbosePrePhaseGate/>`. AUTO_WINDOW = none. Phase 12 is
NOT yet authorized — the user is answering the three pending decisions first,
walked one at a time in `/adhoc_review` style at their request ("i don't
understand the other 2 though").

**Resolved and already written into `docs/hana_rubric/v1.md` (Phase 12 Work
Order, above the archived decision text):**

1. **Decision A — `.with_protected_keystroke(Keystroke)`** — approved as
   recommended, with the user's amendment that it must be **chainable**:
   `#[must_use] pub fn with_protected_keystroke(mut self, keystroke: Keystroke)
   -> Self`, by-value `self`, pushes onto a `Vec<Keystroke>`, repeated calls
   accumulate. Duplicates are not an error. Acceptance: two distinct protected
   keystrokes are each enforced (proves accumulation, not last-write-wins).
2. **Decision B — registry/condition validation stays all-or-nothing (Route
   1)** — approved. Empty `CommandRegistry` inserted, diagnostics recorded,
   error-level log, app starts. Shipped-default hard-fail is suppressed when the
   registry is empty *because validation failed*; it still fires when the
   registry built fine but a default names something nonexistent. No signature
   change to `CommandRegistry::initialize` or `ConditionRegistry::register`.
   Zed precedent read from source and recorded in the doc (its action registry
   `panic!`s on duplicate names — `gpui/src/action.rs:286,296,311,53`; its
   keymap *files* use the same two-tier model as D7 —
   `settings/src/keymap_file.rs:158,180,204,287-297`).

**Still open — the last one:** does a failed **context** registration also
silence *global* bindings? Currently yes, because `register_context` calls
`await_context()` unconditionally (`condition.rs:299-303`) and nothing restores
`is_initialized` on the `Err` path, so `route_input` bails at
`keymap/runtime/dispatch.rs:41-43`. Recommendation: call `enable_global()` on
the failure path so global bindings keep routing. Zed supports this — a bad
context predicate skips only its own section. Present it as `/adhoc_review`
item B2, behavior-first, then wait.

**After it is answered:** write the outcome into the Work Order the same way,
then present the Phase 12 `<VerbosePrePhaseGate/>` briefing (already drafted and
shown once) and wait for approval. Do not dispatch until the user approves.

### All three Phase 12 decisions now resolved (2026-07-31)

3. **Context-registration failure silences ALL input, globals included** —
   recommendation REJECTED; today's behavior kept deliberately. No
   `enable_global()` on the `Err` path. Rationale + acceptance cases written
   into the Work Order. Read from the user's "in developer mode that's fine to
   be more guarded" followed by "agreed" after seeing the triggering example.

**Next step: Phase 12 is still NOT authorized.** Present the
`<VerbosePrePhaseGate/>` briefing (already drafted once) and wait for explicit
approval before composing `implementation_prompt_p12.md`.
