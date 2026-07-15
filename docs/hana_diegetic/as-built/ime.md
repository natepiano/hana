# Single-line IME

Single-line text editing for diegetic panel values. Double-click an editable
value on a panel, edit it in a transient screen-space editor that follows the
projected field, and commit the parsed value back to the panel on Enter or blur.
The same core also drives caller-owned screen-space text entry (command
palettes, etc.).

The IME is fundamentally window/screen-space: even when the edited value is
drawn on a 3D panel, the OS candidate popup is positioned in window coordinates
through `Window::ime_enabled` / `Window::ime_position`. So the design splits into
a stored value (owned by the panel tree or app model), a transient screen-space
edit session, an anchor projected from the field to window coordinates, and a
parse-on-commit step.

Example: `crates/hana_diegetic/examples/ime.rs` (world-panel numeric editing plus
an app-owned popup).

## Module layout

`crates/hana_diegetic/src/ime/` is a private module; the approved public API is
re-exported from the crate root with `Ime...` names (plus `PanelElementId`).
There is no public `ime` module and no `plugin.rs`; `ImePlugin` lives in
`ime/mod.rs` and is installed by `DiegeticUiPlugin` (`src/lib.rs`).

| File | Owns |
| --- | --- |
| `mod.rs` | `ImePlugin`, `ImeSystemSet`, observer/system wiring, re-exports |
| `ids.rs` | `ImeSessionId`, `ImeCommitAttemptId`, `ImeValueRevision`, `PanelElementId` / `AutoElementId` |
| `target.rs` | `ImeTarget`, `ImeSessionAnchor` |
| `field.rs` | `ImeEditableFieldSpec` and the built-in/app-owned spec + applied-value types, `ImePanelField` |
| `events.rs` | Public lifecycle events (`ImeStarted`, `ImeTextChanged`, `ImeCommitRequested`, `ImeApplied`, `ImeValidationRejected`, `ImeCanceled`) and response events (`ImeAcceptCommit`, `ImeRejectCommit`) |
| `session.rs` | `ActiveImeSession` (single-active resource), request events (`ImeOpenSession`, `ImeRequestCommit`, `ImeRequestCancel`), `ImeCommitAuthority`, lifecycle observers |
| `buffer.rs` | `ImeEditBuffer` (UTF-8-safe single-line editing) and the public snapshot types |
| `input.rs` | Bevy `Ime` + `KeyboardInput` routing, `ImeAppInputDispositionHook` |
| `lease.rs` | `ImeInputBlocker` (window-scoped focus lease) |
| `activation.rs` | Panel double-click picking → `ImeOpenSession` |
| `editor.rs` | Transient screen-space editor panel, anchoring, caret geometry, `Window::ime_position` |
| `apply.rs` | Built-in commit validation and panel-tree writeback |

Panel-boundary field records live outside the module in
`src/panel/field.rs` (`PanelFieldRecord`) and are stored on
`ComputedDiegeticPanel`. Authoring lives in `src/layout/builder.rs`
(`El::editable_field`).

## Authoring an editable field

`El::editable_field(field_id, field_spec)` marks a layout element editable. It
stores an `ImePanelField { field_id, field_spec }` on the element.

```rust
El::new().editable_field(
    "gain",
    ImeEditableFieldSpec::BuiltIn(ImeBuiltInFieldSpec::new(
        ImeBuiltInFieldKind::Float { min: Some(-96.0), max: Some(12.0) },
    )),
)
```

- `field_id: impl Into<PanelElementId>` — panel-local semantic identity used for
  hit testing, anchoring, and commit routing. A `&str`/`String` becomes
  `PanelElementId::Named`.
- `field_spec` is `ImeEditableFieldSpec::BuiltIn(ImeBuiltInFieldSpec)` (parsed and
  written back by `hana_diegetic`) or `AppOwned(ImeAppOwnedFieldSpec { key })`
  (the app parses and applies).
- `ImeBuiltInFieldKind` is `Text`, `Float { min, max }`, or `Integer { min, max }`
  with optional inclusive bounds.

### `PanelElementId`

```rust
pub enum PanelElementId {
    Named(String),        // author-assigned, publicly addressable
    Auto(AutoElementId),  // builder-minted positional id for an unnamed text run
}
```

One panel-local namespace shared by element/text/field ids. Every public
constructor (`named`, `From<&str>`, `From<String>`) yields `Named`; only the
layout builder's per-build counter mints `Auto` (via `PanelElementId::auto`,
crate-internal). A string can never forge an `Auto`, so the two families cannot
collide. Editable fields always carry a `Named` id because `editable_field`
requires an author-supplied `field_id`.

## Field records at the panel boundary

`collect_panel_field_records` (`src/panel/field.rs`) walks the laid-out tree and
produces one `PanelFieldRecord` per editable element:

```rust
pub struct PanelFieldRecord {
    pub field_id:      PanelElementId,
    pub bounds:        BoundingBox,        // panel-local layout points
    pub field_spec:    ImeEditableFieldSpec,
    pub display_text:  String,             // text displayed when computed
    pub element_index: usize,              // source element in the LayoutTree
    pub duplicate_id:  bool,
}
```

Records live on `ComputedDiegeticPanel`, reached through
`field_records()`, `field_at_local_position(panel_local)`, and
`field_id_conflicts()`. Identity and hit resolution never read render commands
or raw layout indices; the semantic id is authored, the `element_index` locator
stays internal.

Duplicate ids are not a hard layout error. A duplicated id marks every matching
record `duplicate_id = true` and appears in `field_id_conflicts()`; hit
resolution and commit skip duplicated records (`field_record` filters
`!record.duplicate_id`), so an ambiguous field simply does not activate.

## Session lifecycle

`ActiveImeSession` holds at most one session. Its private `ImeSessionState` is
`Editing`, `Composing(ImePreedit)`, or `PendingCommit(ImeCommitAttemptId)`. All
transitions run through observers registered in `ImePlugin`:

- `open_session` (`ImeOpenSession`) — cancels any existing session with
  `Replaced`, mints a new `ImeSessionId`, seeds the buffer from `initial_text`,
  acquires the input-blocker lease (recording the activation frame), and emits
  `ImeStarted` plus an initial `ImeTextChanged`.
- `request_commit` (`ImeRequestCommit`) — only from `Editing`; mints an
  `ImeCommitAttemptId`, moves to `PendingCommit`, sets the commit authority, and
  emits `ImeCommitRequested { attempt_id, field_spec, text, cause }`.
- `request_cancel` (`ImeRequestCancel`) — emits `ImeCanceled` and clears the
  lease + authority.
- `accept_commit` (`ImeAcceptCommit`) — matches `session_id` + pending
  `attempt_id`, clears lease + authority, emits `ImeApplied`.
- `reject_commit` (`ImeRejectCommit`) — matches ids, returns state to `Editing`
  (session stays alive), emits `ImeValidationRejected`.
- `cleanup_stale_sessions` (system, `ImeSystemSet::Cleanup`) — terminal cancel
  for lease loss (`LeaseLost`), window closed/despawned (`WindowClosed`), OS
  focus loss (`FocusLost`), or target entity gone (`TargetStale`).

`ImeCommitAttemptId` distinguishes attempts within a session so a delayed
accept/reject cannot resolve a newer attempt; stale responses are dropped by the
id match in `accept_commit`/`reject_commit`.

### Semantic target

```rust
pub enum ImeTarget {
    WorldPanelField  { panel: Entity, field_id: PanelElementId },
    ScreenPanelField { panel: Entity, field_id: PanelElementId },
    AppOwned         { owner: Entity, field_id: PanelElementId },
}
```

`ImeTarget` is semantic identity only — the backing thing being edited, not the
rendered surface or anchor. Systems resolve anchors, focus scope, and apply sink
from the target through helper functions rather than branching everywhere.

## Activation (picking)

`activation::observe_panel_clicks` attaches an `open_from_panel_click` observer
to every `DiegeticPanel` on `Add`. On a primary-button double-click
(`click.count >= 2`) it:

1. Transforms the hit world point into panel-local layout coordinates
   (`panel_local_from_hit`).
2. Resolves the field via `computed.field_at_local_position`.
3. Calls `click.propagate(false)` so the activating click does not also drive the
   camera or another panel action (activation-frame capture; the lease also
   records the frame via `captured_activation_frame`).
4. Stores a `PendingImePanelAnchor` (panel, field id, picked camera, window) so
   the editor projects through the same camera that produced the click.
5. Triggers `ImeOpenSession` with `WorldPanelField` or `ScreenPanelField`
   depending on `panel.coordinate_space()`, seeding `initial_text` from the
   record's `display_text`.

App-owned sessions skip picking: the app triggers `ImeOpenSession` directly with
`ImeTarget::AppOwned`, its own `field_spec`, and an optional `ImeSessionAnchor`
(`ScreenRect`/`ScreenPoint` in logical pixels) for editor placement.

## Commit flow

`ImeCommitRequested` carries everything the sink needs (`target`, `attempt_id`,
`field_spec`, `text`, `cause`) so neither built-in nor app code has to re-read
layout metadata.

**Built-in** (`apply::apply_builtin_commit`): guards on
`ImeCommitAuthority::is_current`, parses per `ImeBuiltInFieldKind` (trims, checks
finiteness for floats, applies bounds), clones the panel's `LayoutTree`, writes
the formatted text with `LayoutTree::set_field_display_text`
(`crate::layout::FieldDisplayTextUpdate`), pushes it via
`DiegeticPanelCommands::set_tree` (preserving layout change classification), and
triggers `ImeAcceptCommit` with `ImeBuiltInApplied { value, display_text,
value_revision }`. `value_revision` is the panel's `next_tree_revision()`.
Parse/range/field failures trigger `ImeRejectCommit` instead.

**App-owned**: the app observes `ImeCommitRequested`, filters
`ImeTarget::AppOwned`, calls `ImeCommitAuthority::is_current(session_id,
attempt_id)` immediately before mutating its model, then triggers `ImeAcceptCommit`
(with opaque `display_text`/`value_revision`) or `ImeRejectCommit`. `AcceptCommit`
means the app has *already* applied the change; the IME core validates ids and
emits `ImeApplied` without touching app state again. It carries no universal
output value.

`ImeCommitAuthority` (resource) plus `ImeCommitAuthorityToken` exist because the
session/attempt id match in `accept_commit` only ignores stale responses *after*
they arrive — the authority check lets app apply code refuse to mutate for a
stale attempt *before* the mutation happens.

## Input routing (`input.rs`)

System order inside `ImeSystemSet` (all in `Update`, chained):
`PublishInputBlockers` → `UpdateWindowIme` → `Input` → `UpdateEditorGeometry` →
`UpdateImePosition` → `Cleanup`.

- `update_window_ime` sets `Window::ime_enabled = true` for the active session's
  window and `false` for a window it just released.
- `handle_window_ime` routes `bevy::window::Ime`: `Preedit` → composing state
  (`ImeTextChanged` with `ImePreedit`), `Commit` → insert into the committed
  buffer and leave `Composing`, `Enabled`/`Disabled` → clear preedit.
- `handle_keyboard` (runs only while the session owns the lease):
  - not leased → drains and drops keyboard events;
  - composing → only Escape (clears preedit); all other keys belong to the IME
    layer and are consumed;
  - for `AppOwned` targets, runs `ImeAppInputDispositionHook` first; a `Surface`
    decision consumes the frame, `Commit`/`Cancel` trigger the matching request,
    `Edit` falls through to built-in editing;
  - Enter/NumpadEnter → `ImeRequestCommit(Enter)`, Escape → `ImeRequestCancel(Escape)`;
  - movement/selection/delete/select-all map to `ImeEditCommand`
    (`command_from_key_code`);
  - otherwise, printable text is inserted from `KeyboardInput::text`, but only
    when `ImeInputFrame::saw_platform_ime` is false. Bevy 0.19 delivers composed
    text through *both* `Ime` messages and `KeyboardInput::text`; this per-frame
    guard prevents double insertion. Do not insert text characters from raw
    keycodes — go through IME commits or logical-key text so dead keys and
    composed characters work.

### Shortcut mapping

`command_from_key_code` covers a full single-line command set across platforms:
character/word/line movement (word = Ctrl or Alt, line = Super/Cmd or Home/End),
shift-extend selection, select-all (Ctrl/Cmd+A), and backward/forward delete by
character/word/line. Word delete uses Ctrl or Alt; line delete uses Super/Cmd.

## Edit buffer (`buffer.rs`)

`ImeEditBuffer` is a single-line, control-character-stripping buffer with a
directed `anchor`/`focus` selection. Byte offsets are wrapped in
`ImeBufferBoundary` / `ImePreeditBoundary` whose constructors are private to the
module; external code reads offsets via `as_usize` but cannot build them.
Movement and deletion operate on char/word boundaries computed inside the buffer,
so operations never split UTF-8.

Public snapshot types (`ImeBufferSnapshot`, `ImeCursorState`,
`ImeSelectionSnapshot`, `ImeBufferRange`, `ImePreedit`) keep committed text and
preedit text separate. `ImeTextChanged.snapshot` is what the editor and app-owned
consumers render from; it fires for committed *and* preedit-only changes and is
**not** a field commit.

## Transient editor (`editor.rs`)

The editor is a session-owned screen-space `DiegeticPanel` (marker
`ImeEditorPanel`, `camera_order = 120`), spawned on the first `ImeTextChanged`
for a session and despawned on `ImeApplied`/`ImeCanceled`. It is not part of the
target panel's layout tree; the backing value stays the source of truth and is
only written on commit.

- `update_editor_from_text_changed` rebuilds the editor tree from the buffer
  snapshot (committed text, caret, selection highlight, preedit text, optional
  validation text) via `set_tree`.
- `update_editor_anchor` (`UpdateEditorGeometry`, ordered
  `after(PanelSystems::ResolvePanelAttachments)` and
  `before(PanelSystems::PositionScreenSpace)`) re-projects the target field each
  frame: world panels through the captured camera
  (`Camera::world_to_viewport`), screen panels through resolved
  `PanelScreenBounds`, app-owned through `ImeSessionAnchor` (cursor-position
  fallback if none). It sizes and positions the editor panel via
  `DiegeticPanel::set_size` / `set_screen_position`, clamps to the viewport, and
  computes the caret position from measured text. Invalid projection (off-screen,
  behind camera, degenerate rect) triggers `ImeRequestCancel(TargetStale)`.
- `update_window_ime_position` (`UpdateImePosition`) writes
  `Window::ime_position` from the editor's caret rect, so the OS candidate popup
  sits at the caret rather than the field bounds.

Anchor/caret freshness is a bounded one-frame policy: geometry is computed from
last frame's snapshot, so the caret and candidate-popup position can lag by one
frame but never persist stale beyond it.

### Blur

`classify_panel_click` observes clicks on any panel while an editor is active. A
click on the editor panel is consumed. A click elsewhere records an
`ImeBlurIntent` and consumes the click. `handle_blur_intent` (in `Input`) checks
the intent against the active session's focus scope (source panel or app-owned
owner) and, if outside, triggers `ImeRequestCommit(Blur)`. Pending commits
suppress blur handling.

## Focus lease (`lease.rs`)

`ImeInputBlocker` is the single window-scoped lease and the one authority for
whether input on a window is IME-owned. It records `session_id`, `window`, and
the activation frame. Public reads: `window()`, `session_id()`,
`blocks_window(window)`, `captured_activation_frame(frame)`. The crate publishes
this blocker in `ImeSystemSet::PublishInputBlockers`, before app camera/input
consumers read raw input; apps using a camera controller (Lagrange, etc.) bridge
`ImeInputBlocker` into their own camera-suppression path rather than making IME
depend on the camera system. Lease loss is a terminal cancel cause.

## Invariants and gotchas

- One active session, one lease writer, one window `ime_enabled`/`ime_position`
  writer.
- The activating double-click must not drive the camera/scene: activation calls
  `propagate(false)` and the lease captures the activation frame. Bridge the
  blocker early enough (before camera routing) for the same-frame guarantee.
- Preedit composition owns Enter/Escape/arrows; field commit/cancel/navigation
  must not fire while composing.
- Text characters come from IME commits or `KeyboardInput::text`, never raw
  keycodes; the `saw_platform_ime` guard dedupes platforms that deliver both.
- Built-in writeback goes through the authored panel-tree text descendant and
  `set_tree`; there is no external model-binding registry. `value_revision` is
  the panel's next `TreeRevision`, not an app revision.
- Rejection keeps the session alive, focus, and buffer/selection; a retry mints a
  fresh `ImeCommitAttemptId`. A successful retry emits exactly one `ImeApplied`.
- App apply sinks must call `ImeCommitAuthority::is_current` *before* mutating —
  the session id match only discards stale responses after the fact.
- Duplicate field ids do not fail layout; they silently disable activation and
  commit for the ambiguous id (`duplicate_id` / `field_id_conflicts`).

## Not implemented

Multiline editing, mouse text selection, clipboard, rich text, full world-space
caret/preedit rendering, multiple simultaneous editors, software-keyboard support
beyond desktop IME. App-owned popup routing exposes only a keyboard-frame
disposition hook (`ImeAppInputDispositionHook`); pointer-scoped popup actions,
result-row focus, and semantic action-token forwarding are not built. Active
sessions do not snapshot the original value or field epoch, so concurrent
external value changes are not compared against a conflict policy — built-in
writeback wins.
