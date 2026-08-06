# Keymap management model

> **Status: proposed · Priority: P0 · Product:** `hana_rubric` manager API

**Builds on:** v1 Phases 4b, 7, 12, 21, and 24.

## Problem

`KeymapBindings` reports one representative keystroke per command. It does not expose multiple
contexts, source layers, tombstones, shadowed entries, or the reason a binding is effective. A
manager built on that resource would hide real behavior and could edit the wrong entry.

## Product outcome

Applications can read one immutable snapshot that faithfully describes the current keymap's
declared commands, authored entries, and runtime resolution.

## Requirements

- Enumerate every declared command, including title, description, capability, and bound/unbound
  state.
- Enumerate every default and user binding occurrence with its sequence, global or named context,
  source layer, source location, and effective state.
- Represent effective, shadowed, overridden, tombstoned, and rejected entries as named states.
- Explain which entry wins for a command/sequence/context and the precedence rule that selected it.
- Identify one committed runtime generation and publish a replacement snapshot only when that
  generation changes.
- Give UI selections stable typed identities within a generation and reject identities from stale
  generations.
- Expose domain values rather than internal command/condition handles or parser storage.
- Preserve the allocation-free routing path when no management UI is reading the snapshot.

## Non-goals

This item does not edit files, render UI, or change keymap precedence.

## Acceptance criteria

- Fixtures cover one command bound globally and in multiple contexts, a user override, a
  tombstone, a shadowed entry, and an invalid entry.
- The snapshot's effective result matches the compiled keymap used for routing.
- Reload replaces the snapshot and invalidates stale item identities as one transaction.
- Existing command-palette callers of `KeymapBindings` continue to work.
