# Single-line IME editing — implementation plan

## Goal

Add single-line text editing for diegetic panel values while keeping the first
version pragmatic: values live on the diegetic panel or backing ECS data, but
the active editor is a transient screen-space affordance anchored to the
projected value.

This supports DAW-style interactions such as double-clicking a floating-point
value on a panel, editing it in place, and committing the parsed value back to
the panel when the user presses Enter or clicks away.

## Design model

The IME is fundamentally window/screen-space. Bevy exposes it through
`Window::ime_enabled`, `Window::ime_position`, and `bevy_window::Ime` events.
Even when the editable value is drawn on a 3D panel, the OS candidate popup is
positioned in window coordinates.

So the first implementation should split the problem:

- **Stored value**: owned by the panel element, component, or backing model.
- **Edit session**: transient screen-space single-line editor.
- **Anchor**: projected from the diegetic field bounds to window coordinates.
- **Commit**: parse and write the final text back to the backing value.
- **Cancel**: discard the edit buffer and restore the previous presentation.

The user experience should still feel like editing the panel directly. The
screen-space editor follows the projected field, matches the field styling, and
the original panel value is highlighted, dimmed, or suppressed while editing.

## Why not world-space first?

A world-space editor would still need to project the caret into screen
coordinates for `Window::ime_position`, because the OS IME candidate popup is
not rendered in Bevy world space. It would also need world-space caret hit
testing, preedit styling, selection rendering, clipping, off-angle readability,
and camera-motion behavior.

That is useful later, but it is not required for the common "edit this value"
workflow. A screen-space editor anchored to the projected field gives the same
basic affordance with fewer rendering and input edge cases.

## User flow

1. The user double-clicks an editable value rendered on a diegetic panel.
2. Picking resolves the panel field and starts an edit session.
3. The edit session captures:
   - target entity or model key,
   - original text,
   - current edit buffer,
   - field style,
   - projected screen anchor,
   - commit parser/validator.
4. A screen-space single-line editor appears at the projected field position.
5. The active window has `ime_enabled = true`.
6. `Ime::Preedit` updates the composing string without committing it.
7. `Ime::Commit` inserts committed text into the edit buffer.
8. Keyboard editing handles Backspace, Delete, arrows, Enter, and Escape.
9. Enter or blur validates and commits the value back to the panel.
10. Escape cancels and restores the previous value.

## Public surface

The first public API should be field-oriented, not a general text editor.

Possible component shape:

```rust
#[derive(Component)]
pub struct EditablePanelValue {
    pub text: String,
    pub mode: EditableValueMode,
}

pub enum EditableValueMode {
    Text,
    Float {
        min: Option<f32>,
        max: Option<f32>,
    },
    Integer {
        min: Option<i64>,
        max: Option<i64>,
    },
}
```

The exact storage may change once it is wired into the panel element model. The
important public contract is:

- editable values opt in explicitly,
- the field decides how text is parsed,
- invalid input can reject commit and keep the editor open,
- commit/cancel emit events so app code can synchronize backing state.

Possible events:

```rust
pub struct DiegeticEditStarted {
    pub target: Entity,
}

pub struct DiegeticEditCommitted {
    pub target: Entity,
    pub text: String,
}

pub struct DiegeticEditCanceled {
    pub target: Entity,
}
```

## Internal systems

### Picking and focus

Use the existing picking path to identify editable panel values. A double-click
or configured activation gesture starts a `DiegeticEditSession` resource.

The session should be exclusive: one active single-line editor at a time. When a
new editable value is activated, the existing session is either committed or
canceled according to the configured blur policy.

### Screen-space editor

The editor is a transient screen-space entity, not part of the source panel's
layout tree. It should render above panels and follow the projected field anchor
each frame while editing.

The initial implementation can use one text run plus a cursor and optional
preedit styling. It does not need mouse selection or multiline layout.

### IME event handling

When an edit session is active:

- set the active window's `ime_enabled` to `true`,
- set `ime_position` from the projected caret position,
- consume `Ime::Preedit` into session-local composing state,
- consume `Ime::Commit` into the committed edit buffer,
- clear composing state on `Ime::Enabled`, `Ime::Disabled`, cancel, and blur.

When no edit session is active, `ime_enabled` should be `false` unless another
system has explicitly opted into IME for the same window. If this crate later
coexists with other text-input systems, this should become a small ownership
protocol instead of blindly toggling the field.

### Keyboard editing

Start with a deliberately small single-line command set:

- character insertion from committed text,
- Backspace/Delete,
- Left/Right,
- Home/End if trivial with the chosen cursor representation,
- Enter to commit,
- Escape to cancel.

Do not use raw key presses for text characters while IME is active. Text
characters should come from IME commits or Bevy logical-key text paths so dead
keys and composed characters work.

### Projection and anchoring

Each frame, compute the editor anchor from the target field's current panel
bounds:

1. field local bounds in panel layout space,
2. panel transform to world space,
3. active camera projection to viewport/window coordinates,
4. clamp or flip if the editor would leave the viewport.

If the target is off-screen, behind the camera, or occluded by policy, the edit
session should either cancel or keep the last valid anchor with a clear visual
state. The first version can cancel on invalid projection.

### Commit back to the panel

On successful commit, write the parsed value to the backing component/model and
rebuild or update the panel through the existing panel update path. For direct
tree replacement, use `DiegeticPanelCommands::set_tree` so layout change
classification remains intact.

The original panel value should not be edited by mutating the transient editor
alone. The transient editor is only the active input surface; the backing value
remains the source of truth.

## Non-goals for the first pass

- Multiline editing.
- Mouse text selection.
- Clipboard support.
- Rich text editing.
- Full world-space caret/preedit rendering.
- Multiple simultaneous editors.
- Mobile/software keyboard support beyond what Bevy exposes for desktop IME.
- Editing arbitrary `LayoutBuilder::text` nodes without an explicit backing
  value or field identity.

## Phases

### Phase 1 — Core edit session

Implement the data model and lifecycle:

- `DiegeticEditSession` resource,
- editable value marker/component,
- start/commit/cancel events,
- double-click activation,
- single active session,
- Enter/Escape/blur handling.

Acceptance: double-clicking an editable value starts a session, Enter emits a
commit event, Escape emits cancel, and only one session can be active.

### Phase 2 — IME and keyboard input

Wire the active session to Bevy window IME:

- toggle `Window::ime_enabled`,
- handle `Ime::Preedit`,
- handle `Ime::Commit`,
- maintain committed buffer plus composing string,
- update `Window::ime_position` from the caret anchor,
- support Backspace/Delete/Left/Right.

Acceptance: composed text input works for a single-line field, preedit is shown
without changing the stored value, commit inserts the final string, and cancel
clears composition.

### Phase 3 — Screen-space rendering

Render the transient editor:

- project target bounds each frame,
- draw the editor at the projected position,
- draw caret and composing text,
- apply basic viewport clamping,
- visually mark the source field as active.

Acceptance: the editor tracks a moving camera/panel, remains readable off-angle,
and the OS candidate popup appears near the editor/caret.

### Phase 4 — Commit integration

Connect committed text back to real panel values:

- parse text by field mode,
- reject invalid commits with editor still open,
- write valid values to backing data,
- refresh the diegetic panel,
- expose commit/cancel events for app-specific synchronization.

Acceptance: editing a numeric panel value changes the backing value and the
panel display after commit; cancel leaves the backing value unchanged.

## Later work

A later world-space editor can reuse most of the session and parsing model. The
main difference would be rendering caret, selection, and preedit directly on the
panel while still projecting the caret to `Window::ime_position` for the OS IME
candidate popup.

## Proposed user decisions

### D1 — Add stable editable field identity

Status: proposed
Class: design-improvement
Severity: important
Source: correctness, architecture, type-system

Problem: the current plan identifies the active target as a target entity or
model key, but panel layout/rendering uses frame-local indices such as layout
element index and render command index. Those are useful for reuse and geometry,
but they are not stable semantic identities across tree replacement, row
reorder, wrapping, repeated text, or sibling insertion.

Recommendation: add a stable `EditableFieldId` or `PanelFieldId` supplied by
the authoring API. Carry it through editable layout elements, render commands or
layout-result metadata, hit testing, edit sessions, and commit/cancel events.
Treat element indices and command indices as frame-local geometry lookup keys
only.

### D2 — Define field-level hit testing

Status: proposed
Class: design-improvement
Severity: important
Source: correctness, architecture

Problem: "use the existing picking path" is too vague for the current panel
architecture. Picking can resolve a panel hit, but editable values are layout
fields, not stable generated text entities. Without a resolver, double-click
activation may only know that the panel was clicked, not which value was
clicked.

Recommendation: define a field hit-test system that converts a pointer hit into
panel-local layout coordinates, filters editable field bounds from the latest
computed layout while respecting clip rects and draw order, and emits a typed
start request such as `DiegeticEditStartRequested { panel, field_id, window,
camera }`.

### D3 — Make IME ownership window-scoped from the first pass

Status: proposed
Class: design-improvement
Severity: important
Source: risk, architecture, type-system

Problem: the doc mentions the active window and warns against blindly toggling
`Window::ime_enabled`, but it leaves ownership underspecified. Bevy IME events
are tagged by window, and this crate has multi-window examples. A session must
not consume IME from the wrong window or disable IME that another system owns.

Recommendation: store the activation window and camera in the edit session, add
an internal per-window IME lease or owner token, filter IME and keyboard input by
that window, and release IME only if the diegetic editor still owns the lease.

### D4 — Gate other input while editing

Status: proposed
Class: design-improvement
Severity: critical
Source: risk, correctness

Problem: the plan does not define how active edit focus blocks other input
consumers. Arrow keys, Escape, Enter, wheel, activation clicks, and blur clicks
can otherwise affect both the editor and camera/app systems in the same frame.

Recommendation: define an input ownership rule for active edit sessions. Editor
keyboard and pointer events should be consumed or marked handled, activation
clicks should not immediately become blur clicks, and camera systems should have
an integration path such as `CameraInputDisabled` or a dedicated input blocker
while editing.

### D5 — Use a dedicated single-line edit buffer

Status: proposed
Class: design-improvement
Severity: important
Source: correctness, type-system

Problem: the plan names a current edit buffer and composing string, but does not
define cursor and text mutation semantics. Raw `String` plus ad hoc offsets can
split UTF-8, mishandle grapheme clusters, or accidentally persist preedit text.

Recommendation: introduce an internal `SingleLineEditBuffer` with committed
text, preedit text, validated cursor/range types, and UTF-8-safe editing
operations. Rendering derives display text from committed text plus preedit
state; backing values derive only from validated committed text.

### D6 — Model edit lifecycle as explicit state transitions

Status: proposed
Class: design-improvement
Severity: important
Source: type-system, risk

Problem: a mutable `DiegeticEditSession` bag can represent impossible states:
committed and canceled in the same frame, invalid commit closing the editor,
preedit surviving cancel, or a new activation racing an old blur.

Recommendation: model editor state as a closed enum, such as `Idle`,
`Editing(EditSession)`, `Composing(EditSession, PreeditText)`, and
`CommitRejected(EditSession, EditValidationError)`. Expose transition methods
for start, IME preedit, IME commit, commit request, commit rejection, commit
success, and cancel. Emit lifecycle events from terminal transitions.

### D7 — Separate commit request, validation, rejection, and applied commit

Status: proposed
Class: design-improvement
Severity: important
Source: architecture, risk, type-system

Problem: `DiegeticEditCommitted { target, text }` does not say whether parsing
succeeded, what typed value changed, or whether app code has already applied the
backing model update. Invalid blur commits can otherwise emit misleading events
or trap focus without a clear recovery policy.

Recommendation: split commit into explicit stages: request, validation,
rejection, and applied commit. Built-in numeric modes should produce a typed
validated value before emitting a committed/applied event. Invalid input should
emit a rejection or hold an error state while keeping the editor open according
to a documented blur policy.

### D8 — Specify target invalidation and projection ordering policy

Status: proposed
Class: design-improvement
Severity: important
Source: risk, correctness

Problem: the plan says the editor follows projected field bounds each frame, but
does not define what happens when the panel despawns, field id disappears, tree
revision changes, backing state changes externally, camera changes, window
resizes, or projection is temporarily invalid.

Recommendation: store panel entity, field id, activation window, activation
camera, starting tree revision, and original value in the session. Re-resolve
the field each frame from the current computed layout, project after transform
and camera data are current, and apply a documented stale-target policy:
deterministically cancel on missing panel/field/window/camera, and require a
clear policy for revision changes and external value changes.

### D9 — Keep the transient editor inside the existing screen-space pipeline

Status: proposed
Class: design-improvement
Severity: important
Source: architecture

Problem: the plan says the editor is a transient screen-space entity, but does
not say whether it reuses the existing screen-space panel/camera/layer machinery
or creates a parallel overlay renderer. A separate path can drift in window
selection, camera order, render layers, text styling, and cleanup.

Recommendation: implement the transient editor as an internal screen-space
editor/panel module that depends on the existing screen-space systems and text
style conversion. Keep cursor and preedit visuals editor-specific, but avoid a
parallel rendering stack for the first pass.
