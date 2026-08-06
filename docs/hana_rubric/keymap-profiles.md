# Keymap profiles

> **Status: proposed · Priority: P1 · Product:** named and portable keymap sets

**Builds on:** v1 Phases 7, 9, 10, 12, and 24.

## Problem

One `keymap.jsonc` cannot cleanly support alternate workflows, experiments, shared team layouts, or
safe import. Users must replace the live file manually and keep their own backups.

## Product outcome

Users create, clone, validate, switch, import, export, and delete named keymap profiles from Hana
without risking the currently active profile.

## Requirements

- Preserve `keymap.jsonc` as the initial `Default` profile during a lossless, one-time migration.
- Support create-empty, clone-current, rename, duplicate, switch, export, import, and delete.
- Layer exactly one active user profile over shipped defaults; show the active profile everywhere
  the manager reports source or generation.
- Validate an imported or selected profile before activation. Failure leaves the current profile
  and generation active.
- Store profile identity separately from editable JSONC so renaming does not rewrite bindings.
- Use persistence history for profile mutations, backups, and external-change protection.
- Require confirmation before deleting a non-empty profile and prevent deleting the only profile.
- Export portable JSONC without machine-specific absolute paths; never export backups or unrelated
  configuration files.
- Watch only the active profile for runtime reload while still detecting manager-visible metadata
  changes.

## Non-goals

Profiles do not include cloud synchronization, automatic device selection, application settings,
or command implementations.

## Acceptance criteria

- Existing installations adopt their current `keymap.jsonc` without changed effective bindings.
- Switching profiles commits one validated generation or leaves the previous profile untouched.
- Invalid imports are inspectable but cannot become active.
- Export followed by import on another supported platform preserves commands, contexts, sequences,
  tombstones, and comments.
- Deletion and rename remain recoverable through the shared history/backup policy.
