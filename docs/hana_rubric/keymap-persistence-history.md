# Keymap persistence and history

> **Status: proposed · Priority: P0 · Product:** safe user-keymap transactions

**Builds on:** v1 Phases 6, 9, 10, 12, and 24.

## Problem

The runtime reads, watches, and reloads `keymap.jsonc`, but it has no public mutation boundary for
an in-app editor. Direct rewrites risk losing comments, racing an external editor, replacing a
symlink, or leaving a partial file that disables the user's latest changes.

## Product outcome

Hana can preview and commit structured keymap edits without corrupting manual work, and users can
undo both in-session mistakes and broader resets.

## Requirements

- Model add, replace, remove, tombstone, and reset as typed edit operations against a specific
  source revision.
- Validate commands, contexts, keystrokes, held-command restrictions, and protected sequences
  before any write.
- Preview the changed bytes, human-readable diff, and effective binding changes before commit.
- Preserve untouched bytes, including comments, whitespace, ordering, and unrecognized members.
- Commit durably and atomically; a failure leaves both the file and active generation unchanged.
- Preserve symlink behavior instead of silently replacing a linked keymap with a regular file.
- Detect external changes by revision. A stale app edit pauses for rebase/review and never
  overwrites the newer file.
- Keep bounded in-session undo/redo plus timestamped pre-change backups for destructive resets.
- Confirm that the watcher commits exactly the saved revision and report success, rejection, or
  timeout as explicit states.
- Browsing and keystroke capture never write implicitly.

## Non-goals

This item does not provide cloud history, source control, or a general JSONC editor.

## Acceptance criteria

- App edits preserve comments and unrelated manual blocks byte-for-byte.
- Concurrent external edits produce a reviewable conflict instead of data loss.
- Write, permission, and reload failures leave the previous generation usable.
- Undo and redo survive multiple edits during one Hana session; a reset can restore its backup.
- Native-save, rename-save, and symlinked-keymap tests pass on supported platforms.
