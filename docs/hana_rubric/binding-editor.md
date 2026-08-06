# Binding editor

> **Status: proposed · Priority: P0 · Product:** guided binding creation and editing

**Builds on:** v1 Phases 1, 6, 7, 11a, 19, and 20.

## Problem

Editing JSONC requires users to know command IDs, context names, keystroke spelling, tombstones,
held-command rules, and precedence. A typo or wrong scope can silently leave the intended command
unbound.

## Product outcome

Users record the keys they mean, choose where the binding applies, preview its effect, and commit a
valid edit without learning the wire format.

## Requirements

- Start from a selected command and one explicit scope: global or a named context.
- Capture modifiers, ordinary keys, bare modifier-family holds, and multi-stroke sequences using
  the runtime parser's canonical representation.
- Show each captured stroke as it arrives and provide explicit finish, retry, and cancel actions.
- Support adding another binding occurrence, replacing one occurrence, removing a user edit,
  explicitly unbinding with a tombstone, and resetting to the shipped default.
- Distinguish removing an override from suppressing a default; describe the resulting effective
  binding before save.
- Reject the protected recovery chord, invalid held-command shapes, empty sequences, and stale
  command/context selections before persistence.
- Pass every valid proposal through conflict analysis and persistence preview.
- Preserve keyboard ownership: captured keys do not dispatch commands or edit the search field.
- Make no change until the user confirms the preview.

## Non-goals

This item does not create commands, contexts, mouse gestures, gamepad bindings, or executable
macros.

## Acceptance criteria

- Add, replace, remove, tombstone, and reset workflows produce the expected effective keymap.
- A multi-stroke sequence and a bare-modifier held command can each be authored when valid.
- Protected and capability-incompatible proposals cannot reach the write transaction.
- Canceling capture or preview leaves the document and generation unchanged.
- The saved binding becomes visible in both the manager and command palette after reload.
