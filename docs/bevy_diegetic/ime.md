# Single-line IME — implementation plan

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

The first public API has two entry points over the same IME core:

- **field-authored editing**: an editable field is declared in a panel layout
  and activated by picking;
- **app-authored screen-space sessions**: app code can open and close an IME
  session attached to a screen-space panel field it owns.

Both paths share the same session ids, buffer, IME ownership, shortcut handling,
and lifecycle events.

Possible field-authored shape:

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
- editor lifecycle emits typed events so app code can synchronize backing state
  without guessing which session or commit attempt is current.

## Event contract

Bevy provides the OS-facing input events through `bevy_window::Ime`. The IME
module should translate those low-level window events into session-scoped
events. App code should listen to these events, not raw `Ime`, unless it is
implementing another text-entry system.

Public IME types live under `bevy_diegetic::ime`. The short names are
intentional inside that module; avoid re-exporting the full event set at the
crate root.

Every IME session event carries a `SessionId`. Events that are part of a
commit attempt also carry a `CommitAttemptId`, so delayed validation or apply
responses cannot affect a newer edit attempt in the same field.

```rust
pub struct SessionId(u64);
pub struct CommitAttemptId(u64);

/// Semantic target of the session, not the rendered surface or anchor provider.
pub enum Target {
    WorldPanelField {
        panel: Entity,
        field_id: PanelFieldId,
    },
    ScreenPanelField {
        panel: Entity,
        field_id: PanelFieldId,
    },
    AppOwned {
        owner: Entity,
        field_id: PanelFieldId,
    },
}

/// Validated UTF-8 byte boundary in the committed edit buffer.
pub struct BufferBoundary(usize);

/// Validated UTF-8 byte boundary in the active preedit string.
pub struct PreeditBoundary(usize);

pub struct BufferRange {
    pub start: BufferBoundary,
    pub end: BufferBoundary,
}

pub struct Preedit {
    pub text: String,
    pub replacement: BufferRange,
    pub cursor: Option<PreeditBoundary>,
}

pub struct BufferSnapshot {
    /// Committed buffer text only. Preedit text is separate.
    pub committed_text: String,
    pub cursor: CursorState,
    pub preedit: Option<Preedit>,
}

pub enum CursorState {
    Insertion(BufferBoundary),
    Selection(BufferRange),
}

pub struct Started {
    pub session_id: SessionId,
    pub target: Target,
}

pub struct TextChanged {
    pub session_id: SessionId,
    pub target: Target,
    pub buffer: BufferSnapshot,
}

pub struct CommitRequested {
    pub session_id: SessionId,
    pub attempt_id: CommitAttemptId,
    pub target: Target,
    pub cause: CommitCause,
    pub text: String,
}

pub struct ValidationRejected {
    pub session_id: SessionId,
    pub attempt_id: CommitAttemptId,
    pub target: Target,
    pub reason: Rejection,
}

pub struct Applied {
    pub session_id: SessionId,
    pub attempt_id: CommitAttemptId,
    pub target: Target,
    pub output: Output,
}

pub struct Canceled {
    pub session_id: SessionId,
    pub target: Target,
    pub cause: CancelCause,
}

pub struct AcceptCommit {
    pub session_id: SessionId,
    pub attempt_id: CommitAttemptId,
    pub output: Output,
}

pub struct RejectCommit {
    pub session_id: SessionId,
    pub attempt_id: CommitAttemptId,
    pub reason: Rejection,
}
```

`BufferBoundary` is a byte offset in `committed_text`. `PreeditBoundary` is a
byte offset in `Preedit::text`. Both are only constructible after UTF-8
boundary validation. User-facing movement and deletion should operate in
grapheme or word terms, then convert to these validated boundaries before
mutating the buffer. `Preedit::replacement` is the committed-buffer range
the active preedit would replace for display, layout, and eventual commit.

`TextChanged` is the event app-owned panels listen to for live text updates. It
fires for committed-buffer changes and preedit-only changes; consumers choose
whether to preview `preedit` or use only `committed_text`. It is not a field
commit. `Applied` is reserved for the point where backing ECS/model state
actually changed.

Direction matters:

- outbound events from the editor: `Started`, `TextChanged`,
  `CommitRequested`, and `Canceled`;
- inbound app/apply-sink responses: `AcceptCommit` and `RejectCommit`, matched
  by both `SessionId` and `CommitAttemptId`;
- outbound confirmation from the transition system after a response:
  `ValidationRejected` or `Applied`.

For crate-owned built-in fields, the apply sink can validate and mutate backing
state internally before emitting `Applied`. For app-owned state, app code
handles `CommitRequested`, mutates its model if valid, then sends `AcceptCommit`
or `RejectCommit`.

`Target` answers "what backing thing is being edited?" It is semantic identity
only. Anchor providers, focus scopes, validation/apply sinks, and edit surfaces
are separate internal capabilities so systems do not branch directly on
`Target`.

## Internal systems

### Picking and focus

Use the existing picking path to identify editable panel values. A double-click
or configured activation gesture starts an IME `Session` resource.

The session should be exclusive: one active single-line editor at a time. When a
new editable value is activated, the existing session is either committed or
canceled according to the configured blur policy.

### Screen-space editor

The editor is a transient screen-space entity, not part of the target panel's
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

The first implementation and example should cover the full single-line command
set below, not only bare character insertion:

- character insertion from committed text,
- Backspace/Delete,
- Left/Right with Shift selection,
- word movement/deletion,
- Home/End or platform equivalents,
- select-all,
- Enter to commit,
- Escape to cancel.

Do not use raw key presses for text characters while IME is active. Text
characters should come from IME commits or Bevy logical-key text paths so dead
keys and composed characters work.

Minimum shortcut coverage for the IME example and first implementation:

| Command | macOS | Windows/Linux |
| --- | --- | --- |
| Move left/right | Left / Right | Left / Right |
| Extend selection | Shift+Left / Shift+Right | Shift+Left / Shift+Right |
| Move by word | Option+Left / Option+Right | Ctrl+Left / Ctrl+Right |
| Extend by word | Shift+Option+Left / Right | Shift+Ctrl+Left / Right |
| Start/end of line | Cmd+Left / Cmd+Right | Home / End |
| Extend to start/end | Shift+Cmd+Left / Right | Shift+Home / End |
| Select all | Cmd+A | Ctrl+A |
| Delete backward/forward | Backspace / Delete | Backspace / Delete |
| Delete word backward/forward | Option+Backspace / Option+Delete | Ctrl+Backspace / Ctrl+Delete |
| Commit field | Enter when not composing | Enter when not composing |
| Cancel/close | Escape when not composing | Escape when not composing |

While IME preedit is active, Enter/Escape/arrows belong to the IME/editor
composition layer and must not trigger field-level commit, cancel, or navigation
unless the transition table explicitly maps that case. Clipboard shortcuts can
be added later; the first example may display them as out of scope if the buffer
does not implement clipboard integration yet.

App-owned surfaces can route non-text controls before buffer commands. The IME
crate does not build a command palette or fuzzy-search widget; it only exposes
the focused single-line session and a way for app code to decide whether a key
is a text-editing command or an app-surface action.

The routing order is:

1. active IME preedit consumes composition keys first,
2. the session policy maps keys to app-surface actions or text-buffer commands,
3. unhandled input can fall through only if the focus/capture policy allows it.

Example app-surface defaults for a caller-owned popup:

| Input | Default popup action when not composing |
| --- | --- |
| Up / Down | move result selection |
| PageUp / PageDown | move result selection by page if results are paged |
| Enter | accept selected result if one is active, else request text commit |
| Escape | close/cancel popup |
| Pointer click on result row | accept or focus that result inside the editor focus scope |
| Pointer click outside popup | apply the configured outside-blur policy |

Because Bevy documents that `Window::ime_enabled` changes text input delivery,
the implementation must verify on macOS, Windows, and Linux which non-text
`KeyboardInput` events still arrive while IME is enabled. If a platform does not
deliver a shortcut through `KeyboardInput`, the editor needs an explicit fallback
path before claiming support for that shortcut.

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

- IME `Session` resource,
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
- support selection, word movement/deletion, start/end, select-all, and
  Backspace/Delete/Left/Right across macOS, Windows, and Linux shortcuts.

Acceptance: composed text input works for a single-line field, preedit is shown
without changing the stored value, IME commit inserts text into the edit buffer
without requesting field commit, field commit is requested only by a field-level
cause, selection and word shortcuts update the buffer snapshot, and cancel
clears composition.

### Phase 3 — Screen-space rendering

Render the transient editor:

- project target bounds each frame,
- draw the editor at the projected position,
- draw caret and composing text,
- apply basic viewport clamping,
- visually mark the target field as active.

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

## Team review follow-ups

The team review produced no premise challenges. Its findings collapse into four
implementation decisions to carry into the plan:

### D1 — Stable field identity and hit testing

Status: accepted

Editable values need a stable `EditableFieldId` or `PanelFieldId` supplied by
the authoring API. Frame-local layout element indices and render command indices
can still be used for geometry lookup, but they must not be the semantic target
for picking, anchoring, or commit.

Field activation should be a dedicated resolver: convert the pointer hit into
panel-local layout coordinates, filter editable fields from the latest computed
layout while respecting clip rects and draw order, then emit a typed start
request such as `EditStartRequested { panel, field_id, window, camera }`.
Use Bevy picking for the activation path; the extra work is the diegetic
field-resolution layer after the panel hit is known.

### D2 — Window-scoped IME and input ownership

Status: accepted

An edit session should store the activation window and camera. IME handling
should use a per-window lease or owner token, filter IME and keyboard input by
that window, and release IME only if the editor still owns the lease.

While editing, editor keyboard and pointer events should be consumed or marked
handled. Camera/app systems need an integration point, such as
`CameraInputDisabled` or a dedicated input blocker, so Enter, Escape, arrows,
wheel, activation clicks, and blur clicks do not also drive the scene.
Use this as the default until implementation exposes a concrete conflict.

### D3 — Explicit single-line editor state and buffer

Status: accepted

The editor should not be a mutable bag of raw strings. Use a closed editor state
model for idle, editing, composing, commit rejected, committed, and canceled
transitions. Emit lifecycle events from those transitions.

Use an internal `SingleLineEditBuffer` with committed text, preedit text,
validated cursor/range types, and UTF-8-safe editing operations. Split commit
into request, validation, rejection, and applied commit so invalid numeric input
does not emit a misleading committed event.
Prefer typestate or closed transition APIs where practical so invalid editor
states are not representable and systems do not grow scattered conditional
guards around every operation.

### D4 — Target invalidation, projection ordering, and rendering path

Status: accepted

The session should store panel entity, field id, activation window, activation
camera, starting tree revision, and original value. Each frame, re-resolve the
field from the current computed layout, project after transform and camera data
are current, and apply a deterministic stale-target policy for missing
panel/field/window/camera, tree revision changes, and external value changes.

The transient editor should be a screen-space `DiegeticPanel`, not a separate
overlay renderer. For world-panel editing, anchor that screen-space panel to the
projected target field each frame. Cursor and preedit visuals can be
editor-specific, but window selection, camera/layer behavior, text styling, and
cleanup should reuse the existing screen-space systems where practical.

The same IME surface should also be usable from app-authored screen-space
panels without a world-panel target field. This lets a client build its own
screen-space affordance, such as a command palette, by creating a screen-space
panel, putting an IME text-entry field in it, and owning any result rows or
actions outside the IME module.

## Proposed module structure

The module boundary should be `ime`, not `editing`. IME is the industry term of
art, and this crate has only one text-entry/editing system planned. Avoid
introducing a generic editing namespace unless another editing domain appears.

```text
crates/bevy_diegetic/src/ime/
  mod.rs          // public facade, ImePlugin, ImeSystems, system wiring
  ids.rs          // SessionId, CommitAttemptId
  target.rs       // Target and target adapters
  events.rs       // public lifecycle events and validation responses
  requests.rs     // public app intent events: open, focus, request commit, cancel
  session.rs      // active session resource and session-owned entities
  state.rs        // closed transition table / typestate API
  buffer.rs       // SingleLineEditBuffer, cursor, ranges, preedit
  shortcuts.rs    // macOS/Windows/Linux key mapping to ImeCommand
  input.rs        // internal ImeCommand translation from keyboard, pointer, Bevy Ime
  focus.rs        // focus lease, input blocker, inside/outside policy
  anchor.rs       // AnchorSnapshot and projection validity
  surface.rs      // screen-space panel surface, caret geometry, overlay root
  field.rs        // editable panel-field adapter
```

`mod.rs` owns `ImePlugin`; there is no `plugin.rs`. Bevy plugin definitions live
with their struct in this codebase, and the module root should read as the table
of contents for the IME feature.

If a public anchor type named `Ime` becomes useful, it should live in this module
root. Do not force it in just to make names look symmetrical; `bevy_window::Ime`
already names the raw Bevy OS event stream, while this module owns the higher
level session API.

`requests.rs` is not a command-palette implementation. Its request events are
typed app intents such as "open an IME session for this field", "request commit",
or "cancel this session". Internally, `input.rs` can translate keyboard,
pointer, and `bevy_window::Ime` events into an `ImeCommand` enum consumed by the
transition function. These are state-machine inputs, not Bevy `Commands`, and
not a built-in fuzzy-search feature.

### Required external touch points

- `src/lib.rs`: add `mod ime`, install `ImePlugin` from `DiegeticUiPlugin`, and
  re-export only the approved public IME surface.
- `src/layout/builder.rs`: add field metadata authoring such as
  `El::field_id(...)` or `LayoutBuilder::editable_text(...)`.
- `src/layout/element.rs`: store generic panel field metadata on elements.
- `src/layout/render.rs` / layout result data: carry field identity far enough
  for hit resolution and field-bound lookup.
- `src/panel/diegetic_panel.rs`: expose crate-internal computed field records
  from `ComputedDiegeticPanel`.
- `src/panel/mod.rs`: order `ImeSystems` relative to `PanelSystems`.
- `src/screen_space/mod.rs`: add a supported follow-anchor path for the transient
  screen-space IME panel.
- `examples/ime.rs`: canonical example covering composed/accented/CJK input,
  selection, platform shortcuts, commit, cancel, and an app-owned popup surface.

## Second team review refinements

### R0 — Namespace and public/private boundary

Status: proposed

Short IME names live under `bevy_diegetic::ime`, for example `ime::SessionId`
and `ime::TextChanged`. The crate root should not glob-re-export the full IME
event set. Public API starts with the IME lifecycle events, request events,
`Target`, session/attempt ids, field specs, and possibly `ImePlugin` /
`ImeSystems`.

`DiegeticUiPlugin` should install `ime::ImePlugin` by default. `ImePlugin` lives
in `ime/mod.rs`; do not create a separate `plugin.rs`. `ime::ImeSystems` should
expose the ordering points external code needs,
such as `AcquireFocusLease`, `PublishInputBlockers`, `ProcessInput`,
`ResolveAnchors`, `UpdateSurface`, `UpdateImePosition`, and `Cleanup`.
`PublishInputBlockers` must run before app camera/input consumers;
`UpdateImePosition` must run after caret geometry is available.

Implementation machinery such as `SingleLineEditBuffer`, transition state,
target adapters, anchor snapshots, follow-anchor components, overlay root
components, and screen-space internals starts as `pub(crate)` until a concrete
external integration needs it.

### R1 — Split core sessions from field adapters

Status: proposed

The core IME surface should be a shared single-line session API, not a
world-field-only abstraction. Model the target as a closed enum such as
`WorldPanelField`, `ScreenPanelField`, and `AppOwned`, so projected diegetic
field editing and caller-owned screen-space fields reuse lifecycle, buffer, IME
lease, and focus logic without fake world targets.

Keep the target enum from turning into scattered `match` statements. Split the
session model into roles:

- **target/owner**: semantic backing identity and app ownership,
- **anchor provider**: projected world field, screen field, or fixed screen rect,
- **focus scope**: what counts as inside/outside the editor,
- **validation/apply sink**: where commit requests and applied values go,
- **edit surface**: spawned transient panel or existing screen-panel field.

Each target variant should provide these capabilities through typed adapters so
editor systems can call common operations instead of branching on optional
fields.

### R2 — Make editable identity explicit in the layout API

Status: proposed

Stable field identity should be authored in the layout tree, not inferred by the
resolver. Add an `EditableFieldId`/`PanelFieldId` newtype and an authoring path
such as `LayoutBuilder::editable_text(...)`, `El::field_id(...)`, or an
editable-field spec. Propagate that metadata into computed field records with
bounds, style, clip state, draw order, and field id.

Treat field metadata as a generic panel contract, not only editable text. A
`PanelFieldRecord` can describe roles such as editable text, result row, action,
focus target, or other hit-testable panel regions. Editable text fields should
carry an `EditableFieldSpec` with field id, value kind or parser key, backing
target or event sink, display/style metadata, and edit policy. Validate field-id
uniqueness where the authoring API can catch it.

### R3 — Use session IDs and typed lifecycle events

Status: proposed

Every start, text-change, commit-request, validation-rejected, applied, and
canceled event should carry a `SessionId` plus a typed target.
Delayed validation or apply responses must match the current active session
before mutating editor state. Reserve "applied" for the point where backing
state actually changed.

Use `CommitAttemptId` for individual commit attempts inside a session. Enter,
blur, and app-requested commits can create multiple attempts before the session
ends, so delayed validation/apply responses must match both the session and the
current attempt. Add explicit `PendingValidation` and `PendingApply` states; the
IME lease, input capture, and editor panel remain alive until the attempt is
rejected, applied, or a hard terminal cause ends the session.

App-owned screen-space use should have request-event ergonomics, not only raw
lifecycle events. Provide request events for opening, focusing, requesting
commit, and canceling a text-entry session; responses still carry the session
id. These requests are IME session intents, not Bevy `Commands` and not a
command-palette implementation.

### R4 — Specify IME lease, input blocker, and cleanup ownership

Status: proposed

Represent IME ownership with a typed lease token, not only a window entity.
Acquire the lease before editing, release idempotently only if the owner still
matches, and treat lease loss/window close/focus loss as terminal editor
transitions. Pair this with a window/camera-scoped input blocker; pointer
propagation alone is not enough because other systems can read global input
resources.

IME ownership and input capture should be one typed focus lease bundle keyed by
session and window, not two independent resources. Session start succeeds only
after acquiring that bundle. Losing the bundle, closing the window, or losing
window focus follows one terminal transition path that releases any remaining
state.

Publish input blockers early enough for same-frame correctness. Add an explicit
system set, such as `PublishInputBlockers`, that runs before camera/input
consumers read `ButtonInput`, mouse motion, or wheel resources. External systems
should have a clear ordering point and a scoped focus/capture policy for
keyboard, pointer, wheel, and camera input.

### R5 — Define composition, blur, and validation policy

Status: proposed

Field-level Enter/Escape/navigation should not fire while IME composition is
active. During preedit, keyboard events belong to the IME/editor layer. Model
commit causes such as Enter, blur, and app requests, and define blur
outcomes for invalid input, inside-editor clicks, result-row actions, window
focus loss, target invalidation, and new-session activation.

Write this as a transition table, not policy prose scattered across systems.
Systems translate Bevy events into `ImeCommand`, `CommitCause`, `BlurCause`,
and `CancelCause`; only the IME transition function mutates state. While
composition is active, Enter/Escape/navigation should either be consumed by the
IME layer or explicitly mapped to composition behavior, never field-level
commit/cancel by accident.

Initial transition sketch:

| State | Input | Transition / event |
| --- | --- | --- |
| `Idle` | start request + focus lease acquired | `Editing`, emit `Started` |
| `Idle` | start request + focus lease denied | stay `Idle`, no editor surface |
| `Editing` | text edit command | stay `Editing`, emit `TextChanged` |
| `Editing` | `Ime::Preedit` | `Composing`, emit `TextChanged` with `Preedit` |
| `Composing` | preedit update | stay `Composing`, emit `TextChanged` |
| `Composing` | `Ime::Commit` | `Editing`, insert committed text, clear preedit, emit `TextChanged` |
| `Composing` | Enter/Escape/navigation | consume or map to composition behavior; no field commit/cancel |
| `Editing` | Enter / commit blur / app-requested commit | `PendingValidation`, emit `CommitRequested` |
| `PendingValidation` | `RejectCommit` | `Editing`, emit `ValidationRejected` and keep focus |
| `PendingValidation` | `AcceptCommit` | terminal apply path, mutate backing state, emit `Applied`, clean up |
| `Editing` or `Composing` | soft cancel | terminal cancel path, emit `Canceled`, clean up |
| any active state | window close / lease loss / target destruction | terminal cancel path, emit `Canceled`, clean up |

Separate soft blur from hard teardown. Inside-editor clicks and result-row
actions remain in the session focus scope. Outside blur may request commit,
cancel, or reject-and-refocus depending on session kind. Hard causes such as
window close, lease loss, and target destruction cancel deterministically unless
an explicit recovery path exists. Defaults can differ by session policy: world
numeric edits usually keep invalid input open, while an app-owned popup may
close on outside blur if the app requests that behavior.

### R6 — Define editor scheduling, anchor mutation, and caret IME position

Status: proposed

Add explicit IME system sets and ordering. Re-resolve target layout after
panel layout, project from current transform/camera data, mutate the
screen-space editor panel through a supported internal anchor path, and update
`Window::ime_position` from the final caret/preedit rectangle after the editor
surface has laid out. Field bounds alone are not precise enough for the OS IME
candidate popup.

Each target adapter should emit a typed `AnchorSnapshot` or equivalent:
window entity, logical client-area rect, visible clip rect, target revision,
validity/depth state, and optional camera/viewport provenance. One ordered
screen-space owner consumes that snapshot through an internal component or
request such as `ScreenPanelFollowAnchor`, rather than ad hoc transform writes.

The transient editor panel should have a session-owned overlay root entity, for
example `Overlay { session_id }`, with focus-scope metadata. It is
re-anchored from the target while valid, but destroyed only through terminal
session transitions. Editor chrome and result rows should be descendants of this
focus scope so they do not count as outside-click blur targets.

Caret geometry is a first-class output of the editor surface. A
`SingleLineEditLayout` or `CaretGeometry` result derived from the edit buffer and
text shaping drives both caret/preedit rendering and `Window::ime_position`.
Do not derive IME candidate position from field bounds, glyph mesh entities, or
render-command bounds. Choose and document whether the first implementation
accepts a one-frame projection snapshot or implements a stricter same-frame
freshness path.

### R7 — Example acceptance matrix

Status: proposed

The IME example should prove the contract, not only render a text box:

| Scenario | Expected signal |
| --- | --- |
| Accent/dead-key input | `TextChanged` updates after composed text enters the buffer |
| CJK preedit | `TextChanged` carries active `Preedit` with cursor/range |
| CJK commit | `TextChanged` reflects committed buffer text with preedit cleared; no `CommitRequested` until a field-level commit cause |
| Shift-selection | `BufferSnapshot.cursor` enters `Selection` without losing text |
| Platform word movement | macOS Option and Windows/Linux Ctrl move by word |
| Select-all | Cmd+A / Ctrl+A selects the buffer |
| Enter commit | emits `CommitRequested` only when not composing |
| Escape close | cancels/closes only when not composing; composing Escape follows composition policy |
| App-owned popup navigation | non-text navigation emits an app-surface action/request or is handled by a documented hook |
| Multi-byte boundary | accented, CJK, or emoji text does not split invalid UTF-8 boundaries during selection, deletion, or cursor movement |
| Activation frame capture | double-click starts editing without moving the camera or triggering another app action in the same frame |
| Invalid numeric commit | malformed, empty, non-finite, and out-of-range values emit rejection, keep focus, and do not emit `Applied` |
| Rejection recovery | editing after rejection preserves the buffer/cursor, creates a fresh attempt on retry, ignores stale responses, and applies exactly once on success |
| Outside blur | invalid blur keeps focus and consumes the click; valid blur forwards only a captured semantic action token |
| App-owned popup priority | composition input wins before popup actions; row navigation and selection do not mutate text |
| Anchor freshness | documented first-version freshness policy holds; no stale caret or candidate-popup position persists beyond the accepted latency |

## Third team review refinements

The third team review produced no premise challenges. Cycle 1 added the
following proposed implementation decisions.

### R8 — Make IME lease and input blocking authoritative

Status: proposed

Severity: critical

Source dimension: risk and correctness

Class: design-improvement

IME ownership should have one actual writer for `Window::ime_enabled` and
`Window::ime_position`. Represent the active owner with a per-window typed
lease such as `ImeLease { session_id, owner, previous_state }`; acquire it
before editing, release it idempotently only if the owner still matches, and
make lease loss, window close, or focus loss terminal transitions.

IME focus and input capture should publish before camera or app input reads the
same raw keyboard, pointer, mouse-motion, or wheel data. `bevy_diegetic` should
publish its own crate-owned `ImeInputBlockers` or focus-lease resource in
`PreUpdate`; it should not depend directly on `bevy_lagrange` internals. Apps
or examples that use Lagrange should add a small adapter that translates the
IME blocker into Lagrange's public camera-suppression hook before Lagrange
routing or adapter injection.

Activation-frame capture is part of the contract. The field activation adapter
must either run early enough to publish the blocker before app/camera routing,
or emit a consumed-input token that the integration adapter honors for that
same frame. The example should prove that the double-click or activation
gesture starts editing without also orbiting, panning, zooming, selecting, or
triggering another app action in the same frame.

### R9 — Split public scheduling from internal IME sequencing

Status: proposed

Severity: important

Source dimension: architecture

Class: design-improvement

Expose only durable integration points in public `ime::ImeSystems`, such as
publishing input blockers and applying the final IME candidate-popup position.
Keep lifecycle sequencing, input translation, anchor resolution, surface update,
and cleanup in crate-private `ImeInternalSystems` so the module can evolve
without freezing every internal step as public API.

The first implementation should use a documented one-frame anchor/caret
freshness policy unless implementation work proves the stricter same-frame path
is cheap. That latency is acceptable only if it is explicit, bounded, and tested:
no stale anchor, caret, validation feedback, or `Window::ime_position` may
persist beyond the documented frame.

Add a public `ScreenSpaceSystems::PositionPanels` ordering point for the
existing screen-space positioning owner. `ImeInternalSystems::UpdateImePosition`
should run after that ordering point and after caret geometry is available.
Editor tree changes caused by text edits should happen before panel layout;
caret geometry used for IME positioning must come from the laid-out edit
surface, not raw field bounds.

### R10 — Put field records at the panel boundary

Status: proposed

Severity: important

Source dimension: architecture and type system

Class: design-improvement

Field identity and hit resolution should not depend on render commands or raw
layout indices. Add a panel-layer `PanelFieldRecord` produced from layout
metadata and stored on `ComputedDiegeticPanel` as crate-internal data. Include
stable `PanelFieldId`, role/spec, bounds, effective clip, draw order, style
snapshot, panel-local geometry provenance, tree revision, computed field epoch,
and the crate-private source element locator.

Use distinct newtypes for semantic field identity and implementation locators,
for example `PanelFieldId`, `LayoutElementIndex`, `RenderCommandIndex`, and
`ComputedFieldIndex`. Public targets carry only semantic `PanelFieldId`;
locators stay internal. Field-id uniqueness validation should return a typed
error keyed by `PanelFieldId`.

`PanelFieldId` is panel-local. Session identity should use an internal
`PanelFieldKey { panel, field_id }`; public events continue exposing the
semantic `panel + field_id` pair and never expose layout or render indices.
Duplicate-id errors should include the panel plus internal source locators.

Field metadata participates in layout-tree change classification and cache
invalidation. Adding or changing a field id, role, parser key, apply sink, or
editable policy must rebuild `PanelFieldRecord`s and rerun duplicate-id
validation even when render commands can otherwise be reused.

Panel-field activation should be a dedicated adapter: consume a panel hit,
convert it into panel-local layout coordinates, resolve field records with clip
and draw-order rules, apply double-click policy, and emit `EditStartRequested`.
The IME session code should consume typed start requests instead of owning
Bevy picking details.

### R11 — Keep follow-anchor ownership in `screen_space`

Status: proposed

Severity: important

Source dimension: architecture and risk

Class: design-improvement

The transient editor should not mutate screen-space panel transforms or sizing
ad hoc. Add a generic crate-private follow-anchor path in `screen_space`, such
as `ScreenPanelFollowAnchor`, consumed by the existing screen-space owner. The
IME anchor system publishes an anchor snapshot; `screen_space` applies it and
reports the final rect that caret layout and IME positioning use.

Make coordinate domains explicit in the anchor snapshot. Carry the concrete
window entity, viewport-local rect, window-logical rect, camera viewport
provenance, visible clip rect, scale-factor/logical-coordinate basis, target
revision, and a projection-validity enum such as `Valid`, `BehindCamera`,
`Offscreen`, `Clipped`, or `MissingTarget`. The follow-anchor consumer operates
only on window-logical coordinates tied to `WindowRef::Entity(activation_window)`.
Resolve `WindowRef::Primary` at session start and spawn the editor against that
concrete window so primary window changes cannot retarget an active IME lease.
If viewport provenance is missing or cannot be proven current, projection is
invalid rather than best-effort.

### R12 — Split built-in apply from app-owned apply

Status: proposed

Severity: important

Source dimension: correctness and risk

Class: design-improvement

Commit handling needs one owner for each backing mutation. Built-in fields use
an internal validate/apply sink before emitting `Applied`. App-owned sessions
receive `CommitRequested` and respond with an app-specific accepted/rejected
response that means the app has already applied or rejected the change; the
response should carry an opaque commit token plus optional display/revision
metadata, not a universal `Output` value the IME core must understand.

Internally model `ActiveSession` as a closed enum with `Editing`, `Composing`,
`PendingValidation`, `PendingApply`, and terminal transitions. Keep
`CommitAttemptId` only in pending variants and expose transition methods that
produce effects, so stale ECS response events cannot resolve commits outside a
current pending attempt.

App-owned mutation is valid only while the attempt is still current. Provide a
short-lived apply token or an `ImeCommitAuthority::is_current(session_id,
attempt_id)` check that app apply sinks call immediately before mutating their
model. Stale responses are ignored by the IME, but stale app-owned mutations
must be prevented before they happen.

Pending validation and pending apply quarantine input by default. New
`Ime::Preedit`, `Ime::Commit`, keyboard, pointer, and open-session requests are
consumed, ignored, or explicitly queued by policy; they do not mutate the
buffer. Hard lifecycle causes still take the terminal cancel path and invalidate
the attempt.

### R13 — Make field specs encode parse and apply contracts

Status: proposed

Severity: important

Source dimension: user impact and type system

Class: design-improvement

The editable-field spec should be a closed enum split by ownership mode, for
example `EditableFieldSpec::BuiltIn(BuiltInFieldSpec)` and
`EditableFieldSpec::AppOwned(AppOwnedFieldSpec)`. Built-in specs pair
`PanelFieldId` with an explicit value kind, typed range constraints,
validation policy, display formatter, live-preview policy, apply sink, and
typed rejection reasons. App-owned specs carry parser/apply keys or event
sinks instead of making the IME core understand every app output type.

Numeric fields need a two-stage round-trip contract: display value to editable
text, permissive edit buffer syntax while typing, strict commit parser on
commit, then deterministic display formatting after successful apply. Partial
edit strings such as `-`, `.`, `1.`, empty strings, and unit/suffix edits may be
valid while editing but rejected on commit according to the field policy.
Acceptance should include malformed input, empty input, partial numeric input
while editing, non-finite floats, out-of-range values, successful finite floats,
integer ranges, clamping-versus-rejection behavior, and deterministic
post-commit formatting.

### R14 — Keep edit-buffer invariants behind operations

Status: proposed

Severity: important

Source dimension: type system

Class: design-improvement

`BufferBoundary` and `PreeditBoundary` are necessary but not sufficient. Keep
their constructors private to `SingleLineEditBuffer`, and expose editing
operations such as `move_left_grapheme`, `move_word_right`,
`delete_word_backward`, `replace_selection`, and `apply_preedit`.

Represent selection direction explicitly, for example with
`Selection { anchor, focus }`, so shift-selection, word movement, replacement,
and deletion do not collapse into a sorted range too early. User-facing motion
and deletion should operate on grapheme or word boundaries, then convert to
validated buffer positions inside the buffer implementation.

Public snapshots should preserve directed selection too, for example
`SelectionSnapshot { anchor, focus }`, so renderers can identify both the
selected span and active caret edge. Opaque public boundary types may expose
read-only byte offsets, but external code should not construct them directly.
The implementation should name its grapheme/word-boundary service or dependency
before claiming platform shortcut coverage.

### R15 — Specify stale-target and external-value policy

Status: proposed

Severity: important

Source dimension: correctness

Class: design-improvement

The active session should store enough provenance to decide whether editing can
continue: panel entity, semantic field id, activation window, activation camera,
starting tree revision, computed field epoch, original text, and an adapter-owned
value snapshot.

If the field id survives a layout or tree revision change, re-resolve geometry
and continue. If the backing value changes incompatibly while editing, reject
or cancel with a stale-target reason according to session policy. If the panel,
field, window, camera, lease, or required viewport provenance disappears, cancel
through one deterministic terminal path and release IME ownership.

Each validation/apply adapter should expose a `ValueSnapshot` with display text,
optional revision/hash, and conflict policy. The IME core compares snapshots and
policies; it does not inspect backing models directly. If no revision,
validation source, or compatible conflict policy exists, default to cancel or
reject on tree/layout revision changes instead of applying optimistically.

Use one internal terminal-cause detector before cleanup. It should emit a
single terminal transition for lease loss, window close, camera despawn, panel
despawn, field disappearance, focus loss, or invalid projection. Cleanup is
idempotent and session-owned: release the lease if still owner, despawn the
overlay root if still alive, clear blockers, and drop pending attempts.

### R16 — Define validation feedback and blur ordering

Status: proposed

Severity: important

Source dimension: user impact and correctness

Class: design-improvement

Define visible editor states for normal editing, composing, pending validation,
pending apply, rejected/error, committed, and canceled. Built-in fields should
render a deterministic invalid style and optional short reason after
`ValidationRejected`; app-owned sessions can consume the rejection or provide a
renderer/policy hook. No `Applied` event or backing mutation occurs until the
matching commit attempt is accepted.

Rejected-state recovery is part of the contract. Rejection keeps focus,
preserves editable invalid text and cursor/selection state, and lets the user
correct the buffer. A retry creates a fresh `CommitAttemptId`; stale rejection
or acceptance responses for older attempts are ignored. Escape from rejected
state cancels and restores the original value. A successful retry emits exactly
one `Applied`.

Make outside-pointer blur testable. Default to consuming the blur click while
commit validation is pending; invalid input keeps focus and does not leak the
click; valid commit may replay or forward an outside action only when the
implementation has captured an explicit semantic target/action token before
validation. Do not replay raw input after validation; raw replay can hit a
different entity after layout changes or editor teardown. Add example rows for
valid outside blur, invalid outside blur, click inside editor chrome, click
result row, click another editable field, window focus loss, target
invalidation, stale async rejection, and correction after rejection.

### R17 — Name the app-owned popup input hook

Status: proposed

Severity: important

Source dimension: user impact

Class: design-improvement

App-owned screen-space surfaces need a concrete synchronous hook for deciding
whether a key or pointer event is a text-editing command or an app-surface
action. Name the public disposition shape before implementation, for example
`InputDisposition::{Edit(ImeCommand), SurfaceAction(AppSurfaceAction),
RequestCommit, RequestCancel, Blur(BlurPolicy), Consume, FallthroughAllowed}`.
It should run after active preedit/composition handling but before text-buffer
commands, outside-blur handling, or app fallthrough.

The example should prove that popup arrows move result selection without
mutating text, Enter accepts a selected result instead of requesting field
commit, row clicks stay inside the focus scope, and outside clicks follow the
configured blur policy. It should also include a platform matrix for shortcuts:
supported non-composing shortcuts, composition behavior while preediting, and
degraded or unsupported paths where platform IME delivery suppresses raw
keyboard events.

For blur commits, store only semantic action tokens captured before validation.
Forward them after successful apply only if their session, window, focus-scope,
and target provenance still match.
