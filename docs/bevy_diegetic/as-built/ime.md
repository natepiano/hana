# Single-line IME — implementation plan

> **Archived 2026-06-07 — implemented.** The plan's module layout is the
> current `src/ime/` directory, file for file: `activation`, `apply`,
> `buffer`, `editor`, `events`, `field`, `ids`, `input`, `lease`, `session`,
> `target`, with `ImePlugin` in `ime/mod.rs`. The in-body "now owns / now
> renders" notes were progress annotations written during implementation.
> Demonstrated by `examples/ime.rs`.

## Goal

Add single-line text editing for diegetic panel values while keeping the first
version pragmatic: values live on the diegetic panel or backing ECS data, but
the active editor is a transient screen-space affordance anchored to the
projected value.

This supports DAW-style interactions such as double-clicking a floating-point
value on a panel, editing it in place, and committing the parsed value back to
the panel when the user presses Enter or clicks away.

## Important decisions from review

The team review produced no premise challenges. These are the high-signal
decisions to carry into implementation:

- IME ownership is window-scoped and has one writer for `Window::ime_enabled`
  and `Window::ime_position`.
- `bevy_diegetic` publishes its own input blocker; apps bridge it into
  Lagrange or other camera/input systems instead of making IME depend on them.
- Activation-frame capture is required: the double-click that starts editing
  must not also drive the camera or another app action.
- Field identity is panel-local and semantic; computed field records live at
  the panel boundary and are not recovered from render commands.
- The transient editor follows anchors through `screen_space`; IME code does
  not write screen-panel transforms directly.
- App-owned apply responses mean the app already applied the change; the IME
  core must not carry a universal app output value or mutate app state again.
- Numeric editing uses permissive text while typing and strict parsing only at
  commit.
- Invalid commit recovery preserves focus, buffer text, and cursor/selection,
  then retries with a fresh commit attempt.
- App-owned popups use a synchronous input-disposition hook before text-buffer
  commands or app fallthrough.
- The first implementation accepts a bounded one-frame anchor/caret freshness
  policy unless a same-frame path proves cheap during implementation.

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

Possible field-authored component:

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
- editor lifecycle emits typed events that identify the current session and
  commit attempt, so app code can synchronize backing state against them.

## Event contract

Bevy provides the OS-facing input events through `bevy_window::Ime`. The IME
module should translate those low-level window events into session-scoped
events. App code should listen to these events, not raw `Ime`, unless it is
implementing another text-entry system.

Public IME types are curated crate-root exports with `Ime...` names. The
implementation module stays private; do not introduce a public `ime` module.

Every IME session event carries a `SessionId`. Events that are part of a
commit attempt also carry a `CommitAttemptId`, so delayed validation or apply
responses cannot affect a newer edit attempt in the same field.

```rust
pub struct SessionId(u64);
pub struct CommitAttemptId(u64);
pub struct CommitAuthorityToken(u64);
pub struct ValueRevision(u64);

pub enum AppliedResult {
    BuiltIn(BuiltInApplied),
    AppOwned {
        display_text: Option<String>,
        value_revision: Option<ValueRevision>,
    },
}

pub enum BuiltInApplied {
    Text(String),
    Float(f32),
    Integer(i64),
}

/// Semantic target of the session, not the rendered surface or anchor provider.
pub enum Target {
    WorldPanelField {
        panel: Entity,
        field_id: PanelElementId,
    },
    ScreenPanelField {
        panel: Entity,
        field_id: PanelElementId,
    },
    AppOwned {
        owner: Entity,
        field_id: PanelElementId,
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

pub struct SelectionSnapshot {
    pub anchor: BufferBoundary,
    pub focus: BufferBoundary,
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
    Selection(SelectionSnapshot),
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
    pub result: AppliedResult,
}

pub struct Canceled {
    pub session_id: SessionId,
    pub target: Target,
    pub cause: CancelCause,
}

pub struct AcceptCommit {
    pub session_id: SessionId,
    pub attempt_id: CommitAttemptId,
    pub token: CommitAuthorityToken,
    pub display_text: Option<String>,
    pub value_revision: Option<ValueRevision>,
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
actually changed. It should not contain a universal app-owned output value:
built-in fields may expose typed built-in applied data, while app-owned sessions
report only opaque display/revision metadata supplied by the app.

Direction matters:

- outbound events from the editor: `Started`, `TextChanged`,
  `CommitRequested`, and `Canceled`;
- inbound app/apply-sink responses: `AcceptCommit` and `RejectCommit`, matched
  by both `SessionId` and `CommitAttemptId`; app-owned acceptance also carries
  a current commit-authority token;
- outbound confirmation from the transition system after a response:
  `ValidationRejected` or `Applied`.

For crate-owned built-in fields, the apply sink can validate and mutate backing
state internally before emitting `Applied`. For app-owned state, app code
handles `CommitRequested`, verifies the attempt is still current, mutates its
model if valid, then sends `AcceptCommit` or `RejectCommit`. `AcceptCommit`
means the app has already applied the change; the IME core validates ids and
token, emits `Applied`, and cleans up without mutating app state again.

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

## Development Phases

### Phase 1 — Core session and public surface (complete)

Implement the data model and lifecycle:

- private `ime` module and `ImePlugin`,
- narrow crate-root exports for the approved public IME surface,
- session, attempt, target, and field-id types,
- typed lifecycle, request, validation, apply, and cancel events,
- IME `Session` resource,
- editable field spec and panel-field metadata authoring,
- request-opened activation entry point for already-resolved targets,
- single active session,
- Enter/Escape/blur handling.

Acceptance: opening an already-resolved editable target starts a session, Enter
emits a commit request, Escape emits cancel, public events carry stable session
and target identity, and only one session can be active.

#### Retrospective

**What worked:**

- `crates/bevy_diegetic/src/ime/` now owns the session ids, targets, field
  specs, lifecycle events, and active-session resource.
- Bevy observers fit the request/response lifecycle: `ImeOpenSession`,
  `ImeRequestCommit`, `ImeRequestCancel`, `ImeAcceptCommit`, and
  `ImeRejectCommit` all route through typed events.

**What deviated from the plan:**

- The previously planned public IME module was not implemented because the
  style guide disallows `pub mod`; public IME API is exposed through curated
  crate-root `Ime...` re-exports.
- Double-click activation moved out of Phase 1 because it depends on field hit
  resolution and panel-field records owned by Phase 2.

**Surprises:**

- Editable field metadata has to participate in layout-tree change
  classification immediately, otherwise later field-record caches could miss a
  changed field contract.

**Implications for remaining phases:**

- Phase 2 must bridge pointer/picking activation into `ImeOpenSession` instead
  of creating a separate session path.
- Phase 2 should keep the private-module plus crate-root re-export pattern and
  avoid introducing a public `ime` module.
- Phase 3 can replace the temporary Enter/Escape keyboard shortcut handling
  with the full IME-aware input transition table without changing the public
  lifecycle events.

#### Phase 1 Review

- Phase 2 now explicitly adds authored `PanelElementId` metadata, double-click
  activation, and replacement of the temporary global Enter/Escape shortcut
  path with lease-scoped command routing.
- Phase 3 now treats `Window::ime_position` as temporary/coarse until Phase 4
  supplies laid-out caret geometry.
- Phase 5 now names the missing app-owned commit-authority token or equivalent
  current-attempt guard.
- Phase 6 now builds app-owned popup behavior on top of the Phase 1
  `ImeOpenSession` / `ImeTarget::AppOwned` core instead of creating a second
  session path.
- Later plan text now follows the private `ime` module plus curated crate-root
  `Ime...` re-export pattern.

### Phase 2 — Field records, picking, and focus ownership (complete)

Add the panel boundary and ownership pieces that make session start reliable:

- computed `PanelFieldRecord`s on the panel boundary,
- replace the Phase 1 spec-only layout metadata with authored
  `PanelElementId` plus editable field spec metadata,
- field-id uniqueness validation,
- field hit resolution from panel-local coordinates,
- double-click activation from panel picking,
- window-scoped IME lease,
- crate-owned input blocker,
- activation-frame capture for the starting gesture,
- replace Phase 1's temporary global Enter/Escape shortcut path with
  lease-scoped command routing,
- terminal cleanup for lease loss, focus loss, window close, and stale targets.

Acceptance: activation resolves a semantic panel field instead of a render
command, the starting click does not also drive camera or app input, IME
ownership has one writer, and terminal causes clean up the session idempotently.

#### Retrospective

**What worked:**

- `PanelFieldRecord` now lives on `ComputedDiegeticPanel`, so activation and
  later anchoring can resolve `PanelElementId` without reading render commands.
- Panel double-click activation now routes through the Phase 1 `ImeOpenSession`
  observer and uses `ImeInputBlocker` for window-scoped ownership.

**What deviated from the plan:**

- `ComputedDiegeticPanel::set_result` kept its old public signature; the panel
  layout system uses an internal setter to attach field records.
- Phase 2 records duplicate field ids and ignores duplicated records during hit
  resolution instead of failing panel layout.

**Surprises:**

- The Phase 1 `ImeOpenSession` request needed an explicit window entity before
  lease ownership and terminal cleanup could be reliable.
- Commit requests need the field spec copied from the active session, otherwise
  later apply sinks cannot validate without looking back into layout metadata.

**Implications for remaining phases:**

- Phase 3 can keep `ImeInputBlocker` as the routing gate while replacing the
  Enter/Escape-only command path with the full IME-aware transition table.
- Phase 4 should consume `PanelFieldRecord::bounds` from
  `ComputedDiegeticPanel` for projection instead of re-resolving field ids.
- Phase 5 can validate against `ImeCommitRequested::field_spec` without
  assuming the panel layout tree is still available or unchanged.

#### Phase 2 Review

- Phase 3 now explicitly replaces the raw session text with an editing state
  machine, single-line buffer API, `ImeTextChanged`, and buffer snapshots.
- Phase 3 now adds durable IME/input system sets so `ImeInputBlocker` has a
  documented early read point for camera and app adapters.
- Phase 4 now captures camera/viewport provenance and extends field records or
  derives anchor snapshots before editor projection.
- Phase 5 now adds a backing binding or built-in apply-sink contract before
  built-in values can be parsed and written.
- Phase 5 keeps commit-authority work because session/attempt checks alone do
  not give app code a pre-mutation current-attempt guard.
- Phase 6 now narrows app-owned work to focus, routing, popup behavior, and
  examples on the existing `ImeOpenSession` / `ImeTarget::AppOwned` path.
- Phase 6 now includes duplicate field-id diagnostics and ambiguous-field
  non-activation coverage.

### Phase 3 — IME and keyboard input (complete)

Wire the active session to Bevy window IME:

- replace the raw Phase 2 session buffer with a closed editing state machine
  that separates editing, composing, pending-commit, and terminal states,
- introduce the single-line edit buffer API and the `ImeTextChanged` /
  buffer-snapshot surface,
- publish durable IME/input system sets so `ImeInputBlocker` has an early,
  documented read point for camera and app input adapters,
- toggle `Window::ime_enabled`,
- handle `Ime::Preedit`,
- handle `Ime::Commit`,
- maintain committed buffer plus composing string,
- publish a temporary coarse `Window::ime_position` only until Phase 4 supplies
  laid-out caret geometry,
- support selection, word movement/deletion, start/end, select-all, and
  Backspace/Delete/Left/Right across macOS, Windows, and Linux shortcuts,
- keep buffer boundaries private behind UTF-8-safe editing operations.

Acceptance: composed text input works for a single-line field, preedit is shown
without changing the stored value, IME commit inserts text into the edit buffer
without requesting field commit, field commit is requested only by a field-level
cause, selection and word shortcuts update the buffer snapshot, and cancel
clears composition.

#### Retrospective

**What worked:**

- `ime/buffer.rs` now owns UTF-8-safe single-line editing, selection, word
  movement/deletion, preedit snapshots, and `ImeTextChanged`.
- `ime/input.rs` now routes Bevy `Ime` and `KeyboardInput` through
  `ImeInputBlocker`, with `ImeSystemSet` ordering for external input systems.

**What deviated from the plan:**

- Terminal teardown remains represented by removing `ActiveImeSession.active`
  after emitting `ImeCanceled`/`ImeApplied`, not by retaining a stored terminal
  enum variant.
- `Window::ime_position` is still cursor-position/zero fallback only; real
  caret geometry stays in Phase 4.

**Surprises:**

- Bevy 0.19 exposes text through both `bevy_window::Ime` messages and
  `KeyboardInput::text`, so Phase 3 needed a per-frame guard to avoid duplicate
  text insertion when platform IME events arrive.
- Opening a session needs an initial `ImeTextChanged` so Phase 4 can render the
  editor from a snapshot without reconstructing buffer state.

**Implications for remaining phases:**

- Phase 4 should consume `ImeBufferSnapshot`/`ImeTextChanged` directly for
  editor text, selection, caret, and preedit rendering.
- Phase 4 should replace the coarse `Window::ime_position` writer in
  `ime/input.rs` once caret layout exists.
- Phase 5 can rely on pending-commit state blocking further edits until an
  accept/reject response returns.

#### Phase 3 Review

- Phase 4 now explicitly renders from `ImeTextChanged`/`ImeBufferSnapshot`
  instead of reading `ActiveImeSession`.
- Phase 4 now adds a later caret/IME-position system set and makes it the final
  `Window::ime_position` writer after editor layout.
- Phase 4 now builds only the validation-feedback slot; Phase 5 populates it
  from invalid commit/rejection results.
- Phase 4 and Phase 5 now name outside-blur classification and policy as
  implementation work before Phase 6 example coverage.
- Phase 5 now calls out the public current-attempt authority guard before
  app-owned backing-state mutation.
- Phase 6 now treats platform shortcut work as coverage of the Phase 3 command
  router and places app-owned input disposition before built-in command mapping.

### Phase 4 — Screen-space rendering and anchoring (complete)

Render the transient editor:

- consume `ImeBufferSnapshot` from `ImeTextChanged` as the editor source of
  truth for text, selection, caret, and preedit, without reading
  `ActiveImeSession`,
- capture camera and viewport provenance for field-authored sessions before
  projecting panel fields,
- extend `PanelFieldRecord` or derive an anchor snapshot with effective clip,
  style, tree revision or field epoch, and stale-target checks needed by the
  editor,
- project target bounds each frame,
- anchor through the supported `screen_space` follow-anchor path,
- draw the editor, caret, selection, a validation-feedback slot, and composing
  text,
- apply basic viewport clamping,
- add a later caret/IME-position system set after editor layout and move the
  final `Window::ime_position` write there, replacing the Phase 3 coarse
  cursor-position fallback,
- classify inside-editor and outside-editor pointer hits so outside blur can
  request commit/cancel policy instead of falling through as ordinary panel
  input,
- visually mark the target field as active.

Acceptance: the editor tracks a moving camera/panel, remains readable off-angle,
the OS candidate popup appears near the editor/caret, and the documented
anchor/caret freshness policy is bounded and tested.

#### Retrospective

**What worked:**

- `ime/editor.rs` now renders the transient editor from `ImeTextChanged` /
  `ImeBufferSnapshot`, including insertion caret, selection highlight,
  preedit text, validation text, viewport clamping, and final caret-based
  `Window::ime_position`.
- Activation now captures the picked camera in `PendingImePanelAnchor`, so
  field-authored sessions project `PanelFieldRecord::bounds` through the same
  camera path that produced the click.

**What deviated from the plan:**

- Phase 4 derives a private anchor snapshot instead of extending
  `PanelFieldRecord`; the computed panel record remains focused on panel-local
  field identity, bounds, spec, and display text.
- The active target is visually marked by the overlaid screen-space editor,
  not by mutating the source panel's authored tree.
- Outside pointer blur is classified and captured in `ImeBlurIntent`, but
  Phase 5 still owns the commit/cancel policy.

**Surprises:**

- The editor panel can reuse the existing `screen_space` panel path with a
  small internal `DiegeticPanel::set_screen_position` mutator rather than a
  separate renderer.
- Short non-empty buffers needed caret math separate from the empty-buffer
  fallback so candidate popup placement does not drift toward the left edge.

**Implications for remaining phases:**

- Phase 5 should consume `ImeBlurIntent` or replace it with the final blur
  policy, then emit `ImeCommitCause::Blur` / `ImeCancelCause::Blur`.
- Phase 5 can populate the existing validation slot by returning
  `ImeRejectCommit`; `ime/editor.rs` already keeps the editor open and redraws
  the rejection text.
- Phase 6 should add example coverage for the one-frame anchor freshness
  policy and app-owned fallback anchoring, since Phase 4 only gives app-owned
  sessions a cursor-position fallback.

#### Phase 4 Review

- Phase 5 now treats session/attempt stale-response checks and
  `ImeCommitRequested::field_spec` / `text` as already-built plumbing, keeping
  the remaining work focused on built-in apply, commit authority, and blur
  policy.
- Phase 5 now scopes parsing to built-in field specs; app-owned parsing and
  mutation stay app-side behind the current-attempt authority contract.
- Phase 5 now sequences backing binding/display/revision work before parser
  and writeback behavior.
- Phase 5 now extends or replaces `ImeBlurIntent` with semantic blur intent
  and focus-scope data before implementing blur commit/cancel.
- Phase 6 now includes app-owned anchoring/open-session API work before
  app-owned examples, because Phase 4 only provides cursor-position fallback
  anchoring for `ImeTarget::AppOwned`.
- Remaining screen-space editor work now accepts the Phase 4 internal
  `screen_space` panel positioning path; a general follow-anchor abstraction
  is later work unless app-owned anchors require it.
- Phase 6 now strengthens candidate-popup coverage around variable-width text,
  accented/CJK/emoji text, selections, and preedit cursor placement.
- Phase 6 now narrows duplicate field-id work to diagnostics and example
  coverage because duplicate-id non-activation is already implemented.

### Phase 5 — Commit integration (complete)

Connect committed text back to real panel values:

- add a backing binding or built-in apply-sink contract for fields whose values
  are parsed and written by `bevy_diegetic`, including display formatting and
  value-revision or field-epoch data before parser/writeback work,
- parse text by built-in field mode; app-owned parsing stays app-side and
  responds through the current-attempt authority contract,
- keep invalid commits open and populate the Phase 4 validation-feedback
  channel,
- write valid built-in values to backing data,
- add a public commit-authority token or equivalent current-attempt guard for
  app-owned apply responses before app code mutates backing state,
- validate app-owned accept/reject responses without mutating app state again,
- extend or replace `ImeBlurIntent` with semantic pointer/focus-scope data,
  then implement outside-blur commit/cancel policy using
  `ImeCommitCause::Blur` and `ImeCancelCause::Blur`,
- refresh the diegetic panel,
- rely on the Phase 3 session/attempt guards to ignore stale responses.

Acceptance: editing a numeric panel value changes the backing value and the
panel display after commit; invalid input preserves focus, buffer text, and
cursor/selection; stale responses are ignored; cancel leaves the backing value
unchanged.

#### Retrospective

**What worked:**
- `ime/apply.rs` keeps built-in parser/writeback separate from app-owned
  authority and response handling.
- `ImeCommitAuthority` gives app code a current-attempt check before mutation,
  while session accept/reject observers still ignore stale responses.

**What deviated from the plan:**
- Built-in backing is the authored panel tree text descendant plus the next
  `TreeRevision` reported by the panel tree source, not a separate model-binding
  registry.
- Outside blur currently commits when focus leaves the editor/source scope;
  blur cancellation remains covered by explicit cancel and stale-target cleanup.

**Surprises:**
- `LayoutTree::set_field_display_text` could stay internal and still satisfy
  built-in writeback through `DiegeticPanelCommands::set_tree`.

**Implications for remaining phases:**
- Phase 6 examples should show app-owned code checking `ImeCommitAuthority`
  before mutating caller state.
- Later model-binding work can replace the panel-tree text sink without
  changing the session or attempt lifecycle.

### Phase 6 — App-owned sessions and example coverage (complete)

Finish the caller-owned surface and prove the contract:

- app-authored focus, routing, and popup behavior on top of the existing
  `ImeOpenSession` / `ImeTarget::AppOwned` core,
- app-owned anchor/open-session data so app-owned sessions no longer depend on
  the Phase 4 cursor-position fallback,
- synchronous app-surface input disposition hook that runs before built-in
  command mapping for app-owned sessions,
- popup focus-scope behavior,
- platform shortcut matrix coverage for the Phase 3 command router rather than
  a second shortcut implementation,
- duplicate field-id diagnostics and example coverage for the already-built
  "ambiguous fields do not activate" behavior,
- one-frame anchor freshness coverage for the Phase 4 internal `screen_space`
  panel positioning path,
- canonical example coverage plus focused unit coverage for CJK,
  accent/dead-key, multi-byte, emoji, variable-width caret positioning,
  selection/preedit caret placement, invalid numeric, rejection-retry, outside
  blur, activation-capture, and app-popup cases.

Acceptance: the canonical IME example shows world-panel editing plus app-owned
screen-space text entry using the same core session, buffer, IME lease, and
lifecycle events; focused unit tests cover the detailed parser, shortcut,
UTF-8, duplicate-field, preedit, selection, and caret cases.

#### Retrospective

**What worked:**
- `ImeSessionAnchor` removes the app-owned cursor fallback path for callers
  that can provide screen geometry.
- `ImeAppInputDispositionHook` lets app-owned sessions consume surface input
  before built-in command mapping while preserving the shared editor buffer.

**What deviated from the plan:**
- Coverage is split between the canonical `examples/ime.rs` and focused unit
  tests for parser bounds, duplicate field updates, shortcut routing, UTF-8
  boundaries, preedit, selection, and caret placement.
- Duplicate field-id behavior remains diagnostic plus non-activation coverage;
  no separate duplicate-field demo was added to the example.

**Surprises:**
- Example-owned apply logic can stay small because `ImeCommitRequested`
  already carries target, attempt id, field spec, and text.

**Implications for remaining phases:**
- There are no numbered phases left; later work should decide whether built-in
  fields need a model-binding registry beyond panel-tree text writeback.

#### Phase 5 and 6 Review

- Later work now records that there are no remaining numbered phases; any
  unsatisfied items are explicit later-work gaps.
- Later work now names the model-binding decision created by Phase 5's
  panel-tree text sink and `TreeRevision` value reporting.
- Later work now narrows `ScreenPanelFollowAnchor` to general screen-space
  cleanup because Phase 6 added `ImeSessionAnchor` for app-owned geometry.
- R0-R7 now mark the portions satisfied or partially satisfied by Phases 1-6
  instead of remaining as proposed backlog.
- R11 now documents that the Phase 4 internal editor-panel path superseded the
  earlier follow-anchor requirement.
- Later work now records the narrower Phase 6 app-owned popup hook: keyboard
  disposition is implemented; pointer actions, row focus, semantic action
  tokens, and forwarding remain later work.
- Later work now records that external value conflict detection still needs
  original-value snapshots, field epochs, and conflict policy.
- Phase 6 acceptance now states that the canonical example and focused unit
  tests share the acceptance matrix.

## Later work

A later world-space editor can reuse most of the session and parsing model. The
main difference would be rendering caret, selection, and preedit directly on the
panel while still projecting the caret to `Window::ime_position` for the OS IME
candidate popup.

A general `ScreenPanelFollowAnchor` abstraction remains later work. Phase 4
uses an internal screen-space editor panel plus `DiegeticPanel::set_screen_position`;
Phase 6 app-owned sessions use `ImeSessionAnchor`, so this is now a general
screen-space architecture cleanup rather than an app-owned IME blocker.

Built-in field writeback currently updates the authored panel tree text
descendant and reports the panel tree source's next `TreeRevision` as
`ImeValueRevision`. Keep that sink until a caller needs external backing-value
conflict semantics; only then add a model-binding registry with original value
snapshots, revisions, and conflict policy.

App-owned popup routing currently exposes a keyboard-frame
`ImeAppInputDispositionHook` with edit, surface, commit, and cancel decisions.
Pointer-scoped popup actions, result-row focus scope, semantic action tokens,
and post-commit action forwarding remain later work.

External value conflict detection remains later work. Active sessions do not
yet store original text, starting tree revision, computed field epoch, or a
value snapshot, so concurrent external changes are not compared against a
field-level conflict policy.

## Team review follow-ups

The team review produced no premise challenges. Its findings collapse into four
implementation decisions to carry into the plan:

### D1 — Stable field identity and hit testing

Status: accepted

Editable values need a stable `EditableFieldId` or `PanelElementId` supplied by
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
  mod.rs          // private facade, ImePlugin, internal system wiring
  ids.rs          // ImeSessionId, ImeCommitAttemptId
  target.rs       // ImeTarget and target adapters
  events.rs       // lifecycle events and validation responses re-exported at crate root
  requests.rs     // app intent events: open, focus, request commit, cancel
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

If a public anchor type named `Ime` becomes useful, expose it as a curated
crate-root export instead of making `ime` public. `bevy_window::Ime` already
names the raw Bevy OS event stream, while this module owns the higher-level
session API.

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
- `src/panel/mod.rs`: order exported IME system sets relative to `PanelSystems`.
- `src/screen_space/mod.rs`: add a supported follow-anchor path for the transient
  screen-space IME panel.
- `examples/ime.rs`: canonical example covering composed/accented/CJK input,
  selection, platform shortcuts, commit, cancel, and an app-owned popup surface.

## Second team review refinements

### R0 — Namespace and public/private boundary

Status: satisfied by Phases 1, 3, and 4

Public IME names use the crate-root `Ime...` prefix, for example
`ImeSessionId` and `ImeCommitRequested`. Do not add a public `ime` module.
Public API starts with the IME lifecycle events, request events, `ImeTarget`,
session/attempt ids, field specs, and possibly exported IME system sets.

`DiegeticUiPlugin` should install the private `ImePlugin` by default.
`ImePlugin` lives in `ime/mod.rs`; do not create a separate `plugin.rs`.
Exported IME system sets should expose the ordering points external code needs,
such as `AcquireFocusLease`, `PublishInputBlockers`, `ProcessInput`,
`ResolveAnchors`, `UpdateSurface`, `UpdateImePosition`, and `Cleanup`.
`PublishInputBlockers` must run before app camera/input consumers;
`UpdateImePosition` must run after caret geometry is available.

Implementation machinery such as `SingleLineEditBuffer`, transition state,
target adapters, anchor snapshots, follow-anchor components, overlay root
components, and screen-space internals starts as `pub(crate)` until a concrete
external integration needs it.

### R1 — Split core sessions from field adapters

Status: partially satisfied by Phases 1, 4, and 6

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

Status: satisfied by Phase 2

Stable field identity should be authored in the layout tree, not inferred by the
resolver. Add an `EditableFieldId`/`PanelElementId` newtype and an authoring path
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

Status: satisfied by Phases 1, 3, and 5

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

Status: satisfied by Phases 2 and 3

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

Status: partially satisfied by Phases 3, 5, and 6

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
| `PendingValidation` | built-in accept | mutate through the built-in apply sink, emit `Applied`, clean up |
| `PendingValidation` | app-owned `AcceptCommit` | validate ids/token, emit `Applied`, clean up without mutating app state |
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

Status: partially satisfied by Phases 4 and 6

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
render-command bounds. The first implementation accepts a documented one-frame
projection/caret freshness policy unless a stricter same-frame path proves cheap
during implementation.

### R7 — Example acceptance matrix

Status: split between `examples/ime.rs` and focused unit tests

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

The third team review produced no premise challenges. The two review cycles
converged on the following implementation constraints; none require a separate
user decision before implementation.

### R8 — Make IME lease and input blocking authoritative

Status: accepted

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

Status: accepted

Severity: important

Source dimension: architecture

Class: design-improvement

Expose only durable integration points as curated crate-root IME system sets,
such as publishing input blockers and applying the final IME candidate-popup
position. Keep lifecycle sequencing, input translation, anchor resolution,
surface update, and cleanup in crate-private internal system sets so the module
can evolve without freezing every internal step as public API.

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

Status: accepted

Severity: important

Source dimension: architecture and type system

Class: design-improvement

Field identity and hit resolution should not depend on render commands or raw
layout indices. Add a panel-layer `PanelFieldRecord` produced from layout
metadata and stored on `ComputedDiegeticPanel` as crate-internal data. Include
stable `PanelElementId`, role/spec, bounds, effective clip, draw order, style
snapshot, panel-local geometry provenance, tree revision, computed field epoch,
and the crate-private source element locator.

Use distinct newtypes for semantic field identity and implementation locators,
for example `PanelElementId`, `LayoutElementIndex`, `RenderCommandIndex`, and
`ComputedFieldIndex`. Public targets carry only semantic `PanelElementId`;
locators stay internal. Field-id uniqueness validation should return a typed
error keyed by `PanelElementId`.

`PanelElementId` is panel-local. Session identity should use an internal
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

Status: superseded by the Phase 4 internal editor-panel path; later-work only

Severity: important

Source dimension: architecture and risk

Class: design-improvement

The accepted implementation uses a session-owned screen-space editor panel and
mutates it through `DiegeticPanel::set_screen_position`. A generic
crate-private follow-anchor path in `screen_space`, such as
`ScreenPanelFollowAnchor`, is no longer required for app-owned IME anchoring;
revisit it only as a general screen-space ownership cleanup. If added later,
the IME anchor system should publish an anchor snapshot and `screen_space`
should report the final rect that caret layout and IME positioning use.

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

Status: accepted

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

Status: accepted

Severity: important

Source dimension: user impact and type system

Class: design-improvement

The editable-field spec should be a closed enum split by ownership mode, for
example `EditableFieldSpec::BuiltIn(BuiltInFieldSpec)` and
`EditableFieldSpec::AppOwned(AppOwnedFieldSpec)`. Built-in specs pair
`PanelElementId` with an explicit value kind, typed range constraints,
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

Status: accepted

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

Status: accepted

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

Status: accepted

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

Status: accepted

Severity: important

Source dimension: user impact

Class: design-improvement

App-owned screen-space surfaces need a concrete synchronous hook for deciding
whether a key or pointer event is a text-editing command or an app-surface
action. Name the public disposition enum before implementation, for example
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
