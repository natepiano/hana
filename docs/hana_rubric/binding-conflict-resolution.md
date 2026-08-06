# Binding conflict resolution

> **Status: proposed · Priority: P0 · Product:** runtime-accurate conflict analysis

**Builds on:** v1 Phases 7, 11a, 19, 20, and 24.

## Problem

The same keystroke can appear in default and user layers, global and contextual scopes, or as a
prefix of another sequence. A simple duplicate check cannot say whether the new binding replaces,
shadows, delays, or coexists with existing behavior.

## Product outcome

Before saving, Hana explains the exact runtime consequence of a proposed binding and offers only
resolutions that preserve a valid keymap.

## Requirements

- Analyze proposals with the same layering, context, sequence-prefix, held-command, and protected
  keystroke rules used by compilation.
- Classify exact collisions, shadowing, default overrides, tombstone effects, sequence-prefix
  ambiguity, capability violations, and protected-sequence violations.
- State which command wins, in which context, and whether the result blocks save or is advisory.
- Treat bindings in mutually exclusive scopes as coexistence, not a false collision.
- Offer explicit choices where legal: replace the existing user entry, override a default, unbind
  the old entry, keep an intentional prefix relationship, choose another scope, or cancel.
- Never modify shipped defaults; default conflict repair is expressed as a user override or
  tombstone.
- Re-run analysis against the latest source revision immediately before commit.
- Do not choose or apply a resolution automatically.

## Non-goals

This item does not redesign runtime precedence or promise that every advisory has a one-click fix.

## Acceptance criteria

- Tests cover same-scope collisions, global-versus-context precedence, default overrides,
  tombstones, multi-stroke prefixes, held commands, and the recovery chord.
- The preview result matches the binding selected by the compiled runtime.
- A stale preview cannot commit after an external edit or keymap generation change.
- Every blocking result names at least one safe next action.
