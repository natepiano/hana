# Space-invoked button press

> **Status: DESIGN — not yet phased.** Compile with `/plan:to_phased_plan` before
> dispatching.

Give a focused button the same press lifecycle from the keyboard that it already
has from the pointer, without taking `Space` away from the application when no
widget is focused.

## Problem

Two defects, one of which is a prerequisite for the other.

### P1 — keyboard activation has no press phase

Holding `Space` on a focused button produces no visual change. The button's
authored `pressed` appearance is only reachable through the pointer.

Keyboard activation fires the completed click immediately:

```rust
// widgets/button.rs:706-712
SemanticWidgetIntent::Activate { .. } => {
    commands.trigger(ButtonClicked {
        entity,
        id: widget.id().clone(),
        pointer_id: None,
    });
},
```

The `ButtonPress` marker that `present_button_state` reads (`button.rs:149`,
`:165`) is inserted only by the two pointer paths, `button.rs:443` and `:630` —
both of which take a `PointerId`. Nothing on the keyboard path ever inserts it.

The cause is upstream: only the **start** edge of each action is observed
(`input.rs:72-77`, six `record_action_start::<…>` observers), and the message it
produces carries no phase:

```rust
// widgets/input.rs:627
WidgetInputAction::Activate => WidgetInput::Activate { window },
```

`WidgetInput` (`input.rs:639-671`) has one `Activate` variant. There is no
release edge anywhere in the chain, so there is nothing a press phase could hang
off.

### P2 — `Space` is consumed whether or not a widget is focused

Widget input actions are installed with input consumption on:

```rust
// widgets/input.rs:534-541
let keybindings = Keybindings::new::<WidgetShiftModifierAction>(
    spawner,
    ActionSettings {
        require_reset: true,
        consume_input: true,
        ..default()
    },
);
```

The input context is **window-scoped**, and it is activated purely by window
focus:

```rust
// widgets/input.rs:607-613
for (window, activity) in contexts.iter() {
    let should_be_active = focused_window == Some(window);
    ...
}
```

Nothing in that condition asks whether a widget is focused. Consumption happens
at the enhanced-input layer, ahead of `route_semantic_input` (`input.rs:703`),
which is where the "is anything actually focused?" question is first asked. So a
`Space` press is taken from the application even when the routing that follows
does nothing with it.

This is why the design cannot ship P1 alone. An application that binds `Space` to
a transport control and also draws a Hana panel currently loses `Space` for as
long as its window is focused. Adding a hold-to-press phase makes that worse: the
key would be held down by the widget layer for the whole duration of the press.

The existing escape hatch is all-or-nothing per window — `WidgetInputMode::
Bindings` (`input.rs:90`) can drop `Space` from `activate`, which also removes
keyboard activation from every widget in that window.

## Requirements

1. Holding the activate binding on a focused button shows the button's authored
   `pressed` appearance for as long as it is held.
2. Releasing it over the still-focused button fires `ButtonClicked` exactly once,
   with `pointer_id: None`, as it does today.
3. `Escape`, focus moving away, and the button becoming disabled each cancel the
   press without firing a click.
4. A widget press and a pointer press on the same button do not both run. One
   arbitration rule, not two independent lifecycles.
5. **When no widget is focused, the activate binding is not consumed** and the
   application's own binding for that key runs normally.
6. When a widget *is* focused, the application does not also see the key.
   Widget-first is the correct precedence; the point is that it must be scoped to
   an actual focus, not to window focus.
7. Sliders and editable fields keep their current activate behavior. This changes
   buttons only.

Requirement 5 is the one that resolves the transport conflict, and it is
independently useful — it fixes a real defect that exists today.

## Design

### Phase edges on `WidgetInput`

Add the release edge. `WidgetInput::Activate` gains a phase, or splits into two
variants — the choice is an open question below. Either way:

- `input.rs` observes the completion edge alongside `record_action_start`, and
  `PendingWidgetInputActions` records which edge each entry is.
- `emit_widget_input` (`input.rs:617-631`) maps them onto the new shape.
- `route_semantic_input` forwards both edges to the focused widget as
  `SemanticWidgetIntent`.

Only `Activate` needs edges. `Next`, `Previous`, `First`, `Last`, and `Cancel`
stay one-shot.

### Press lifecycle without a pointer id

`handle_semantic_intent` (`button.rs:692`) inserts `ButtonPress` on the press
edge and fires `ButtonClicked` on the release edge, mirroring what the pointer
path does across `begin_press` / `finish_press`.

The obstacle is capture. The pointer path reserves the widget before it presses:

```rust
// widgets/button.rs:620-625
if !world
    .resource_mut::<WidgetCaptures>()
    .try_capture(pointer_id, entity, sequence)
{
    return;
}
```

`WidgetCaptures` is keyed by `PointerId`, and a keyboard press has none. This is
what makes requirement 4 a design decision rather than a detail: either the
capture key becomes an enum with a keyboard variant, or the button tracks a
keyboard press outside `WidgetCaptures` and the two lifecycles arbitrate
explicitly. The first keeps one arbitration point; the second avoids widening a
type that the pointer code paths all read.

### Focus-scoped consumption

Consumption must become conditional on a widget being focused in that window.
Candidate mechanisms, cheapest first:

- **Context activity already carries the signal.** `synchronize_context_activity`
  (`input.rs:595`) decides activity from window focus alone. Extending its
  condition to "window focused **and** this window's panel has a focused widget"
  deactivates the whole widget context when nothing is focused, which releases
  every binding including `Space`.
  - This also releases `Tab`, which is wrong: `Tab` must still be able to *enter*
    the widget set from nothing. So the split is not clean at the context level.
- **Per-action contexts.** Traversal actions (`Tab`, `Shift+Tab`, and the
  first/last bindings) live in an always-active context; `Activate` and `Cancel`
  live in a second context activated only while a widget is focused. This gives
  the right behavior for both key groups with no per-frame condition, at the cost
  of a second context type and its registration.
- **Conditional consumption per action.** Keep one context and make consumption
  a per-action decision evaluated against current focus. Whether
  `bevy_enhanced_input` supports revoking consumption after the fact needs
  checking; if it does not, this option is out.

The per-action-context split looks right, but the enhanced-input capabilities
need verifying before committing.

## Open questions

1. **Message shape.** `Activate { window, phase: ActivatePhase }` versus
   `ActivatePressed { window }` / `ActivateReleased { window }`. The enum-variant
   split reads better at the match sites in `route_semantic_input`; the phase
   field avoids duplicating the routing arm. `SemanticWidgetIntent` must make the
   same choice.
2. **Capture key.** Widen `WidgetCaptures`' key to cover a keyboard press, or
   arbitrate between two separate lifecycles in `button.rs`.
3. **Consumption mechanism.** Confirm what `bevy_enhanced_input` supports before
   choosing between the second and third options above.
4. **Repeat suppression.** Key auto-repeat must not fire repeated presses. Verify
   whether `require_reset: true` (`input.rs:537`) already covers this for a held
   key or whether the press edge needs its own guard.
5. **Does `Enter` follow `Space`?** `Enter` is bound to the same `activate`
   action. Giving both a hold phase is consistent; it also means `Enter` stops
   being available to the application under the same focus condition. Probably
   correct, but it should be a stated decision rather than a side effect.

## Phase outline

Each phase leaves the tree green on its own.

1. **Focus-scoped consumption.** Requirement 5 only. Split the input context (or
   whatever open question 3 resolves to) so activate and cancel bindings are held
   only while a widget is focused, and traversal bindings stay always-on. Ships
   the transport fix with no change to activation semantics.
2. **Release edge through the input chain.** Add the completion-edge observer,
   the phase-carrying message, and the routing arm. `handle_semantic_intent`
   still fires the click on the press edge, so behavior is unchanged — this phase
   is the plumbing only.
3. **Button press lifecycle.** Resolve the capture question, insert `ButtonPress`
   on press, fire `ButtonClicked` on release, and cancel on `Escape`, focus loss,
   and disable. This is the phase that makes the authored `pressed` appearance
   reachable from the keyboard.
4. **Example and live smoke.** `examples/widgets.rs` help text updates to say the
   press is held; the keyboard smoke covers hold, release-click, and each cancel
   path.

## Verification

- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`

Phase 1 needs a regression test proving an application-level binding on `Space`
still runs while a Hana panel is drawn and no widget is focused. That test is the
whole point of the phase and must not be reduced to an assertion about context
activity.
