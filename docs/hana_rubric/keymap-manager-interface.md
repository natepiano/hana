# Keymap manager interface

> **Status: proposed · Priority: P0 · Product:** Hana in-app keymap manager

**Builds on:** v1 Phases 21, 23a, and 24, plus Hana's completed Phase 25 command palette.

## Problem

Hana's command palette can search commands, display one shortcut, and open repair files, but users
cannot browse the complete keymap or understand default, override, context, and health state inside
the app.

## Product outcome

A `Manage Keymaps` command opens a screen-space workspace where users can discover every command
and binding before choosing an edit or repair action.

## Requirements

- Search by command title, ID, description, or keystroke using the palette's normalization rules.
- Filter by namespace, context, capability, source layer, bound state, overridden state, and
  diagnostic state.
- Show every effective binding on each command row; never collapse multiple contexts into one
  unexplained keystroke.
- Show a selected command's metadata, default bindings, user edits, active result, source location,
  and relevant diagnostics.
- Provide clear entry actions for add, change, remove, reset, inspect, and open source.
- Display the active generation and whether an external change, save, reload, or failure is
  pending.
- Remain usable with only embedded defaults, an absent user file, or a rejected user-file reload.
- Support full keyboard navigation, visible focus, Escape-to-close, and correct text-entry routing
  through the existing IME ownership boundary.
- Open from the registry-driven command palette and return focus predictably when closed.

## Non-goals

The manager is not an embedded raw JSONC editor and does not replace the command palette.

## Acceptance criteria

- A user can locate a command by title, ID, and currently bound sequence.
- A command bound in two contexts shows both scopes and the correct effective state.
- The interface is operable without a pointer and ordinary shortcuts stay suppressed while its
  text field owns the keyboard.
- Opening the manager while the latest reload is invalid shows the last active generation and the
  rejected revision distinctly.
