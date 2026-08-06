# Keymap context inspector

> **Status: proposed · Priority: P1 · Product:** live routing explanation

**Builds on:** v1 Phases 5, 11c, 17, 19, 21, 22, and 24.

## Problem

A valid binding can appear not to work because a different context is active, text entry owns the
keyboard, a longer sequence is pending, or a higher-precedence entry wins. Static keymap rows
cannot explain frame-level routing behavior.

## Product outcome

Users can press a sequence in a non-dispatching inspector and see how Hana would route it now,
including the winning command or the reason nothing runs.

## Requirements

- Show the active condition, global scope, keyboard owner, text-entry state, pending sequence, and
  held-input state relevant to routing.
- Capture a test sequence without dispatching application commands.
- Trace normalization, scope candidates, source layers, prefix decisions, and the final winner.
- Name explicit non-dispatch outcomes such as unbound, protected, waiting for another stroke,
  shadowed, text entry owned, invalid for held input, or inactive context.
- Link every candidate to its command and binding row in the manager.
- Update when context, generation, keyboard ownership, or pending-sequence state changes.
- Keep tracing dormant and allocation-free on the normal input path while the inspector is closed.
- Offer a bounded copyable trace for bug reports without recording arbitrary typed text.

## Non-goals

The inspector does not invoke commands, mutate context, or become a general input-event recorder.

## Acceptance criteria

- The trace predicts runtime behavior for global, contextual, held, and multi-stroke bindings.
- IME ownership and pending-prefix cases explain why an otherwise valid command did not dispatch.
- Closing the inspector removes capture/tracing state and restores prior keyboard routing.
- Production input routing has no new per-frame allocation when the inspector is closed.
