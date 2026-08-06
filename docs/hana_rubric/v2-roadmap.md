# hana_rubric v2 — in-app keymap management

> **Status: PRODUCT ROADMAP.** v2 turns the shipped keymap runtime into a Hana-native interface for
> discovering, editing, validating, and recovering keymaps without requiring a text editor.

## Shipped baseline

v1 already provides command metadata, global and condition-scoped bindings, multi-stroke and held
commands, default/user JSONC layering, tombstones, schema publication, hot reload, diagnostics, and
the protected recovery chord. Hana's `feature/rubric` branch also contains the completed
registry-driven command palette and keymap repair routes (`d6a3b4e`, refined by `01876be`).

The remaining gap is management. The public `KeymapBindings` resource exposes one representative
keystroke per command, the app opens keymap files in an external editor, and no public transaction
can safely author a user keymap. v2 fills that gap without replacing the JSONC format or the live
reload path.

## Product principles

- The effective live keymap and the authored default/user layers are both visible; the UI never
  presents one representative binding as the whole truth.
- Every edit states its context, source layer, precedence effect, and conflicts before it is saved.
- Invalid or stale edits never replace the active keymap generation.
- Untouched JSONC comments, ordering, and manually authored content survive app-originated edits.
- External editor changes remain supported and are reconciled explicitly.
- The protected recovery chord and degraded repair path remain independent of the editable keymap.

## Development items

| Order | Item | Priority | Outcome |
| --- | --- | --- | --- |
| 1 | [Keymap management model](keymap-management-model.md) | P0 | A complete, manager-readable projection of authored and effective bindings. |
| 2 | [Keymap persistence and history](keymap-persistence-history.md) | P0 | Validated, atomic, comment-preserving edits with undo and external-change protection. |
| 3 | [Keymap manager interface](keymap-manager-interface.md) | P0 | A searchable in-app home for commands, bindings, contexts, and keymap health. |
| 4 | [Binding editor](binding-editor.md) | P0 | Keyboard capture plus add, replace, remove, unbind, and reset workflows. |
| 5 | [Binding conflict resolution](binding-conflict-resolution.md) | P0 | Runtime-accurate conflict previews and explicit repair choices. |
| 6 | [Keymap diagnostics center](keymap-diagnostics-center.md) | P1 | Persistent, actionable load and configuration diagnostics. |
| 7 | [Keymap context inspector](keymap-context-inspector.md) | P1 | A live explanation of which binding wins and why. |
| 8 | [Keymap profiles](keymap-profiles.md) | P1 | Named, validated, portable keymap sets with a safe migration from `keymap.jsonc`. |

Items 1 and 2 are platform work in `hana_rubric`. Items 3–8 are Hana product surfaces backed by
those APIs. The manager interface may land read-only after item 1 while persistence is completed.

## v2 completion

v2 is complete when a user can find any declared command, see every binding and its scope, record a
replacement, understand and resolve conflicts, save or undo the change, recover from a malformed
file, explain live routing, and move between named profiles without leaving Hana.

## Outside v2

Mouse/gamepad binding, command macro authoring, executable user macros, cloud synchronization, and
an embedded raw JSONC editor are separate products. v2 continues to support external JSONC editors
through the existing schema and watcher.
