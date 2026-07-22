# Arrangement mutual-exclusion research

## Status

Closed. The final V1 design in [arrangements.md](arrangements.md) does not need
mutually exclusive arrangement-recipe components.

## Original question

The proof of concept represented `Strip`, `Accordion`, and `Coil` as separate
first-class ECS components. Bevy 0.19 allowed an entity to carry any combination
of them, so the review investigated whether lifecycle observers should enforce
an at-most-one invariant and whether adopting a development Bevy revision for
native mutually exclusive components would be worthwhile.

## Research result from the Hana binary

The Hana binary uses marker-component state machines for genuinely exclusive
runtime states such as `Idle`, `Hovered`, and `Selected`. An `On<Add, T>`
observer removes sibling state components:

```rust
fn on_selected_added(added: On<Add, Selected>, mut commands: Commands) {
    commands.entity(added.entity).remove::<Idle>();
    commands.entity(added.entity).remove::<Hovered>();
}
```

The same pattern is used for `Icon` versus `Gizmo` and `Icon` versus
`Expanded`. It gives the newly added component precedence after deferred
commands are applied. Replacing the value of an already-present state emits
`Insert`, not `Add`, so it does not perform a state transition.

The relevant sources at the time of research were:

- `../hana/crates/hana/src/movable/state.rs` — marker state components,
  observer registration, sibling removal, and a deferred-ordering hazard;
- `../hana/crates/hana/src/selection/dimension_lock/affordance.rs` and
  `../hana/crates/hana/src/selection/dimension_lock/mod.rs` — `Icon`/`Gizmo`;
- `../hana/crates/hana/src/selection/rotate/affordance.rs` and
  `../hana/crates/hana/src/selection/rotate/mod.rs` — `Icon`/`Expanded`.

The pattern has important limits:

- it enforces at most one component, not exactly one;
- its invariant holds only after deferred observer commands are flushed;
- co-inserting several exclusive components can cause every observer to remove
  its siblings, leaving no winner;
- observers added after state already exists do not repair it; and
- removal observers that reinsert displaced components can create ping-pong
  transitions.

Those limits make the observer pattern suitable only when the components are
real, persistent runtime states and callers transition one destination at a
time.

## Final arrangement decision

V1 separates three different concepts:

- `Arrangement` is controller identity.
- `Member` / `Members` records arrangement membership.
- Accordion, Coil, Wrap, and downstream `FoldRecipe` implementations are
  transient authoring values that calculate hinge assignments and are then
  discarded.

`Strip` is removed. A one-row or one-column `QuadSheet` or `TriangleSheet` is
an ordinary degenerate sheet. An arrangement may have no folding groups, no
recipe, and no `FoldSequence` while remaining valid.

Because recipes are not competing ECS components, there is no recipe mutex to
enforce, no transition winner to define, no conflicting recipe bundle to
diagnose, and no switching helper for component state. `ArrangementPlugin`
still owns arrangement construction and baseline hinge evaluation, but it does
not register exclusivity observers.

Recipe replacement has its own narrower rule: `apply_fold_recipe()` may replace
hinge calibration only while playback is at the shared base endpoint. That is
a command conflict policy, not component mutual exclusion.

## Bevy dependency conclusion

Native mutually exclusive component support would not simplify the confirmed
V1 model because V1 has no mutually exclusive recipe components. This research
therefore provides no reason to pin `bevy_hana`, `bevy_lagrange`, or the Hana
binary to a development Bevy revision.

If a future feature introduces genuinely exclusive persistent ECS states, use
the then-current stable Bevy facility when available. On Bevy versions without
one, the Hana lifecycle-observer pattern remains a viable short-term fallback
when its deferred ordering and one-at-a-time transition contract are explicit
and tested.
