# Keymap diagnostics center

> **Status: proposed · Priority: P1 · Product:** persistent keymap health and repair

**Builds on:** v1 Phases 12 and 23a, plus Hana's completed Phase 25 failure surface.

## Problem

The command palette renders current keymap failure rows and file actions, but it is a transient
search surface. It cannot show the relationship between a rejected reload, the still-active
generation, source text, suggested repairs, and edit history in one place.

## Product outcome

The manager has a durable health view where users can understand, locate, and repair every current
or retained keymap diagnostic.

## Requirements

- Combine reload and retained diagnostics without losing their origin, severity, or lifecycle.
- Group and filter by failure/advisory, source, kind, context, and current-versus-retained state.
- Show source path, line/column, relevant source text, message, and machine suggestions when
  available.
- Distinguish the rejected revision from the generation still routing input.
- Link diagnostics to the affected manager row and binding editor when an in-app repair is safe.
- Preview every suggested replacement through normal conflict and persistence validation.
- Retain Open/Reveal actions for failures that cannot be repaired in-app.
- Provide retry/reload status and deduplicate repeated worker reports without hiding recurrence.
- Export a compact diagnostic report containing versions, paths, generations, and messages but no
  unrelated file contents.

## Non-goals

The center does not suppress diagnostics, silently repair files, or expose generic application
logs.

## Acceptance criteria

- Whole-file syntax failure shows that the prior generation remains active.
- Per-binding failures link to the correct context, sequence, command, and source location.
- Startup registry/path failures remain visible after later reloads.
- Applying a suggestion uses the same preview, conflict, save, and reload gates as a manual edit.
