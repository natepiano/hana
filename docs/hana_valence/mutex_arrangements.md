# Mutually Exclusive Arrangement Components

## Question

`Strip`, `Accordion`, and `Coil` are separate first-class ECS components. Bevy
0.19 permits an entity to carry any combination of them, while the arrangement
API requires exactly one. This note records how `../hana` maintains comparable
component invariants and proposes the same short-term mechanism for
`hana_valence`.

This is a Bevy 0.19 design. The separate investigation of Bevy 0.20's native
mutually exclusive component support determines whether this mechanism should
eventually be replaced.

## Findings from `../hana`

Hana uses a marker-component state machine for `Idle`, `Hovered`, and
`Selected`. Each component has an `On<Add, T>` observer that removes every
other state component from the entity:

```rust
fn on_selected_added(added: On<Add, Selected>, mut commands: Commands) {
    commands.entity(added.entity).remove::<Idle>();
    commands.entity(added.entity).remove::<Hovered>();
}
```

The owning `StatePlugin` registers all three observers. Inserting one state at
a time therefore makes the incoming state win after deferred commands are
applied. Hana uses the same two-observer pattern for `Icon` versus `Gizmo` and
for `Icon` versus `Expanded`.

The relevant live sources are:

- `../hana/crates/hana/src/movable/state.rs:1-39` explains the marker-based
  state machine and lifecycle-observer invariant.
- `../hana/crates/hana/src/movable/state.rs:56-73` defines the three distinct
  state components.
- `../hana/crates/hana/src/movable/state.rs:91-109` registers the lifecycle
  observers in `StatePlugin`.
- `../hana/crates/hana/src/movable/state.rs:121-145` implements the three
  `On<Add, T>` observers and sibling removal.
- `../hana/crates/hana/src/movable/state.rs:246-262` documents a real deferred
  command ordering hazard between removal and insertion observers. This is a
  reminder that other lifecycle observers can observe and affect the
  transition.
- `../hana/crates/hana/src/selection/dimension_lock/affordance.rs:219-227` and
  `../hana/crates/hana/src/selection/dimension_lock/mod.rs:87-96` implement and
  register the `Icon`/`Gizmo` pair.
- `../hana/crates/hana/src/selection/rotate/affordance.rs:156-164` and
  `../hana/crates/hana/src/selection/rotate/mod.rs:31-53` implement and register
  the `Icon`/`Expanded` pair.
- `../hana/Cargo.toml:47-58` selects Bevy 0.19, and `../hana/Cargo.lock` resolves
  both `bevy` and `bevy_ecs` to 0.19.0.

The local Bevy 0.19.0 source confirms the lifecycle behavior this pattern
depends on:

- `bevy_ecs/src/lifecycle.rs:14-35` defines `Add` as a presence change and
  `Insert` as every insertion, including replacement.
- `bevy_ecs/src/bundle/insert.rs:416-455` triggers `Add` observers after the
  inserted components are present, then triggers `Insert` observers.
- `bevy_ecs/src/event/trigger.rs:334-386` states that components inserted in
  the same bundle share one lifecycle trigger and are all visible in the new
  archetype.
- `bevy_ecs/src/observer/distributed_storage.rs:178-192` states that component
  hooks run before observers on addition and after observers on removal.
- `bevy_ecs/src/observer/mod.rs:590-625` tests recursive observer commands and
  shows their deterministic deferred execution.

## Recommended short-term design

Add three crate-owned lifecycle observers in `arrange.rs`:

```rust
fn on_strip_added(added: On<Add, Strip>, mut commands: Commands) {
    commands
        .entity(added.entity)
        .try_remove::<(Accordion, Coil)>();
}

fn on_accordion_added(added: On<Add, Accordion>, mut commands: Commands) {
    commands.entity(added.entity).try_remove::<(Strip, Coil)>();
}

fn on_coil_added(added: On<Add, Coil>, mut commands: Commands) {
    commands
        .entity(added.entity)
        .try_remove::<(Strip, Accordion)>();
}
```

`try_remove` has the same component behavior as Hana's `remove`, but suppresses
a warning if another observer despawns the entity before the queued removal is
applied. Removing a component the entity does not carry is already valid.

Use `Add`, not `Insert`, for parity with Hana and because replacing the data of
the already-active `Accordion` or `Coil` is not a state transition. Normal
mutable queries do not emit either lifecycle event.

Provide an `ArrangementPlugin` that registers:

- the three new exclusivity observers;
- the existing `on_member_added` and `on_member_removed` lifecycle observers.

Consumers would add `ArrangementPlugin` once, while continuing to register the
generic `apply_member_placements::<R>` and `drive_arrangement_hinges::<R>`
systems for each tiling rule they use. This keeps component invariants and
member bookkeeping crate-owned without claiming ownership of consumer-specific
rule scheduling.

Do not put the arrangement observers in `FoldPlugin`. Arrangements are usable
without authored fold playback, and requiring `FoldPlugin` would couple the
base arrangement invariant to an optional higher layer. Existing consumers
that manually register the member observers should remove those registrations
when adopting `ArrangementPlugin`; registering them twice is unnecessary and
makes lifecycle ordering harder to reason about.

## Semantics and limits

The observer set enforces **at most one** arrangement component. It does not
enforce **exactly one**:

- An entity may still carry none of the three components.
- Adding one component to an entity that already carries a different one makes
  the newly added component win after deferred commands are flushed.
- Replacing the value of the already-present component emits `Insert`, not
  `Add`; it leaves the active arrangement type unchanged.
- An entity containing an invalid combination before `ArrangementPlugin` is
  installed is not repaired retroactively.

An entity with no arrangement component is not an arrangement root. Continue
to treat that state as invalid authoring rather than silently inserting
`Strip`. If exactly-one construction becomes important, add explicit spawn or
transition helpers in addition to the observers; do not make a required
component choose a hidden default.

### One-at-a-time transitions only

Do not spawn or insert `(Strip, Accordion)`, `(Accordion, Coil)`, or all three in
one bundle. Bevy triggers all matching `Add` observers for the same bundle after
all newly added components are present. Each observer then queues removal of
the others, so a multi-arrangement insertion can remove every arrangement
component rather than select a winner.

Callers should transition by inserting only the desired new component. If an
API must accept arbitrary or reflected input, first remove all three, flush,
then insert one selected component, or expose one helper per destination type.
The components remain first-class; a transition helper does not need to
reintroduce a public enum component.

### Ordering and recursion

Observer-issued `Commands` are deferred. During the `On<Add, T>` callback, the
new component and the old component can both be present. The invariant holds
after the observer command queue is applied, not at every instruction inside
the lifecycle callback.

Removing siblings does not recurse into these three observers because they
listen only for `Add`. Ordinary `Remove` observers on the displaced component
will run and can see the incoming component. A downstream removal observer that
reinserts the displaced component could create a ping-pong transition; that
behavior must be prohibited for these arrangement types.

Plugin registration must occur before arrangements are spawned or loaded.
Observers added later do not receive lifecycle events for components already
present.

### Defensive runtime behavior remains necessary

Keep the exact-one match in `arrangement_angle` and the `Or` filters in member
bookkeeping. They protect against missing plugin registration, invalid bundle
insertion, pre-existing invalid entities, and the transient interval before
deferred removals apply. The hinge driver should continue to skip an ambiguous
root rather than choose an arrangement by query order.

The current relevant `hana_valence` sources are:

- `crates/hana_valence/src/arrange.rs:58-126` defines `Accordion`, `Coil`, and
  `Strip` as separate components.
- `crates/hana_valence/src/arrange.rs:279-348` contains the existing member
  lifecycle observers and fallback index assignment.
- `crates/hana_valence/src/arrange.rs:350-432` queries all three arrangement
  types for placement and hinge driving.
- `crates/hana_valence/src/arrange.rs:513-527` accepts only an exact-one match
  and otherwise returns `None`.
- `crates/hana_valence/src/fold/mod.rs:41-83` shows the existing plugin boundary
  for the higher-level fold feature.
- `crates/hana_valence/README.md:31-95` currently makes consumers register the
  member observers and arrangement systems directly.
- `Cargo.toml:14-25` and `Cargo.lock` resolve this workspace to Bevy 0.19.0.

## Tests to add with the implementation

1. Add each of `Strip`, `Accordion`, and `Coil` by itself and assert it remains
   present after command flushing.
2. Cover all six directed transitions between different arrangement types.
   Assert the incoming component and its data remain and both siblings are
   absent.
3. Replace an existing `Accordion` and `Coil` value and assert the replacement
   data remains without producing a type transition.
4. Insert transitions through both `Commands` plus `app.update()` and direct
   `World` entity mutation plus an explicit flush, so both supported mutation
   paths exercise lifecycle observers.
5. Co-insert two types and all three types. Lock in the documented invalid-input
   behavior, or assert a diagnostic if the implementation adds one; never
   silently depend on bundle/component registration order to select a winner.
6. Spawn a root without any arrangement component and confirm it is not treated
   as a valid arrangement.
7. Verify `arrangement_angle`/member placement still rejects an ambiguous root
   when the plugin is absent. This preserves the defensive lower-level API.
8. Add `ArrangementPlugin` to existing arrangement integration tests and remove
   their manual member-observer registration, proving the plugin owns lifecycle
   wiring without owning generic rule systems.
9. If `try_remove` is used, add a competing observer that despawns the entity
   and verify the queued cleanup does not emit an entity-missing failure.

## Review items

These six decisions are part of the panel-anchoring ad hoc review. Their
answers are recorded in this document rather than duplicated in
[`panel_anchoring_features.md`](panel_anchoring_features.md).

The later A9 review established that a valid `Arrangement` does not require any
of `Strip`, `Accordion`, or `Coil`. M2's exactly-one validity premise is
therefore rejected. A9.2 is now decomposed into A9.2.1-A9.2.7 and no longer
assumes that folding recipes survive as ECS components at all. A transient
recipe model would make M1, M3, M4, and M5 unnecessary; those decisions remain
open until A9.2 selects the representation after A6 defines the
topology-provider contract.

**Confirmed decisions:** 1

- **M1 — Plugin ownership:** Confirm the plugin name `ArrangementPlugin` and whether it should absorb the
  two existing member observers as recommended.
- **M2 — Valid cardinality:** **Decided:** a valid arrangement may carry no
  built-in endpoint recipe. If recipes survive as ECS state, their cardinality
  is at most one; A9.2 now first decides whether transient authoring removes
  that runtime cardinality concern entirely.
- **M3 — Transition winner:** Confirm the incoming-component-wins rule for one-at-a-time transitions.
- **M4 — Conflicting insertion:** Decide whether multi-component insertion should merely resolve to no active
  arrangement, emit a debug diagnostic, or become a documented unsupported
  authoring error.
- **M5 — Switching helpers:** Decide whether public convenience functions for switching to `Strip`,
  `Accordion`, and `Coil` are useful for reflected/editor-driven callers.
- **M6 — Bevy dependency:** Revisit this observer mechanism after the Bevy 0.20 native mutually exclusive
  component investigation; do not commit to a pinned Bevy dependency from this
  note alone.
