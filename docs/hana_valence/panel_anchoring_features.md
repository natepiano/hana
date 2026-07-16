# Panel anchoring feature plan

## Purpose

The `hana_diegetic` `panel_anchoring` example contains behavior that is useful
beyond panels. This document inventories that behavior, records the API and
feature review decisions, and accumulates the resulting implementation work in
dependency order. It is the source plan that will later be converted into a
phased work plan after in-scope and out-of-scope work is decided.

## Review preservation contract

The detailed review ledger in this document is authoritative. The cohesion
overview is an additional view over that ledger, not its replacement.

- Do not delete, merge, renumber, or materially rewrite an existing review
  item or recorded decision while constructing the overview.
- Map every overview statement back to the review items that support it so
  omitted concerns remain visible.
- Record possible type removal, type merging, or responsibility movement as a
  proposal. It changes the ledger only after an explicit user decision.
- Preserve a superseded decision in the ledger and point to its replacement
  instead of erasing its history.
- Resume the paused one-item review only after the overview has been checked
  against every foundational, feature, and mutual-exclusion item.
- Introduce each type with one succinct sentence stating what data it contains
  and, when relevant, the exact function or system that reads or writes it.
- Do not use aggregate verbs such as "derive," "drive," or "own" as a substitute
  for naming the components read, the components written, and the function or
  system performing the operation.
- Present only one new type or one exact data-flow step at a time. Do not ask
  for confirmation of a diagram containing undefined types or operations.
- Do not presume that an existing type deserves to survive merely because the
  cohesion overview can explain it.
- Before accepting a new type, state which existing type, field, query branch,
  or system it replaces and report the net change in concepts and systems.
- Compare every proposal with a simpler alternative that uses fewer public
  types or fewer writers. Record removals as proposals until the user decides
  them explicitly under this preservation contract.
- Label every code sample with who writes it and when it executes. When a
  public convenience method hides deferred work, show the application call
  first and the Hana-owned deferred implementation separately; do not present
  internal library code as if the application must write it.

The overview may therefore become much smaller than the ledger without making
the work list smaller.

### Shared semantic math types

Use existing `bevy_kana` semantic math types in planned Hana Valence APIs
whenever their meaning matches: `Position` for spatial points, `Displacement`
for spatial offsets, `Orientation` for rotations, and `Angle` for signed
angular displacement. Keep raw Bevy/glam values only at interoperability and
calculation boundaries or when no shared type expresses the value. Do not add a
Hana-specific wrapper that merely restates an existing `bevy_kana` meaning.

A7.5 audits the remaining anchor and hinge fields against this rule so later
work cannot apply it selectively.

## Public API ergonomics reset

The foundational item ledger is paused while the public authoring workflow is
proved end to end. Internal topology generation, logical-key-to-`Entity`
mapping, `MemberTopology`, and `Connection` must not appear in the ordinary
application-facing construction path.

### Confirmed arrangement construction model

Support two friendly construction paths over the same topology provider:

1. `spawn_arrangement()` spawns the `Arrangement` controller and bare `Member`
   entities, then lets the caller iterate those members and add application
   components afterward.
2. A convenience variant (`name TBD`) accepts a named member-construction
   function and inserts application components while each member is spawned.

Both paths return the `Entity` of the `Arrangement` controller. The confirmed
`Arrangement` component and `Member` / `Members` relationship model already
represent the spawned arrangement in ECS, so Hana adds no caller-owned
arrangement-result wrapper. Topology remains an authoring input rather than the
public result name.

```rust
let arrangement_entity =
    commands.spawn_arrangement(TriangleSheet::new(4, 6))?;
```

The construction-time convenience remains readable by passing a named
function rather than an inline closure:

```rust
let arrangement_entity = commands.spawn_arrangement_with( // method name TBD
    TriangleSheet::new(4, 6),
    make_triangle_panel,
)?;
```

Because `Commands` is deferred, the bare path cannot query the newly maintained
`Members` in the same system invocation. The convenience path covers immediate
per-member customization. The bare path uses an ordinary later system that
queries `Members` after the commands have been applied:

```rust
fn decorate_new_arrangements(
    arrangements: Query<&Members, Added<Arrangement>>,
    mut commands: Commands,
) {
    for members in &arrangements {
        for member_entity in members.iter() {
            commands.entity(member_entity).insert(panel_bundle());
        }
    }
}
```

Do not return a duplicate member `Vec<Entity>` merely to permit same-system
iteration. It would duplicate the authoritative `Members` relationship target
and could become stale as membership changes.

**Status:** Construction behavior and returning the controller `Entity`
decided. Immediate customization uses the convenience path and later
customization queries `Members`; only the convenience-method name remains
pending.

This result correction supersedes the caller-owned `Topology` return portions
of A6.5-A6.6. Those ledger entries remain intact until the complete public
workflow is settled, at which point their retained topology-authoring concerns
can be reconciled without restoring the rejected result wrapper.

### Confirmed folding-capability retention

A `TopologyProvider` may identify normalized groups of physical connections
that support compatible folding behaviors. `Members` records only arrangement
membership, and `AnchoredTo` records only physical connections; neither value
preserves provider concepts such as the horizontal groups of a sheet.

`spawn_arrangement()` therefore stores the provider-generated folding
capabilities (`name and representation TBD`) as internal ECS data on the
`Arrangement` controller. It does not retain the concrete provider. This lets
an existing arrangement accept a different recipe or a changed recipe without
requiring the caller to reconstruct and resupply its original provider:

```rust
let arrangement_entity = commands.spawn_arrangement_with(
    TriangleSheet::new(4, 6),
    make_triangle_panel,
)?;

commands.apply_fold_recipe( // method name TBD
    arrangement_entity,
    Accordion::default(), // recipe representation TBD
);
```

This decision settles the persistence responsibility from A9.2.3 while leaving
the capability's exact value type and recipe compatibility contract for the
reconciled detailed design.

**Status:** Decided.

### Confirmed transient recipe lifecycle

A folding recipe is an ordinary, transient authoring value. It reads the
normalized folding capabilities retained on the `Arrangement` controller and
authors complete folded and unfolded endpoint values on the affected `Hinge`
components. Hana does not retain the recipe as an ECS component after that
application.

```rust
commands.apply_fold_recipe( // method name TBD
    arrangement_entity,
    Accordion::default(), // recipe representation TBD
);

commands.apply_fold_recipe(
    arrangement_entity,
    adjusted_accordion,
);
```

An application may retain its recipe value in application-owned state and
submit it again after editing. Applying a different or adjusted recipe does not
require recreating the arrangement or its topology provider. Accordion, Coil,
and Wrap therefore are not mutually exclusive ECS components in this model.

This decides A9.2.1. Compatibility failure behavior and replacement while a
`FoldSequence` is away from its neutral endpoint remain under A9.2.4 and
A9.2.7.

**Status:** Decided.

### Confirmed open recipe application path

Hana supplies built-in recipes such as Accordion, Coil, and Wrap, but the recipe
set is not closed. Application-defined recipes use the same application method
as Hana's built-ins:

```rust
commands.apply_fold_recipe( // method name TBD
    arrangement_entity,
    Accordion::default(),
);

commands.apply_fold_recipe(
    arrangement_entity,
    MyFanFold::new(),
);
```

Do not add one method per built-in recipe or a closed enum that prevents
application-defined recipe values. Use one public `FoldRecipe` trait as that
extension contract, matching the existing `FoldSequence`, `FoldStage`, and
`FoldCommand` vocabulary:

```rust
pub trait FoldRecipe {
    // The authoring method is the next public-API decision.
}

fn apply_fold_recipe<R: FoldRecipe>( // method name TBD
    &mut self,
    arrangement_entity: Entity,
    recipe: R,
);
```

`FoldRecipe` is an authoring extension trait, not a trait object or ECS
component. Each built-in or application-defined recipe remains its own
ordinary Rust value.

**Status:** Decided.

### Confirmed recipe output

One `FoldRecipe` invocation returns the complete unvalidated set of
recipe-owned values for proposed `Hinge` replacements as
`Vec<HingeAssignment>`:

```rust
pub struct HingeAssignment {
    pub member_entity: Entity,
    pub unfolded_angle: Angle,       // endpoint field name TBD
    pub folded_angle: Angle,         // endpoint field name TBD
    pub pivot_offset: Displacement,
}
```

`HingeAssignment` contains only values owned by the recipe. It does not contain
the topology-authored `Edge`; after validation, Hana combines the assignment
with the retained connection data to construct the complete `Hinge`.

Use a vector rather than a map so duplicate assignments for one `Member`
entity remain visible to Hana's whole-result validation instead of being
silently overwritten during map construction. Preserve vector order for
deterministic diagnostics; it expresses no folding or connection order.

Do not add a `HingeAssignments` collection wrapper yet. The vector is
deliberately unvalidated and is consumed immediately after validation, so no
separate collection invariant or behavior currently justifies another public
type.

Name the pure `FoldRecipe` calculation method `hinge_assignments()`. It returns
the values to be considered and does not mutate ECS; the deferred
`apply_fold_recipe()` operation (`Commands` extension method name TBD) performs
validation and application later. Hana resolves the `Member` entities in each
supplied `FoldGroup` to topology-compatible `Connection` values before calling
the recipe. A9.2.4 decides the exact resolved recipe-input type and error
contract.

**Status:** Pure `hinge_assignments()` calculation and
`Vec<HingeAssignment>` output decided. `FoldGroup` membership is decided by
A9.2.2.1; the topology-resolved input type and error contract remain pending.

### Confirmed automatic recipe validation

The application invokes one public `Commands` extension method:

```rust
// Application code: queues the operation during a system.
commands.apply_fold_recipe( // method name TBD
    &topology,
    selected_fold_groups,
    Accordion::default(),
);
```

Hana implements that extension by queuing one deferred command. The closure is
library implementation; the application neither writes nor calls its validation
functions:

```rust
// Hana implementation: runs later when Bevy applies deferred commands.
self.queue(move |world: &mut World| -> Result {
    let assignments = prepare_hinge_assignments(
        world,
        arrangement_entity,
        &recipe,
    )?;

    apply_hinge_assignments(world, assignments);
    Ok(())
});
```

`prepare_hinge_assignments()` resolves the supplied `FoldGroup` member entities
through the caller-owned `Topology`, calls `FoldRecipe::hinge_assignments()`,
and validates the complete returned vector before
`apply_hinge_assignments()` performs the first ECS write. The validation always
runs for Hana's built-ins and application-defined recipes; there is no public
unchecked bypass and no validation call the application must remember. The
exact owned values captured by the deferred command remain part of A9.2.7.

Validation owns structural and runtime consistency: current entity existence,
`Member` ownership by the target `Arrangement`, eligibility in the retained
fold groups, exact assignment coverage for the groups supplied to the recipe,
duplicate or conflicting assignments, and finite endpoint and pivot values. It
does not judge artistic intent, collisions, polyhedral closure, or finite
multi-turn angles.

Because the operation is deferred, its exact diagnostic and error-reporting
surface remains pending; the public call cannot return the later validation
result synchronously.

**Status:** Decided.

### Confirmed exact recipe-assignment coverage

One recipe invocation must return exactly one `HingeAssignment` for every
`Connection` resolved from the `Member` entities in the `FoldGroup` values
supplied to that invocation. The assignment and connection are paired by
`member_entity`.

`HingeAssignment` is not a field on `Connection`. The provider creates and
Hana retains `Connection`; the transient recipe creates `HingeAssignment`; Hana
validates the complete assignment vector and combines each pair into the
durable `Hinge` component.

```text
Connection + HingeAssignment -> Hinge
```

Hana rejects a group member that cannot resolve to exactly one compatible
`Connection`. It also rejects a recipe result that omits a resolved connection,
assigns one member more than once, assigns a member outside the supplied
groups, or is based on supplied groups that contain the same member more than
once. A recipe that wants a connection to remain fixed returns equal unfolded
and folded angles rather than omitting its assignment.

Only the connections in the groups supplied to this invocation receive
recipe-authored `Hinge` replacements. Existing `Hinge` components outside that
input remain untouched. This coverage rule does not decide the later API for
selecting a subset of retained groups.

**Status:** Decided.

### Confirmed fold-group value

`FoldGroup` is the one reusable, nonempty, ordered grouping of `Member` entity
values used by both recipe authoring and playback authoring. It contains no
`Connection`, recipe, timing, or ECS state:

```rust
pub struct FoldGroup {
    members: Vec<Entity>,
}

impl FoldGroup {
    pub fn new(
        first: Entity,
        remaining: impl IntoIterator<Item = Entity>,
    ) -> Self {
        let mut members = vec![first];
        members.extend(remaining);
        Self { members }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.members.iter()
    }
}
```

The infallible constructor makes the ordinary path nonempty by construction
and preserves `first` followed by the iteration order of `remaining`. When a
provider generates a complete vector dynamically, use the standard fallible
conversion:

```rust
let group = FoldGroup::try_from(generated_member_entities)?;
```

`TryFrom<Vec<Entity>>` rejects an empty vector and otherwise preserves it
unchanged. Its concrete named error type remains part of the later error and
diagnostic decision.

The member order is semantic. A topology provider or application orders members
within the group, and a recipe or playback author may use that position to
alternate values, produce a wave, or calculate other position-dependent
behavior. Members in one group therefore need not receive identical endpoint
or timing values. The author defines what increasing member position means;
provider documentation should describe that meaning when the group comes from
a provider, but Hana does not store or validate a universal geometric
direction.

The member index is implicit in the underlying vector position. Do not add
a duplicate index field or index type. Hana preserves provider order, and a
recipe obtains the topology-local `usize` position through `enumerate()`.

It is an ordinary public authoring value, not an ECS component. `Topology`
separately retains the `MemberTopology` / `Connection` data used to resolve a
group for compatible recipe application. The confirmed
`Topology::fold_groups` field supplies provider-authored groups; applications
may also construct groups directly.

**Status:** Nonempty ordered grouping, semantic order, provider-defined
direction, implicit positional index, member-entity storage, and construction
behavior decided. The fallible conversion's error type remains pending with
diagnostics.

### Confirmed A9.2.2.1: one group, separate operations

`FoldGroup` is the shared grouping value. Do not add `MemberSelection` or
`SharedFoldStage` as a second wrapper around the same `Member` entity list.

Recipe authoring and playback authoring remain separate operations over that
one value:

```text
FoldGroup + Topology + FoldRecipe -> HingeAssignment values
FoldGroup + playback timing       -> FoldStage
```

A topology-provided `FoldGroup` does not have to be played. Conversely, an
application-defined group may become a playback stage without being compatible
with Accordion or any other particular `FoldRecipe`. Recipe application first
resolves the group's members through `Topology` and validates the recipe's
topology contract.

`FoldStage` is the coordinated-playback use of a `FoldGroup`, not another
member-group collection. The same group may be used with multiple recipes or
multiple timing arrangements, and recipe grouping need not match playback
grouping. Four box walls may form one recipe group but four rapid stages; many
Accordion groups may instead be combined into one simultaneous stage.

```rust
let walls = FoldGroup::new(wall_1, [wall_2, wall_3, wall_4]);

let stage = walls.clone().with_timing(snap_timing); // method/type names TBD
apply_recipe(&topology, stage.group(), BoxFold::new())?; // method name TBD
```

Complex recipes may require several role-bearing `FoldGroup` values, such as
box walls plus a lid, rather than pretending that one flat group encodes every
topology contract.

This decision supersedes the proposed `SharedFoldStage` type. A9.3.1.1 further
decides that `FoldSequence` owns complete `FoldStage` values and each
`FoldStage` owns one `FoldGroup`; remove the public numeric `FoldStage(usize)`
component from each `Member`. A9.3.1.2 decides the coordinated timing stored by
the stage, and A9.3.2 decides the functional group-to-stage authoring API.
A9.2.4 decides the exact topology-resolved recipe input.

**Status:** Decided: use one reusable `FoldGroup` of ordered `Member` entities;
keep recipe interpretation and coordinated playback as separate operations;
remove `SharedFoldStage`; use sequence-owned `FoldStage` values that each own a
`FoldGroup`; remove the public per-member numeric stage component.

### Confirmed topology-provider and recipe separation

`TopologyProvider` owns the basic spatial layout. It creates the physical
`Connection` values and identifies which ordered `Member` entities form each
`FoldGroup`. A compatible `FoldRecipe` consumes topology-resolved groups and
calculates `HingeAssignment` values; it does not rediscover or redefine the
layout.

The relationship is many-to-many. One provider's groups may support several
recipes, and one recipe may operate on groups created by many providers. Hana
built-ins and application-defined implementations may be mixed independently:

```text
TopologyProvider -> Topology { Connections, FoldGroups }
FoldGroup + Topology + FoldRecipe -> HingeAssignment
```

`FoldGroup` is therefore publicly constructible. Hana's built-in providers,
application-defined providers, and direct topology authors may all create it.
`TopologyProvider::generate_topology()` transfers the provider's member
connections and fold groups together in the confirmed `Topology` value below.

**Status:** Decided.

### Confirmed topology-provider output

`TopologyProvider::generate_topology()` returns one named, pure authoring value
containing both the member topology and its fold groups:

```rust
pub struct Topology {
    pub members: Vec<MemberTopology>,
    pub fold_groups: FoldGroups,
}
```

Returning both collections together lets Hana validate that every retained
group refers to the same connections being materialized. It avoids an unnamed
tuple and separate connection/group generation calls that could disagree.

`Topology` is not returned by `spawn_arrangement()`. A built-in or custom
provider creates it outside the `World`; Hana consumes it while spawning or
applying topology and returns only the `Arrangement` controller `Entity`.
Direct authors may supply the same value to `apply_topology()` for an existing
arrangement.

```rust
// Application code
let arrangement_entity = commands.spawn_arrangement_with(
    MyNet::new(),
    make_panel,
)?;

// Custom TopologyProvider implementation
Ok(Topology {
    members,
    fold_groups,
})
```

This restores the previously planned `Topology` name only for the value that
actually describes topology; it does not restore the rejected caller-owned
spawn-result wrapper.

The confirmed alternative-groupings requirement below reopens only the exact
type of the singular `fold_groups` field shown above. Returning one `Topology`
that keeps member topology and folding capabilities together remains decided.

**Status:** `Topology` output and responsibility decided; exact folding-
capability field representation reopened for alternative `FoldGroups`.

### Confirmed fold-group capability collection

Use a semantic sum type rather than `Option<Vec<FoldGroup>>` or an empty vector
to distinguish whether a `Topology` provides generic fold-group capability:

```rust
pub enum FoldGroups {
    /// This topology does not expose groups for generic FoldRecipe use.
    NotProvided,

    /// This topology exposes one or more ordered FoldGroup values.
    Provided(Vec<FoldGroup>),
}
```

The outer vector order is also semantic: it locates each `FoldGroup` within the
provider-defined progression across groups. Together, the outer group index and
inner member index give recipes two independent positions without assuming
that every group has the same length or that the topology is a rectangular
grid. The provider likewise defines what increasing group position means, and
its documentation should describe that meaning.

The group index is also implicit in its vector position. Hana preserves the
order and recipes obtain the topology-local `usize` position through
`enumerate()`. Neither implicit index is a persistent identity across topology
replacement.

```rust
// FoldRecipe implementation
for (group_index, group) in fold_groups.iter().enumerate() {
    for (member_index, member_entity) in group.iter().enumerate() {
        // Hana resolves member_entity to Connection before recipe calculation.
        // A recipe may use either semantic position.
    }
}
```

This ordered authoring capability can support position-dependent recipes. It
does not itself introduce cloth simulation state or runtime constraints.

The normal constructor requires the first group separately, making its output
nonempty by construction. Whole-`Topology` validation still rejects a directly
constructed `Provided(Vec::new())`.

```rust
impl FoldGroups {
    pub fn provided(
        first: FoldGroup,
        remaining: impl IntoIterator<Item = FoldGroup>,
    ) -> Self {
        let mut groups = vec![first];
        groups.extend(remaining);
        Self::Provided(groups)
    }

    pub const fn is_provided(&self) -> bool {
        matches!(self, Self::Provided(_))
    }
}
```

Expose a read-only collection view. `NotProvided` dereferences to an empty
slice for ordinary iteration, while callers that need the capability
distinction use `is_provided()` or pattern matching. Implement
`IntoIterator for &FoldGroups` as matching iteration sugar, but do not
implement `DerefMut`; changes must return through topology validation.

```rust
impl Deref for FoldGroups {
    type Target = [FoldGroup];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::NotProvided => &[],
            Self::Provided(groups) => groups.as_slice(),
        }
    }
}

for group in &topology.fold_groups {
    // IntoIterator for &FoldGroups
}
```

**Status:** The semantic absence distinction, nonempty provided collection,
ordering, and read-only access are decided. Whether this exact enum is also the
outer container for every alternative supported by a topology is reopened by
the requirement below.

### Confirmed requirement for A9.2.3.1: alternative fold-groupings

A `TopologyProvider` may expose multiple alternative `FoldGroups` for the same
physical topology. Rows and columns of a sheet are the current concrete
example:

```text
rows    -> FoldGroups
columns -> FoldGroups
```

Rows and columns are examples rather than Hana-defined variants. The domain may
later reveal diagonal, radial, concentric, irregular, or other provider-defined
alternatives, so Hana must not hard-code a taxonomy before those requirements
are understood.

Do not concatenate alternatives into one `FoldGroups` merely to fit the current
singular `Topology::fold_groups` field. The exact assignment-coverage rule
continues to apply to whichever `FoldGroups` are ultimately supplied to a
recipe invocation. It does not decide how an alternative is stored, named,
selected, or supplied.

The current singular field and the use of `FoldGroups` as both one ordered
recipe input and the topology's complete capability container are therefore
provisional. The representation and selection API remain deferred until
built-in provider and recipe examples give the domain model enough evidence.

**Status:** Ability to expose alternative `FoldGroups` decided; taxonomy,
storage, identity, selection, and application API deferred to A9.2.3.1.

### Confirmed recipe capability boundary

After obtaining one `FoldGroups` alternative through the still-pending
selection mechanism, Hana matches that value before invoking a recipe. A
`FoldGroups::NotProvided` value is an application-time compatibility failure;
the recipe is not called and no `Hinge` values change. For
`FoldGroups::Provided`, Hana preserves the validated, nonempty
`&[FoldGroup]`, resolves each member through the same `Topology`, and supplies
the resulting topology-qualified input to `FoldRecipe::hinge_assignments()`.
The exact resolved input type remains A9.2.4.

```rust
// Hana implementation, after selecting an alternative (mechanism TBD)
let fold_groups = match selected_fold_groups {
    FoldGroups::NotProvided => {
        return Err(/* fold groups not provided; error type TBD */);
    }
    FoldGroups::Provided(groups) => groups.as_slice(),
};

let resolved_groups = resolve_recipe_groups( // function/type names TBD
    topology,
    fold_groups,
)?;
let assignments = recipe.hinge_assignments(&resolved_groups)?;
```

The read-only `Deref` still treats `NotProvided` as an empty slice for ordinary
inspection and iteration, but recipe application preserves the semantic sum
distinction. Custom recipes never check absence or interpret an empty input.

**Status:** Decided.

### Confirmed A9.2.5.1: Accordion alternation position

The built-in `Accordion` recipe alternates physical mountain-or-valley fold
sense by the outer `FoldGroup` position. Every member-resolved `Connection`
within one group receives the same physical sense, while the next group
receives the opposite sense. Exact `HingeAssignment` values may still differ
per connection when geometry calibration requires it.

`Accordion` does not use the inner member position to choose fold sense.
That semantic position remains available to custom or future recipes that
alternate, wave, or otherwise vary values along one group. A strip is the
degenerate case in which each `FoldGroup` contains one foldable `Member`.

The confirmed borrowed collection APIs support a functional implementation;
no imperative assignment-builder API is required:

```rust
// FoldRecipe implementation. `configured_offset: Angle` is recipe input;
// its public field name and exact endpoint/pivot mapping remain TBD.
let assignments = resolved_groups // topology-resolved view; type name TBD
    .iter()
    .enumerate()
    .flat_map(|(group_index, group)| {
        let group_offset = if group_index.is_multiple_of(2) {
            configured_offset
        } else {
            -configured_offset
        };

        group.iter().map(move |connection| {
            hinge_assignment(connection, group_offset)
            // Recipe-local helper returning HingeAssignment; exact mapping TBD.
        })
    })
    .collect::<Vec<HingeAssignment>>();
```

`FoldRecipe::hinge_assignments()` returns that vector for Hana's automatic
whole-result validation. Endpoint mapping and pivot calibration remain pending
within A9.2.5-A9.2.6.

**Status:** Decided.

### Confirmed A9.2.5.2: signed Accordion offset

The transient `Accordion` recipe contains one signed `Angle` (`field name TBD`).
Its magnitude is the alternating fold offset, and its sign chooses the physical
fold sense of the first `FoldGroup`. Each following group negates that value.
Negating the configured `Angle` reverses the complete Accordion pattern without
a separate direction enum or boolean.

```rust
pub struct Accordion {
    pub fold_offset: Angle, // field name TBD
}

// Application code; constructor name TBD.
commands.apply_fold_recipe(
    arrangement_entity,
    Accordion::new(first_group_offset), // first_group_offset: Angle
);
```

`Accordion` contains no live progress field. The confirmed `FoldSequence` and
`FoldSequenceState` path owns transport, and finite multi-turn `Angle` values
remain valid.

**Status:** Behavior decided; public field and constructor names pending.

### Confirmed A9.2.5.2.1: default Accordion offset

`Accordion::default()` uses a signed positive half-turn (`+pi` radians) as its
fold offset. This preserves the current convenient behavior: ideal
zero-thickness panels fold back 180 degrees onto their neighbors.

The half-turn is a default, not an invariant. Explicit Accordion construction
accepts any finite signed `Angle`: smaller magnitudes create shallower pleats,
and negating the value reverses the first group's physical fold sense. This
remains Accordion behavior rather than Wrap behavior because the offset still
alternates between successive `FoldGroup` values.

```rust
Accordion::default();          // signed fold offset = +pi
Accordion::new(custom_offset); // custom_offset: Angle; constructor name TBD
```

The concrete `bevy_kana::Angle` constructor and constant spellings remain
pending because that planned semantic type is not implemented yet.

**Status:** Default and configurability decided; exact `Angle` and Accordion
constructor spellings pending.

### Confirmed requirement for A9.3.1: interchangeable playback timing

One set of recipe-authored `Hinge` endpoints must support multiple playback
arrangements without rerunning or changing the `FoldRecipe`:

- all affected `Member` entities moving simultaneously;
- successive `FoldGroup` values moving strictly one at a time; and
- overlapping stagger or wave timing, where a later group begins before an
  earlier group finishes.

`FoldRecipe` continues to answer what each `Hinge` endpoint and pivot value is.
Playback authoring answers when each participating `Member` travels between
those endpoints. The same Accordion, Coil, Wrap, or custom recipe output may
therefore be paired with different playback authoring.

The existing implementation's numeric `FoldStage` model covers the first two
cases. Assigning
every member the same numeric stage moves them simultaneously; assigning
successive groups consecutive values moves them one at a time. It does not
cover overlapping waves because each integer stage currently owns one
consecutive non-overlapping interval, and per-member easing changes only the
curve within that shared interval. A9.3.1.1 supersedes that representation:
the public `FoldStage` is now a sequence-owned value containing its reusable
`FoldGroup`, with no public numeric stage component on each member.

```rust
// Playback authoring over the same FoldGroups; exact API remains A9.3.1.
simultaneous(fold_groups); // combine groups into one coordinated FoldStage
sequential(fold_groups);   // map each FoldGroup to a successive FoldStage
wave(fold_groups);         // overlapping timing is required but not yet modeled
```

A9.3.1.2.1 keeps successive `FoldStage` values as consecutive,
non-overlapping sequence steps. Every overlapping stagger or wave belongs to
the member timing inside one stage. This retains unambiguous stage boundaries
for `Step(FoldDirection)` without merging endpoint generation back into
playback.

Additional artistic timing cases refine that requirement:

- four box sides may occupy four successive `FoldStage` values with short
  durations and snap-like easing, producing a rapid one-two-three-four motion;
- one complete `FoldGroup` may become one timed `FoldStage`;
- selected members from a `FoldGroup`, such as alternating or contiguous
  subsets, may form new groups that become separate stages with independently
  authored timing; and
- sequence defaults, stage overrides, and supported member-level overrides may
  be combined without changing the underlying `Hinge` endpoints.

`FoldGroup` is the ordered member grouping shared by recipe and playback
authoring. `FoldStage` adds coordinated-playback semantics and timing to such a
group. The existing A9.3 timing decision already associates a
duration-and-easing value (`type name TBD`) with a stage; A9.3.1.2 now decides
how intra-stage timing represents simultaneous and staggered motion.

A9.3.2 separately decides the functional API that transforms, combines, or
subdivides timing-free `FoldGroup` values into timed `FoldStage` values. It does
not reopen the decision that `FoldGroup` itself stores no timing.

**Status:** Playback-variation requirement decided; overlapping timing
is confined to one stage by A9.3.1.2.1, resolved member timing is decided by
A9.3.1.2.2, and its whole-value cascade is decided by A9.3.1.2.3.
Group-to-stage functional authoring remains A9.3.2.

### Confirmed A9.3.1.1: sequence-owned stages

`FoldSequence` owns the complete authored stage collection, and every
`FoldStage` owns exactly one `FoldGroup`:

```rust
pub struct FoldSequence {
    stages: Vec<FoldStage>,
    // Sequence defaults and other authored fields.
}

pub struct FoldStage {
    group: FoldGroup,
    // Coordinated timing representation: A9.3.1.2.
}
```

Remove the public numeric `FoldStage(usize)` component from `Member` entities.
It is an implementation-oriented label rather than the domain stage: it does
not contain the participants or their timing, and it duplicates membership
already owned by the authored sequence.

`FoldSequence` is the one source of truth for playback membership, ordering,
and timing. `Member` / `Members` remains the one Bevy relationship for assembly
membership. A `Member` omitted from every sequence-owned stage is fixed for
that sequence.

Runtime systems resolve a member's stage from `FoldSequence`. Hana may add a
private derived lookup or cache if profiling requires one, but no public
per-member stage component or second relationship is part of the authoring
model.

Because `FoldStage` owns its `FoldGroup`, it can expose that same group for
later compatible recipe application without reconstructing a second member
collection. The exact accessor and lookup method names remain pending.

**Status:** Decided. A9.3.1.2 defines coordinated timing within a stage;
A9.3.2 defines functional `FoldGroup`-to-`FoldStage` construction.

A9.3.1.2 is reviewed through three atomic decisions:

1. **A9.3.1.2.1 — Stage boundaries:** Decide whether successive `FoldStage`
   values are non-overlapping sequence steps and every overlap belongs to
   intra-stage member timing.
2. **A9.3.1.2.2 — Member timing value:** Define the value that locates one
   member's start, duration, and easing within its stage.
3. **A9.3.1.2.3 — Timing cascade:** Decide how sequence defaults, stage timing,
   and member timing compose and which scopes reuse the same value type.

A9.3.1.2.3.1 separately decides where the generic `Cascade<T>` authoring value
lives. That ownership question does not reopen the timing-cascade behavior.

### Confirmed A9.3.1.2.1: non-overlapping stage boundaries

Successive `FoldStage` values are non-overlapping sequence steps. A stage
boundary is a position where `Step(FoldDirection)` may stop. Every simultaneous,
staggered, or wave motion that overlaps in time belongs to the member timing
inside one stage.

Recipe grouping and playback grouping remain independent. For a pleated
curtain, Accordion may consume one original `FoldGroup` per pleat while playback
functionally combines pairs of those groups into successive stage-owned groups:

```text
Accordion groups:  [g0] [g1] [g2] [g3]
Playback stages:   [g0 + g1] [g2 + g3]
```

The combination preserves the original groups and concatenates their member
order into a new `FoldGroup`. The first two pleats may therefore move together,
then the next two, without changing Accordion's alternating endpoint
calculation.

```rust
for pair in pleat_groups.chunks(2) {
    sequence.stage_with_timing( // method name TBD
        FoldGroup::combine(pair)?, // method name TBD
        simultaneous_timing,       // type/name TBD
    );
}
```

Use multiple stages when `Step` must stop between actions. Use one stage with
staggered member timing when those actions form one coordinated flourish.
Introduce overlapping stages only after a demonstrated example cannot be
represented this way.

**Status:** Decided. A9.3.1.2.2 now defines the resolved timing of one member
inside its stage.

### Confirmed A9.3.1.2.2: resolved member timing

One complete ordinary value (`FoldTiming`, name TBD) represents a member's
resolved timing relative to the beginning of its `FoldStage`:

```rust
pub struct FoldTiming { // type and field names TBD
    pub start_offset: Duration,
    pub duration: Duration,
    pub easing: EaseFunction,
}
```

Simultaneous members use the same zero offset, duration, and easing. Staggered
members use different offsets. A wave uses offsets whose separation is shorter
than the member duration, so their motion overlaps inside the stage. Members
may also use different durations and easing.

```rust
FoldTiming {
    start_offset: index * Duration::from_millis(40),
    duration: Duration::from_millis(160),
    easing: wave_easing,
}
```

The stage finishes at the greatest `start_offset + duration` among its members.
Pause and reverse operate on the one stage clock; each member derives raw local
progress from that clock and applies its easing once. Exact zero-duration,
overflow, and other validation policy remains with the later validation and
diagnostic decisions.

This decision defines the complete resolved value, not how authors avoid
repeating it. A9.3.1.2.3 resolves sequence and stage defaults to one value per
member through whole-value `Cascade<FoldTiming>` inheritance and replacement.

**Status:** Decided; type and field names remain pending.

### Confirmed A9.3.1.2.3: whole-value timing cascade

`FoldSequence` supplies one complete default `FoldTiming`. A `FoldStage` either
inherits that value or replaces it completely. Each member entry within that
stage either inherits the resolved stage value or replaces it completely:

```text
member Cascade<FoldTiming>
    > stage Cascade<FoldTiming>
    > sequence FoldTiming default
```

The root sequence value cannot inherit, so resolution always produces one
complete `FoldTiming` for every participating member. `FoldTiming` is reused at
all three scopes; no separate sequence-timing, stage-timing, or member-timing
value types are introduced.

```rust
pub struct FoldSequence {
    stages: Vec<FoldStage>,
    default_timing: FoldTiming, // field and type names TBD
}

pub struct FoldStage {
    group: FoldGroup,
    timing: Cascade<FoldTiming>, // field name and Cascade type home TBD
    // Per-member entries use Cascade<FoldTiming>; exact storage is A9.3.2.
}
```

`Cascade<T>` has the two named states already used by `hana_diegetic`:

```rust
pub enum Cascade<T> {
    Inherit,
    Override(T),
}
```

This is whole-value replacement, not a field-by-field patch. An override must
therefore provide a complete start offset, duration, and easing. Functional
authoring helpers may derive a complete override from an inherited value, but
the stored result remains unambiguous. Member overrides are stage-owned
authoring data, not components on the `Member` entities.

The fold cascade follows authored containment (`FoldSequence` to `FoldStage`
to a member entry), so it uses the storage-independent `Cascade<T>` helpers.
When an ECS cascade is needed, `bevy_kana` also provides the shared explicit
`CascadeFrom` relationship and propagation engine; it never infers
inheritance from `ChildOf`.

**Status:** Cascade behavior decided. Exact timing names, member-entry storage,
and functional construction remain separate decisions.

### Confirmed A9.3.1.2.3.1: shared cascade core and ECS engine

`bevy_kana` owns `Cascade<T>` and the operations that depend only on its two
states: inspection, borrowing, value transformation, single-layer resolution,
and ordered multi-layer resolution against a required root value. The shared
API does not convert to or from `Option<T>`; `Cascade::Inherit` and
`Cascade::Override(value)` remain explicit authored states.

`bevy_kana` also owns `CascadeFrom` / `CascadeChildren`,
`CascadeDefault<A>`, `Resolved<A>`, `CascadePlugin<A>`, `CascadeSet`, generic
commands and readers, dirty-subtree propagation, and lifecycle handling.
`hana_diegetic` chooses participating attributes, inserts `CascadeFrom` where
inheritance is intended, retains typed domain verbs, and consumes resolved
values. `ChildOf` remains limited to transform and despawn ownership.

Rust requires a cross-crate shared type to be public in its owning crate, so
`Cascade<T>` is public from `bevy_kana`. It remains an internal dependency of
`hana_diegetic` and `hana_valence`: neither crate re-exports it nor exposes it
in public fields, arguments, or return types. Domain APIs expose named
authoring verbs such as override and inherit instead.

This extraction was completed before the planned `hana_diegetic` widget work,
whose new inheriting properties can now use the shared ECS machinery. The
implementation:

- defines and exports the pure type and resolution helpers from
  `crates/bevy_kana/src/cascade.rs`;
- removes the former `hana_diegetic/src/cascade/authoring.rs` copy;
- keeps `Cascade<T>` crate-private throughout `hana_diegetic` and replaces
  public raw-state access with domain-specific authoring verbs;
- removes `hana_diegetic`'s generic hierarchy walk and propagation pass;
- updates the current cascade and shadow-casting as-built documents.

**Status:** Decided and implemented 2026-07-15.

## Cohesion overview status

The cohesion overview is being rebuilt from the smallest end-to-end model of
what an author supplies, which systems read and write that data, what changes
at runtime, and which confirmed type contains each value. The A6 behavior is
complete; A6.7 is a naming correction discovered by the A7 semantic-type audit.

### Simplification gate

The cohesion pass must reduce accidental choices rather than only document
them. These overlaps are visible in the current implementation:

- `ArrangementPlacement` and `MemberPlacement` differ by one live angle value;
  A9.4 removes both placement wrappers, while A8 selects the shared `Angle`
  value used by their surviving connection and hinge data. A6 names those
  authoring values `MemberTopology` and `Connection`.
- `TilingRule::rest_delta()`, `Accordion::lean`, and `Coil::lean` contribute to
  the two `Angle` endpoint values that A9.1 decided will live on `Hinge`; A9.2
  must still decide how those values are calculated without adding another
  placement wrapper.
- `drive_arrangement_hinges()`, `actuate_fold_hinges()`, and
  `HingeAngleLens` can all write `Hinge::angle`. A9.1 decided to remove stored
  current-angle state and derive the angle in `hinge_to_pose()`; A9.3 must
  decide how external progress enters the remaining sequence path.
- `Accordion` and `Coil` each mix a folding policy (`lean` and sign behavior)
  with live transport state (`fold`). A9 must test whether separating those
  responsibilities simplifies the whole API before introducing another type.
- `Member` / `Members` and `FoldMember` / `FoldMembers` are two relationship
  pairs with different intended membership meanings. A9 must decide whether
  both meanings are necessary and, if so, make their names unambiguous.

A9.1 resolves current-angle ownership. The remaining candidates are mandatory
comparison points for their later atomic review items, not removal decisions.

### Folding simplification investigation

One review cycle compared the live implementation, Bevy ECS ownership, public
API surface, and every demonstrated fold behavior. Three of four read-only
expert reviews completed; the minimal-model review stalled while waiting on its
own nested review and produced no findings, so the synthesis uses the three
complete reviews plus the local code inventory.

#### Converged evidence

- `drive_arrangement_hinges()`, `actuate_fold_hinges()`, and
  `HingeAngleLens` expose three ways to change the same physical result.
  `Without<FoldAngles>` prevents only one pair of writers from overlapping.
- A member carrying `FoldAngles` without a valid `FoldMember` and ready
  `FoldSequenceState` is excluded from the arrangement writer while the
  sequence writer cannot update it. Component presence therefore selects no
  guaranteed writer.
- `Accordion::fold`, `Coil::fold`, and `FoldSequenceState::position()` are
  three locations for live fold progress. The triangle example carries an
  `Accordion` with `fold == 0.0` but performs all live motion through
  `FoldSequence`; the arrangement progress is present only to satisfy the
  parallel placement API.
- A one-stage `FoldSequence` already represents simultaneous motion: assigning
  every entity `FoldStage(0)` gives every entity the same eased fraction and a
  duration independent of member count. Repeated stages already support the
  box example's simultaneous walls, while distinct stages support staggered
  folding.
- `Member` and `FoldMember` currently identify different controllers, but no
  shipped example requires those controllers to be different. The box proves
  that physical connection order and fold order differ; retaining `FoldStage`
  preserves that distinction without necessarily retaining a second
  membership relationship.
- `Edge`, unfolded angle, folded angle, and resolved progress are sufficient to
  calculate one entity's fold angle. They are not a complete transport:
  pause, target, direction, step/play motion, and pending neutral-state
  reconfiguration remain controller concerns.

#### Box counterexample and unifying direction

The `box.rs` example is not a `Strip`, `Accordion`, or `Coil`. It directly
authors a branched `AnchoredTo` graph and per-connection hinge endpoints, then
uses `FoldSequenceBuilder` to assign the lid to `FoldStage(0)` and all four
walls to `FoldStage(1)`. The center face is fixed, and the lid targets the north
wall through `AnchoredTo` even though the lid moves before that wall. The
example currently carries no `Arrangement`, `Member`, `Strip`, `Accordion`,
`Coil`, or `TilingRule`.

This is a counterexample to treating the three built-in folding components as
the exhaustive kinds of foldable assembly. The proposed unifying principle is
that the resolved assembly graph is primary:

- `Arrangement` identifies one assembly controller;
- `Member` / `Members` identifies every face in that assembly;
- `AnchoredTo` records the physical connection graph independently of member
  order;
- `Hinge` records the two calibrated endpoint `Angle` values for one foldable
  connection;
- `FoldStage` records when that connection moves; and
- `FoldSequence` plus `FoldSequenceState` controls progress through the stages.

Under this direction, `Strip`, `Accordion`, `Coil`, and future polyhedron
folding policies are optional recipes that materialize `Hinge` endpoints. They
are not exhaustive arrangement kinds. A `TopologyProvider` or
direct authoring materializes `AnchoredTo` and the hinge edge. Direct authoring
and recipes converge on the same resolved components.

One controller and one `Member` / `Members` relationship replaces
`FoldMember` / `FoldMembers`: the controller carries the optional
`FoldSequence`, while its authored `FoldStage` values coordinate reusable
`FoldGroup` values. A fixed face remains a `Member` omitted from the active
stages. A9.3.1.1 removes the public numeric per-member stage marker; Hana may
derive a private lookup cache only if profiling requires one. This deliberately
supports one active sequence per arrangement; a separate sequence-membership
relationship should return only if a demonstrated feature needs independent
sequence controllers over the same arrangement.

`FoldSequence` remains a separate optional component colocated
with `Arrangement`, rather than an optional field inside it. Component presence
then expresses the playback capability without making `Arrangement` contain an
`Option`, and Bevy can track authored `FoldSequence` changes independently of
arrangement identity.

`FoldSequenceState` and `AnchorPose` are not duplicate per-frame state.
`FoldSequenceState` is one controller-level transport state containing
readiness, progress, target, direction, and motion. `AnchorPose` is a
per-member geometric output containing rotation and translation. One
`FoldSequenceState` can therefore produce many different `AnchorPose` values
through the active stages' `FoldGroup` values and each member's `Hinge`
endpoints.

Easing is authored as a cascade but is applied only once, while
`hinge_to_pose()` derives a `Member`'s angle. `FoldSequenceState` retains raw
stage-clock progress and does not copy or store authored easing. A9.3.1.2.2
defines one complete resolved member timing (`FoldTiming`, name TBD) containing
a stage-relative start offset, duration, and easing. Members in the same
`FoldStage` may therefore overlap while using different durations and curves.
The stage ends at the latest resolved member end, and easing is applied once to
each member's raw local progress. A9.3.1.2.3 resolves that complete value by
whole-value cascade: member override, then stage override, then the required
sequence default. Both override scopes use `Cascade<FoldTiming>`; no
field-by-field timing patches or per-member ECS timing components are added.

`FoldSequenceState` remains Hana's canonical fold transport. Playback is
initiated and controlled through `FoldCommandEvent`, so an application or an
external animation timeline such as `bevy_tween` can trigger the same command
path. A future continuous external-driver adapter may feed raw sequence
progress into that transport, but it must explicitly replace Hana's native
progress writer while active and must not apply easing a second time. Hana does
not replace its fold-specific transport with `bevy_tween` or
`bevy_time_runner`.

`FoldCommandEvent` carries one of five explicit `FoldCommand` variants:
`Step(FoldDirection)`, `PlayTo(FoldEndpoint)`, `Pause`, `Resume`, or `Reverse`.
`PlayTo` names its terminal destination and is idempotent when repeated.
`Pause` retains the current journey, `Resume` continues it, and `Reverse`
retargets it without changing the raw position or whether it is a step or
continuous play. Reversing while paused leaves the sequence paused; reversing
while idle does nothing. A private paused flag is orthogonal to the retained
`FoldMotion`, and `FoldSequenceState::is_paused()` exposes that state.

An endpoint recipe reconciles only when the recipe, membership, connection, or
relevant geometry changes. Initial reconciliation must materialize valid
`Hinge` endpoints before sequence validation and advancement. Later recipe
changes while folded use the neutral-state reconfiguration tracked by F9
rather than replacing endpoints in the middle of motion.

The smallest candidate for the built-in chain recipes is one optional sum
component (`name TBD`) with `Strip`, `Accordion`, and `Coil` variants or values.
That type would enforce at most one built-in recipe structurally and eliminate
the lifecycle-observer mutex machinery. Its narrow scope would not claim to
enumerate custom or polyhedral endpoint generation, which can materialize the
same `Hinge` data directly.

**Settled A9.3.1.1 constraint:** Keep `FoldStage` as the named domain stage, but
replace the existing numeric newtype with a sequence-owned value that owns one
`FoldGroup`. Do not put a public stage component on participating `Member`
entities.

**Decided within A9.3:** Resolve one complete `FoldTiming` in this order:
member entry `Cascade::Override`, stage `Cascade::Override`, then the required
`FoldSequence` default. `Cascade::Inherit` advances to the next scope. Keep raw
progress, rather than resolved timing, in `FoldSequenceState`;
`hinge_to_pose()` applies the resolved curve when deriving the current angle.

**Decided within A9.3:** Keep `FoldSequenceState` as Hana's canonical
transport and use `FoldCommandEvent` as its request path. Other systems may
initiate that transport by triggering commands. Defer continuous
`bevy_tween` progress driving to an adapter that selects one progress writer
and supplies raw progress for Hana's easing cascade.

**Superseded part of the earlier A9.3 timing decision:** A member no longer
needs a one-member `FoldStage` merely to use a distinct duration, and a member
override is no longer limited to easing. A9.3.1.2.2 permits each member in one
stage to resolve a complete start offset, duration, and easing. The earlier
requirement to apply easing exactly once remains. A9.3.1.2.3 settles the
sequence/stage/member cascade as whole-value inheritance and replacement using
`Cascade<FoldTiming>` at the stage and member-entry scopes.

**Decided within A9.3:** Replace the context-sensitive directionless
`FoldCommand::Play` with `PlayTo(FoldEndpoint)`. Keep
`Step(FoldDirection)` and add `Pause`, `Resume`, and `Reverse`; all remain
values carried by the single `FoldCommandEvent`. Preserve the current journey
across pause and preserve raw position across reversal.

**Decided within A9.3:** Keep `FoldSequence` as a separate optional component
colocated with `Arrangement`, not an `Option<FoldSequence>` field inside it.
`Arrangement` identifies the controller; `FoldSequence` component presence
adds staged playback and retains independent lifecycle and change detection.

**Decided:** A4.2 no longer requires a complete arrangement to carry `Strip`,
`Accordion`, or `Coil`; directly authored hinge endpoints are valid.

**Still proposed:** Represent the three built-in chain recipes with one
optional sum component (`name TBD`). This consequence is not recorded as
decided yet.

#### Candidate comparison

| Candidate | Result |
|---|---|
| Keep mutable `Hinge::angle` and add disjoint arrangement, sequence, and external writers | Makes ownership explicit but adds an external-angle component and a third writer; it does not reduce the number of mechanisms. |
| Keep `FoldAngles` and add one common angle writer with an explicit progress-source sum type (`name TBD`) | Removes same-field writer overlap but retains two angle-bearing components and adds a source concept. It is viable, but not minimal. |
| Store the two endpoints on `Hinge` and calculate the current angle inside `hinge_to_pose()` | Removes the stored current angle, `FoldAngles`, both built-in angle-writing systems, and the angle-writing tween adapter. It adds no Hana-specific progress or angle-writer type and was selected under A9.1; A8 separately selects shared `bevy_kana::Angle` for the endpoint values. |

The A9.1 decision changes `Hinge` from mutable current state into authored
hinge calibration. The endpoint field names remain undecided:

```rust
pub struct Hinge {
    pub edge: Edge,
    pub unfolded_angle: Angle, // field name TBD
    pub folded_angle: Angle,   // field name TBD
}
```

`hinge_to_pose()` reads `Hinge`, the stage participation resolved from the
controller's `FoldSequence`, and the `FoldSequenceState` on the controller
targeted by `Member::root_entity`. It calculates the current angle without
storing it. The exact lookup API remains A9.3.1:

```rust
let fraction = sequence
    .stage_for(member_entity) // method name TBD
    .map(|stage| state.member_progress(stage, member_entity)) // method name TBD
    .unwrap_or(0.0);

let angle = hinge.unfolded_angle
    + (hinge.folded_angle - hinge.unfolded_angle) * fraction;
```

A `Hinge` on a `Member` omitted from every active `FoldStage` remains at its
unfolded endpoint. A fixed hinge, including `Strip`, uses equal endpoints.
Simultaneous Accordion or Coil playback places all controlled members in one
stage; staged playback derives several `FoldStage` values from the same recipe-
authored groups. `FoldCommandEvent` controls ordinary playback through
`FoldSequenceState`; A9.3 defers continuous external scalar control to a future
single-writer raw-progress adapter.

The resulting built-in data flow is:

```text
TopologyProvider + folding recipe
    -> AnchoredTo + Hinge { edge, unfolded angle, folded angle }

FoldCommand -> FoldSequenceState position
FoldStage { FoldGroup + timing } + FoldSequenceState position + Hinge
    -> hinge_to_pose() -> AnchorPose
    -> resolve_anchors() -> Transform
```

The core reduction is:

- Hana Valence angle-bearing components and adapters: `Hinge`, `FoldAngles`,
  and `HingeAngleLens` to `Hinge` alone, using the shared `bevy_kana::Angle`
  value;
- live progress locations: `Accordion::fold`, `Coil::fold`, and
  `FoldSequenceState::position()` to `FoldSequenceState::position()` alone;
- angle/pose stages: `drive_arrangement_hinges()`,
  `actuate_fold_hinges()`, and `hinge_to_pose()` to `hinge_to_pose()` alone;
- placement wrappers: `ArrangementPlacement` plus `MemberPlacement` to
  `MemberTopology::Anchored(Connection)`;
- new public types: one shared `bevy_kana::Angle`; no role-specific Hana
  Valence angle wrapper.

This reduction does not remove the transport, pivot-offset geometry,
`AnchorPose`, `AnchoredTo`, arrangement output suspension, or safe neutral-state
reconfiguration. A9.4 later folds the one-field `HingePivot` component into
`Hinge::pivot_offset`; the capability remains while the separate type does not.

#### Investigation decision status

- **A9.1 — Derived hinge angle:** Replace mutable `Hinge::angle` and
  `FoldAngles` with unfolded and folded endpoint fields on `Hinge`; merge angle
  actuation into `hinge_to_pose()` and remove the two built-in angle writers and
  `HingeAngleLens`. The endpoint field names remain undecided. **Status:
  decided.**
- **A9.3 — One high-level transport:** Remove `Accordion::fold` and
  `Coil::fold`; use `FoldSequence` for both one-stage simultaneous and staged
  progress. Add external seeking as a `FoldCommand` variant rather than another
  angle or progress component. **Status: proposed.**
- **A9.2 — Fold-recipe authoring:** The POC roles of `Strip`, `Accordion`, and
  `Coil` are not presumed to be the final model. A9.2 is decomposed into seven
  decisions covering transient recipes, ordered fold groups, the optional
  topology-provider capability, compatibility, Accordion semantics,
  Coil/Wrap clearance, and safe initial or runtime application. **Status:
  pending; A6 dependency resolved; decomposed into A9.2.1-A9.2.7.**
- **A9.4 — Further removals:** `MemberPlacement`, `ArrangementPlacement`,
  `FoldFromArrangement`, and the snapshot diagnostic family are removed. The
  approved model has neither a stored live angle for `MemberPlacement` nor a
  second membership graph for `FoldFromArrangement` to populate;
  `ArrangementPlacement` is subsumed by the connected payload of the A6
  result. Use the shared `bevy_kana::Angle` for the fixed connection angle,
  recipe offsets, and `Hinge` endpoints. Remove `FoldSystems::Actuate` with
  `actuate_fold_hinges()`; retain `FoldSystems::Advance` as the public
  controller-transport boundary before `hinge_to_pose()`. Remove the obsolete
  `FoldAngleInvalidReason`, `FoldAngleDiagnostic`, and `FoldAngleDiagnostics`
  family; F13 must review unified hinge and anchor diagnostics, including
  non-finite `Angle` failures and warning throttling. Remove the public
  `Hinge::rotation()` method without replacement so `hinge_to_pose()` remains
  the only fold-angle evaluation path. Remove `HingePivot::reference_angle` and
  define its offset at the unfolded `Hinge` endpoint, then merge the remaining
  offset into `Hinge::pivot_offset` and remove `HingePivot`. Declare
  `AnchorPose` as a required component of `Hinge`. Remove the unused
  `AnchorPoseLens` and the resulting empty tween integration. A9.2.2.1
  supersedes the proposed `SharedFoldStage` addition by reusing `FoldGroup`.
  A9.3.1.1 makes `FoldStage` a sequence-owned struct without adding a numeric
  member component. The local audit therefore removes eleven public types and
  adds none; the complete A9 concept count remains a closure-gate calculation.
  **Status: partial.**

## Current arrangement model

This is the canonical short account of the reviewed design. Keep it current as
the detailed decisions below change. Some approved types and methods described
here are planned API and are not implemented yet.

`ResolvedAnchorGeometry` is the existing per-entity catalog of local anchor
geometry. Its `points: HashMap<AnchorId, AnchorPoint>` maps each existing
`AnchorId` key to an attachment position and orientation; under corrected
A7.1, the planned `AnchorPoint` fields are `position: Position` and
`orientation: Orientation` from `bevy_kana`. Its `edges: Vec<Edge>` lists available local
axes. An `Edge` is an existing value type—not a component or relationship—and
its `start: AnchorId` and `end: AnchorId` refer to two entries in the same
`points` map. The list catalogs available axes; it does not connect entities or
establish arrangement membership.

`AnchoredTo` is the existing immutable Bevy relationship on a source entity
that records one point-to-point attachment. It contains the entity returned by
`AnchoredTo::target()`, `source_anchor: AnchorId`,
`target_anchor: AnchorId`, and `offset: Vec3`; it contains no `Edge`, `Hinge`,
or folding angle. Bevy maintains `AnchoredHere` on the target entity as the
collection of source entities whose `AnchoredTo::target()` points there.
Replacing `AnchoredTo` rather than mutating one field lets Bevy update that
reverse relationship correctly.

`resolve_anchors()` reads the source and target `ResolvedAnchorGeometry`, uses
the two `AnchorId` fields to select their `AnchorPoint` values, applies
`AnchoredTo::offset`, and writes the source entity's `Transform`. It also reads
the optional `AnchorPose` contribution.

`AnchorPose` is the existing resolver-input component containing
`rotation: Quat` and `translation: Vec3`. Animation systems construct or write
`AnchorPose`; `resolve_anchors()` reads it, combines it with the point-to-point
placement, and writes `Transform`. When the component is absent,
`resolve_anchors()` uses `AnchorPose::default()`, which contributes no
additional rotation or translation. This separates animation output from the
final anchored `Transform`: animation systems do not compete with
`resolve_anchors()` for the same component. `hinge_to_pose()` is one exact
`AnchorPose` writer; the next cohesion-overview step defines how `Hinge`
supplies its input.

Geometry adapters write `ResolvedAnchorGeometry`; for panels, the exact writer
is `write_panel_anchor_geometry()`. The current `TilingRule` selects a source
`Edge` and target `Edge`, maps each edge to an `AnchorId`, and uses those two
anchors to construct `AnchoredTo`. Only the source `Edge` is retained in
`Hinge` today. Under A9.1 and A9.4, `Hinge` retains that `Edge`, the unfolded and
folded endpoint `Angle` values (`field names TBD`), and `pivot_offset: Vec3`.
`Vec3::ZERO` means no pivot translation; a nonzero value locates an offset
physical pivot line in the source anchor's tangent frame. `hinge_to_pose()`
calls `Edge::axis()` against the source entity's `ResolvedAnchorGeometry`,
derives the current angle from the endpoints and selected fold progress, and
writes both rotation and pivot-compensating translation to `AnchorPose`.

Under A9.4, `Hinge` declares `AnchorPose` as a Bevy required component. Inserting
`Hinge` therefore inserts `AnchorPose::default()` only when the entity does not
already carry an explicit pose. Ordinary removal of `Hinge` leaves
`AnchorPose` available for another animation writer. `AnchoredTo`,
`ResolvedAnchorGeometry`, and `Member` are not required components of `Hinge`;
those capabilities may be absent or arrive later. `FoldStage` is not an ECS
component under A9.3.1.1, and a hinged member may be omitted from every stage.

`AnchorPoseLens` and `HingeAngleLens` are both removed under A9.4 and A9.1.
No shipped code uses `AnchorPoseLens`, and it cannot safely share an entity with
`Hinge` because `hinge_to_pose()` writes the complete `AnchorPose`. Direct
non-hinge animation remains supported by systems writing `AnchorPose`; future
F2, F3, or F11 adapters must be designed around their demonstrated capture,
coordination, or continuous-motion needs rather than preserving the unused
generic lens.

`Angle` is the planned shared `bevy_kana` value for a signed, unwrapped angular
displacement stored in radians. It is not an ECS component and does not
normalize its value. Explicit constructors and accessors preserve unit meaning;
implicit `f32` conversion and `Deref` are intentionally absent. Semantic APIs
use `bevy_kana::Orientation` and convert to `Quat` or `Rot2` only at math or
interoperability boundaries; dimensionless fold progress remains `f32`.

`TilingRule` is the trait that exists today. Concrete components such as
`QuadTiling` and the example's `TriangleTiling` use it to choose the two tile
edges and anchors that meet, plus the fixed starting `Angle` that aligns them.
The current arrangement code separately inserts `AnchoredTo` on each tile with
the preceding tile as `AnchoredTo::target()`, so it only constructs a linear
chain. A6 replaces that responsibility with `TopologyProvider`. It owns a
complete logical pattern and materializes the physical connection graph.

`Strip`, `Accordion`, and `Coil` are existing, separate ECS components. They do
not choose `Member` entities, `AnchoredTo::target()` values, or edges. Under
A9.1, they contribute the endpoint `Angle` values stored on the `Hinge` created from
the connection information: equal endpoints for `Strip`, alternating folded
directions for `Accordion`, and one folded direction for `Coil`. A9.2 decides
the exact endpoint-calculation contract. This folding responsibility remains
separate from `TopologyProvider` so that one connection
topology can be reused with different folding behavior.

The planned `Arrangement` component identifies a non-spatial assembly
controller entity. A4.2 no longer requires that controller to carry one of
`Strip`, `Accordion`, or `Coil`; an arrangement whose connection and `Hinge`
data is authored directly remains valid. A9.2 decides whether the three
built-in chain recipes become one optional sum component (`name TBD`). A4.3
retains the `commands.spawn_arrangement(...)` extension in principle, but its
parameters must be revised after the optional recipe representation is settled.

Every physical tile carries the planned `Member` relationship pointing to its
arrangement controller, and Bevy maintains `Members` on that controller.
`Member` / `Members` describes arrangement membership only. Physical tile
connections are separate `AnchoredTo` relationships and may form branching
acyclic trees. One or more `Member` entities may have no `AnchoredTo`.

`FoldGroup` is the confirmed ordinary authoring value containing one nonempty,
ordered group of `Member` entities. It stores neither topology data nor timing.
`Topology` resolves a supplied group to compatible `Connection` values for a
`FoldRecipe`; playback authoring combines the same value with timing to form a
`FoldStage`. Do not add a duplicate `MemberSelection` or `SharedFoldStage`.

`FoldSequence` is the existing authored timing and easing component and is
colocated with `Arrangement` under A9.3. `FoldSequenceState` remains the derived
runtime state on the controller. The currently implemented `FoldMember` /
`FoldMembers` relationship and `FoldFromArrangement` request are removed by
A9.5 and A9.4 respectively. Under A9.3.1.1, `FoldSequence` owns complete
`FoldStage` values and each stage owns one `FoldGroup`. Remove the public
numeric per-member stage component; any lookup cache remains private derived
implementation data. Successive stages are non-overlapping. Each participating
member resolves one `FoldTiming` value (`name TBD`) containing a stage-relative
start offset, duration, and easing; the stage ends at the latest member end.
Resolution uses a required sequence default, then whole-value
`Cascade<FoldTiming>` overrides at the stage and member-entry scopes.

## Review status

- Foundational API review items: 48
- Feature candidates: 15
- Mutual-exclusion decisions: 6, recorded in
  [`mutex_arrangements.md`](mutex_arrangements.md)
- Complete confirmed decisions: 35
- Reopened decision: A4.2; `Arrangement` identity and optional built-in recipe
  cardinality are settled, while the complete validity contract remains open
- Shared `bevy_kana` cascade extraction: implemented before widget work
- Active review: foundational arrangement-result review remains paused under
  the public API ergonomics reset
- A9.2 fold-recipe authoring: decomposed into A9.2.1-A9.2.7 plus A9.2.3.1;
  its A6 dependency is resolved
- F1, arrangement activation and suspension: paused until the
  foundational arrangement API is settled
- Scope selection: pending review completion
- Phased work plan: not yet created

Foundational API questions discovered during the review are decided here before
the feature candidates resume. Exploratory discussion is not a recorded
decision.

### Decision reporting

Do not report the current decision table while a proposed decision is still
awaiting acknowledgment. After the user acknowledges the proposal and the
decision is recorded, report every current review item in one table with
columns for ID, description, state, and decision. Leave the decision cell blank
for every undecided item, then advance to the next active item.

Every proposed decision includes a small code sample showing the new behavior.
Do not show the existing or before code unless the user requests it.

Before explaining an item, provide the context required to understand it
without relying on memory of the earlier conversation:

1. list only the confirmed decisions that the item depends on;
2. identify relevant types that already exist and distinguish them from names
   or types merely being proposed;
3. define the terms used in the explanation;
4. state the one question being decided now;
5. state which nearby questions are explicitly deferred to later review items.

Only then present the proposed structure and its new-behavior code sample.

Always use the exact type and method names that already exist or have been
confirmed by this review. Do not substitute an untyped shorthand such as
"parent" or "root" for a type-level statement. If a needed type or method name
has not been decided, mark it `(name TBD)` immediately in prose and with a
`name TBD` comment at its first use in a code sample. Describe an existing
relationship through its actual API—for example, the entity returned by
`AnchoredTo::target()`—rather than assigning it an unconfirmed role name.

### Foundational item boundaries

Only the item marked active is under review. The other items are parked so that
explaining one decision cannot silently expand its scope.

| ID | Question | Status |
|---|---|---|
| A1 | Should Bevy relationships maintain arrangement membership? | Decided: yes |
| A2.1 | What should the membership relationship target field be called? | Decided: `root_entity` |
| A2.2 | What should the membership relationship pair be called? | Decided: `Member` / `Members` |
| A3 | Is member order derived, and is `MemberIndex` still needed? | Decided: derive order; omit `MemberIndex` |
| A4.1 | Is the arrangement root a controller entity or the first physical tile? | Decided: controller entity |
| A4.2 | Which component set makes an arrangement root valid? | Reopened: `Arrangement` identity retained; no built-in folding recipe is required; remaining validity contract pending |
| A4.3 | What happens while an arrangement root is incomplete? | Decided in principle: explicit spawn helper and inert diagnosed incompleteness; helper signature pending A4.2 and A9.2 |
| A5 | Where is the boundary between connection rules and fold patterns? | Decided: connection structure and folding behavior remain separate |
| A6.0 | Is the proposed connection-rule responsibility better modeled as `TopologyProvider`, which materializes connections and may expose optional capabilities such as ordered fold groups? | Decided: yes; `TopologyProvider` owns a logical pattern, materializes its physical graph, and may expose recipe-compatible fold directions; regular triangle, quad, and hex sheet providers should support arbitrary dimensions and Accordion, Coil, and Wrap, with a strip as the one-dimensional degenerate case |
| A6.1 | Must `TopologyProvider` decide whether each `Member` has `AnchoredTo` and select its `AnchoredTo::target()`, instead of Hana always selecting the preceding `Member`? | Decided: yes; for every `Member`, the provider chooses either no `AnchoredTo` or one `AnchoredTo` whose target it selects; Hana never infers the preceding `Member` |
| A6.2 | How does `TopologyProvider` identify `Member` entities and refer to the target `Member` used by `AnchoredTo`? | Decided: each provider defines its own `Position`; the construction path maps those positions to actual `Member` entities and supplies that lookup during generation; Hana-facing results identify source and target with `Entity`, without a universal logical member ID |
| A6.3 | What result type (`name TBD`) represents either a `Member` without `AnchoredTo` or the data needed to add `AnchoredTo`? | Decided for now: transient `MemberTopology::{Unanchored, Anchored}` with `Anchored(Connection)`; `Connection` stores `member_entity`, `anchored_to`, `member_edge`, and `connection_angle` |
| A6.3.1 | What should the outer unanchored-or-anchored value and its variants be called? | Decided for now: authoring-only `MemberTopology::{Unanchored, Anchored}`; the value is consumed during materialization and is not an ECS component |
| A6.3.2 | What should the connected payload be called, and which fields does it expose? | Decided for now: authoring-only `Connection { member_entity, anchored_to, member_edge, connection_angle }`; refine if implementation friction reveals a better boundary |
| A6.4 | What methods and failure contract does `TopologyProvider` expose? | Decided through A6.4.1-A6.4.3: generate pure whole-topology values, validate a complete acyclic forest, queue validated replacements through ordinary `Commands`, return `TopologyError` immediately, and diagnose failures caused only by later ECS state |
| A6.4.1 | What generation method does a topology provider expose, and what authoring values does it yield? | Decided: outside the Bevy `World`, enumerate provider-specific positions and generate `Vec<MemberTopology>` from a supplied position-to-`Entity` lookup; the owned vector contains one explicit entry per associated member and its order implies no topology or fold order |
| A6.4.2 | Which structural invariants are validated over the complete generated topology, and must application commit atomically? | Decided through A6.4.2.1-A6.4.2.3: distinguish construction guarantees from validation, require a geometry-independent acyclic forest, and queue one completely validated replacement through ordinary `Commands` without requiring exclusive `World` access |
| A6.4.2.1 | Which facts does topology-driven spawning guarantee by construction, and which generated values still require defensive validation on each construction path? | Decided: topology-driven spawning guarantees one entity, factory call, `Member` relationship, and binding per logical position; both paths validate generated-output coverage and references, while the existing-member path also validates current entity existence and membership |
| A6.4.2.2 | Which structural invariants must every complete generated topology satisfy? | Decided: accept an empty topology, multiple unanchored members, disconnected trees, branching, repeated targets, and finite multi-turn angles; reject self-targets, cycles, non-finite offsets or angles, and a `member_edge` with identical anchor identifiers; defer geometry-dependent checks |
| A6.4.2.3 | Must validation and topology materialization use an all-or-nothing commit boundary? | Decided: use a validated full replacement through ordinary `Commands`; an invalid candidate queues no topology writes, while a valid candidate queues the complete removal-and-replacement set; do not require exclusive `World` access or promise rollback against unrelated mutation before command application |
| A6.4.3 | How are deferred-command failures, unavailable runtime inputs, diagnostics, and retry reported? | Decided: return generation and structural-validation failures immediately as `Result<_, TopologyError>`; report failures caused by later ECS state against the `Arrangement` controller through the F13 diagnostic channel (`name TBD`); entity lifetime and despawning remain entirely application-owned |
| A6.5 | How are `TopologyProvider` values invoked, and what provider state persists or reruns? | Decided through A6.5.1-A6.5.2: support topology-driven spawning and existing-member binding, return caller-owned `Topology`, and persist no provider lifecycle state automatically in Hana |
| A6.5.1 | Does the normal construction path let a logical topology drive `Member` spawning through an application-supplied member factory while retaining a separate existing-member path? | Decided: yes; the normal helper spawns one `Member` per `Position` and supplies the resulting entity lookup to topology generation, while `apply_topology()` accepts a caller-defined lookup for existing members; both return `Topology` for compatible folding recipes |
| A6.5.2 | Which provider or topology-authoring state persists after topology materialization, and where? | Decided: Hana automatically persists neither the concrete provider nor `Topology` in ECS; return `Topology` as an ordinary Rust value that callers may use once and drop or retain in application-owned storage for later recipe re-authoring; reuse goes through ordinary validation and a caller explicitly creates any replacement value |
| A6.6 | What should the topology-provider trait and its related concepts be called? | Decided: `TopologyProvider`, `Position`, `positions()`, `generate_topology()`, caller-owned `Topology`, `apply_topology()`, and `TopologyError`; retain the previously approved `spawn_arrangement()`; no separate bound-provider type |
| A6.7 | Should `TopologyProvider::Position` and `positions()` be renamed now that `bevy_kana::Position` is the shared spatial-point type? | Pending; the provider value is a logical member key, not necessarily a spatial position |
| A7 | What do `AnchorId`, `AnchorPoint`, `Edge`, and `AnchoredTo` mean and which names should change? | Partial: `Edge` remains the ordered pair of local anchor keys used to derive an axis; `AnchoredTo` remains the immutable point-to-point relationship; retain `AnchorPoint` with shared `bevy_kana::Position` and `Orientation`; key vocabulary, orientation validation, and the remaining semantic-math audit remain A7.3-A7.5 |
| A7.1 | How is an anchor's local position and orientation represented on `AnchorPoint`? | Corrected and decided: `position: bevy_kana::Position` and `orientation: bevy_kana::Orientation`; both are non-optional and default to the origin and identity; remove the redundant proposed `AnchorOrientation` type |
| A7.2 | Should the `AnchorPoint` type itself be renamed now that it stores position and orientation? | Decided: retain `AnchorPoint`; it names the local attachment location, while `Position` and `Orientation` name its spatial values and `AnchorPose` remains the separate runtime animation input |
| A7.3 | What should `AnchorId` be called when it is the provider-authored key used to select an `AnchorPoint`? | Pending |
| A7.4 | Where should invalid `bevy_kana::Orientation` values used by anchor geometry be rejected? | Pending |
| A7.5 | Which remaining raw spatial fields in the anchor and hinge APIs should use existing `bevy_kana` semantic newtypes? | Pending; apply the confirmed shared-semantic-math rule consistently |
| A8 | Are `ArrangementPlacement`, `MemberPlacement`, and raw angle values still needed? | Decided: remove both placement types; use shared `bevy_kana::Angle` for the A6 connected payload's fixed connection angle, recipe offsets, and `Hinge` endpoints; A9.4 later supersedes the planned `HingePivot::reference_angle` conversion by merging the remaining pivot offset into `Hinge` and removing `HingePivot` |
| A9 | How do folding policies, angle ownership, transport timing, and staged `FoldSequence` ordering form one coherent fold pipeline? | Pending; split into A9.1-A9.5 |
| A9.1 | Should `Hinge` store unfolded and folded endpoints while `hinge_to_pose()` derives the current angle instead of storing `Hinge::angle`? | Decided: yes; remove `FoldAngles` and stored current-angle state; endpoint field names remain pending |
| A9.2 | How do folding recipes obtain compatible topology, materialize complete `Hinge` data, and support both initial authoring and runtime re-authoring? | Pending; decomposed into A9.2.1-A9.2.7; A6 dependency resolved |
| A9.2.1 | Are folding recipes transient authoring operations whose durable result is complete `Hinge` data rather than a recipe ECS component? | Pending |
| A9.2.2 | What authoring value represents ordered groups of `Member` entities reusable by topology-aware recipes and coordinated playback? | Decided through A9.2.2.1: `FoldGroup` |
| A9.2.2.1 | Should one reusable `FoldGroup` hold the nonempty ordered `Member` selection while recipe interpretation and coordinated playback remain separate operations over it? | Decided: yes; `FoldGroup` stores ordered `Member` entities and no `Connection`, recipe, timing, or ECS state; `Topology` resolves it for compatible `FoldRecipe` use; `FoldStage` adds coordinated-playback semantics; remove proposed `MemberSelection` and `SharedFoldStage`; A9.3.1.1 makes each sequence-owned `FoldStage` own one `FoldGroup` and removes the public numeric member component |
| A9.2.3 | What optional topology-provider capability (`name TBD`) returns those fold groups, and what must remain available for runtime re-authoring? | Pending |
| A9.2.3.1 | What structure lets a `TopologyProvider` expose alternative `FoldGroups` without hard-coding a domain taxonomy, including whatever identity or selection information the domain proves necessary? | |
| A9.2.4 | Which compatibility invariants and failures govern applying a folding recipe to returned fold groups? | Pending |
| A9.2.5 | What geometry-independent behavior defines Accordion across strips, grids, sheets, and other compatible topologies? | Pending |
| A9.2.5.1 | Which implicit `FoldGroups` position determines the built-in Accordion recipe's alternating physical fold sense? | Decided: alternate by outer `FoldGroup` position only; every member-resolved connection in one group shares the same physical fold sense, while inner member position remains available to other recipes; the iterator API supports functional `enumerate()` / `flat_map()` / `collect()` authoring |
| A9.2.5.2 | Should one signed `Angle` on the transient Accordion recipe encode both the first group's physical fold sense and the alternating fold magnitude? | Decided: yes; negate the configured `Angle` for each following group, negating the recipe value reverses the pattern, and live progress remains exclusively in `FoldSequence` / `FoldSequenceState`; field and constructor names remain pending |
| A9.2.5.2.1 | Should `Accordion::default()` use a signed half-turn offset while explicit construction accepts any finite signed `Angle`? | Decided: yes; default to `+pi` radians for the convenient ideal flat-fold, accept any finite signed `Angle` explicitly, and retain alternating Accordion behavior rather than treating the half-turn as a Wrap rule; exact `Angle` API spelling remains pending |
| A9.2.5.3 | Should `Connection::connection_angle` supply one complete `Hinge` endpoint while Accordion adds its alternating signed offset to produce the other endpoint? | |
| A9.2.6 | What distinct behaviors define Coil and Wrap, and what clearance or pivot calibration must a provider guarantee? | Pending |
| A9.2.7 | How do initial recipe application and runtime replacement commit atomically and interact with neutral-state travel? | Pending; coordinated with F9 and A10 |
| A9.3 | How should `FoldSequence`, `FoldSequenceState`, and `FoldStage` jointly represent simultaneous, staged, and externally controlled progress, and can any be combined or removed? | Partial; keep `FoldSequence` as a separate optional component colocated with `Arrangement`; it owns complete `FoldStage` values, each owning one `FoldGroup`; remove the public numeric per-member stage component; successive stages do not overlap; each resolved member timing contains a stage-relative offset, duration, and easing; resolve timing through member override, stage override, then the required sequence default using whole-value `Cascade<FoldTiming>`; keep raw progress and canonical transport in `FoldSequenceState`; use `FoldCommandEvent` carrying `Step(FoldDirection)`, `PlayTo(FoldEndpoint)`, `Pause`, `Resume`, or `Reverse`; continuous external driving remains pending |
| A9.3.1 | What authored stage and timing representation lets the same recipe-authored `Hinge` endpoints play simultaneously, strictly sequentially, or with overlapping stagger or wave timing? | Decided through A9.3.1.1-A9.3.1.2; `FoldSequence` owns non-overlapping `FoldStage` values over `FoldGroup` selections, while whole-value timing cascades produce simultaneous, staggered, or wave motion within each stage |
| A9.3.1.1 | Does `FoldSequence` own complete `FoldStage` values, each owning one `FoldGroup`, instead of distributing numeric stage components across `Member` entities? | Decided: yes; `FoldSequence` is the authored schedule and owns `Vec<FoldStage>`; each `FoldStage` owns one `FoldGroup`; remove the public numeric per-member stage component; a private derived lookup or cache may be added only if implementation evidence requires it |
| A9.3.1.2 | How does one `FoldStage` represent simultaneous, staggered, wave, duration, and easing timing for the members in its `FoldGroup`? | Decided through A9.3.1.2.1-A9.3.1.2.3; each non-overlapping stage resolves one complete timing per member through whole-value sequence, stage, and member-entry scopes |
| A9.3.1.2.1 | Are successive `FoldStage` values non-overlapping sequence steps, with every overlapping stagger or wave represented as intra-stage member timing? | Decided: yes; a stage boundary is a possible `Step(FoldDirection)` stop; overlapping motion belongs inside one stage; functionally combine any number of recipe-authored `FoldGroup` values into the playback group for that stage without changing the originals; add overlapping stages only if a demonstrated example cannot fit this model |
| A9.3.1.2.2 | What value represents one member's start, duration, and easing within its `FoldStage`? | Decided: one complete ordinary value (`FoldTiming`, name TBD) contains a stage-relative `start_offset: Duration`, `duration: Duration`, and `easing: EaseFunction`; simultaneous members share zero offset and timing, stagger and wave vary offsets, and the stage ends at the maximum member offset plus duration; exact names and validation policy remain pending |
| A9.3.1.2.3 | How do sequence defaults, stage timing, and member timing cascade, and which scopes can reuse the same timing value type? | Decided: `FoldSequence` owns one complete default `FoldTiming`; `FoldStage` and each stage-owned member entry use whole-value `Cascade<FoldTiming>` inheritance or replacement; precedence is member, stage, sequence; no field-level patches, per-scope timing types, or per-member ECS timing components |
| A9.3.1.2.3.1 | Where should the generic authoring-only `Cascade<T>` type live so `hana_valence` and `hana_diegetic` can share it without reversing their dependency direction? | Decided and implemented: `bevy_kana` owns `Cascade<T>` plus the shared ECS engine, including explicit relationships, authored and resolved generic storage, lifecycle handling, commands, propagation, and scheduling; `hana_diegetic` retains its domain attributes, explicit relationship choices, typed verbs and readers, and consumers; `Option<T>` conversions are absent; `Cascade<T>` is public only from its owning crate and is not re-exported or exposed through `hana_diegetic` or `hana_valence` public APIs; completed before widget work |
| A9.3.2 | What functional API transforms, combines, or subdivides timing-free `FoldGroup` values into timed `FoldStage` values? | |
| A9.4 | Which existing fold types, fields, and systems can the A9 pipeline combine or remove, and what is its net concept count? | Partial: all listed removals, retained plugin/system/builder/error types, required `AnchorPose`, and F13 diagnostic ownership remain decided; the proposed `SharedFoldStage` addition is removed in favor of `FoldGroup`; the local simplification audit removes eleven public types and adds none, while the complete A9 public-concept count remains a closure-gate calculation after A9.2 and A9.3 finish |
| A9.5 | Is sequence membership distinct from arrangement membership, and if so, what should `FoldMember` / `FoldMembers` be called? | Decided: use only `Member` / `Members`; remove `FoldMember` / `FoldMembers`; `FoldSequence` owns its `FoldStage` values and their `FoldGroup` selections, so no public per-member stage component or second membership relationship remains |
| A10 | How are connection topology and fold sequencing validated together? | Pending |

### A1. Arrangement membership storage

**Current implementation:** A member carries `Member { arrangement }`.
`ArrangementMembers(Vec<Entity>)` is maintained separately by custom add/remove
observers and index-assignment systems.

**Decision:** Represent arrangement membership as a Bevy relationship. Bevy
maintains the reverse collection when membership is inserted, removed,
retargeted, or discarded. Remove Hana's custom reverse-list maintenance.

The relationship pair is named `Member` / `Members` under A2.2. A3 derives
member position from relationship order and omits `MemberIndex`.

**Status:** Decided.

### A2.1. Membership relationship target field

**Decision:** Name the `Entity` field that targets the arrangement root
`root_entity`.

```rust
pub struct Member {
    #[relationship]
    root_entity: Entity,
}
```

The relationship types are named `Member` and `Members` under A2.2.

**Status:** Decided.

### A2.2. Membership relationship type names

**Decision:** Name the relationship source `Member` and its Bevy-maintained
relationship target `Members`.

```rust
#[relationship(relationship_target = Members)]
pub struct Member {
    #[relationship]
    root_entity: Entity,
}

#[relationship_target(relationship = Member)]
pub struct Members(Vec<Entity>);
```

**Status:** Decided.

### A3. Member order and `MemberIndex`

**Decision:** Derive current member position by enumerating the ordered
Bevy-maintained `Members` relationship target. Do not store a `MemberIndex`
component. Reconsider a stored index only when a concrete future feature
requires semantics that cannot be derived from relationship order.

```rust
for (position, member_entity) in members.iter().enumerate() {
    place_member(member_entity, position);
}
```

Removing a member allows later positions to shift naturally while preserving
the remaining entities' relative order.

**Status:** Decided.

### A4.1. Arrangement identity and physical connection roots

**Decision:** The arrangement root is a controller entity, not the first
physical tile. Every physical tile, including every root of the physical
connection graph, carries `Member` targeting that controller. `Members`
therefore describes arrangement membership only; its order does not choose a
tile's physical predecessor.

Physical connections are represented separately by `AnchoredTo`. They may
branch and may have one or more physical connection roots. A connected net
normally has one such root, while a compound can place multiple disconnected
connection trees under one arrangement controller.

```rust
let arrangement_root = commands
    .spawn((Arrangement, connection_rule, Accordion::default()))
    .id();

for tile in [center, north, south, east, west, lid] {
    commands
        .entity(tile)
        .insert(Member::of(arrangement_root));
}

// Physical topology is independent of membership order.
commands.entity(north).insert(AnchoredTo::new(
    center,
    north_inner_anchor,
    center_north_anchor,
));
commands.entity(south).insert(AnchoredTo::new(
    center,
    south_inner_anchor,
    center_south_anchor,
));
commands.entity(lid).insert(AnchoredTo::new(
    north,
    lid_inner_anchor,
    north_outer_anchor,
));
```

This model can represent ordinary, concave, star, and compound polyhedra when
their geometry providers supply the required face anchors and their connection
topology providers supply the explicit connection trees. It does not yet represent every
model in the [Polyhedra.net catalog](https://www.polyhedra.net/en/): a moving
closed ring such as a kaleidocycle requires the future closed-loop capability
tracked by F14. `AnchoredTo` remains acyclic; a closure constraint must not be
implemented by creating a relationship cycle.

A6 decides how `TopologyProvider` expresses branches and multiple unanchored
`Member` entities. A9 will decide authored fold stages,
including simultaneous folds. A10 will validate their interaction: connection
dependency order remains separate from animation order, all sequenced entities
belong to the arrangement, and the connection graph can be resolved regardless
of the chosen fold stages.

**Status:** Decided.

### A4.2. Arrangement identity and validity

**Retained decision:** Add an `Arrangement` marker component that identifies an
assembly controller independently of its current members and optional
capabilities.

**Correction decided under A9:** A valid `Arrangement` does not require
`Strip`, `Accordion`, or `Coil`. Those built-in endpoint-generating recipes are
optional, and directly authored `AnchoredTo` and `Hinge` data is equally valid.
The three recipes therefore do not define the exhaustive kinds of arrangement.

```rust
#[derive(Component, Default)]
pub struct Arrangement;

let root_entity = commands.spawn(Arrangement).id();
```

`Members` is Bevy-maintained derived storage rather than an authored validity
component. A complete arrangement may have zero members and therefore no
`Members` component yet. Members can be attached later without coupling their
lifecycle to creation of the controller.

The controller does not require geometry, anchors, spatial transforms,
`AnchoredTo`, or `Hinge`; those components belong to physical members. Whether
a selected `TopologyProvider` has enough input to materialize its graph remains
part of the open validity contract, but a provider is not
required when connections are authored directly. `FoldSequence`, when present,
lives on this same controller as a separate optional ECS capability rather than
as an `Arrangement` field or on a second controller entity.

A4.3 provides explicit construction without required-component defaults and
defines how incomplete controllers behave.

**Status:** Reopened. `Arrangement` identity and the absence of a required
built-in folding recipe are decided; the remaining validity contract is
pending A9.2.

### A4.3. Construction and incomplete-controller behavior

The `spawn_arrangement` helper and inert diagnostic behavior remain approved,
but the helper's previously proposed topology-provider and folding-policy
parameters are no longer final because A4.2 has reopened.

**Decision:** Do not attach default connection or folding policies as required
components of `Arrangement`. There is no shape-agnostic default connection
topology, and silently choosing `Strip`, `Accordion`, or `Coil` would hide author
intent without preventing later invalid component edits.

Provide `commands.spawn_arrangement(...)` through a `Commands` extension trait
as the guided construction path. Its final parameters are pending: it must
support a controller whose connections and hinge endpoints are authored
directly, as well as one using optional connection and endpoint recipes. A6
supplies `TopologyProvider`; A9.2 will supply the built-in endpoint-recipe
representation.

```rust
let root_entity = commands.spawn_arrangement(/* parameters TBD */);
```

Direct ECS composition, scene loading, and later component editing remain
supported. While a selected authoring capability lacks the inputs it needs or
the resolved graph is invalid:

- preserve `Arrangement`, `Member`, and `Members`;
- perform no arrangement placement or hinge writes;
- preserve connections and hinges already emitted instead of deleting
  components whose ownership may be unclear;
- report the missing or conflicting responsibility without repeated log spam;
- never panic, invent a default, or choose one policy arbitrarily.

When the required data becomes valid again, request member reconciliation and
resume automatically. A9.2 decides whether folding recipes persist as ECS
state at all and therefore whether a folding-policy exclusivity problem
remains; F1 owns intentional suspension, and F12 owns the reconciliation
mechanism.

When `FoldSequence` is present, its `initial: FoldEndpoint` selects the initial
conformation as members become resolvable. The source of the initial hinge
endpoint for an arrangement with two distinct `Hinge` endpoints but no
`FoldSequence` remains part of the pending constructor and validity contract;
`Arrangement` identity alone must not choose one silently.

**Status:** Decided in principle; constructor signature pending A4.2 and A9.2.

### A5. Connection structure and folding behavior

**Decision:** Keep connection structure and folding behavior separate because
different connection topologies can work with different folding behaviors.

`TopologyProvider` decides whether
each `Member` has `AnchoredTo`, selects its `AnchoredTo::target()`, and owns the
paired tile edges and anchors plus the fixed connection `Angle` needed to align
a connection. The original POC represented folding behavior through `Strip`,
`Accordion`, and `Coil` ECS components. A9.2 has reopened that representation;
the durable A5 decision is only that topology construction and folding behavior
remain separate responsibilities.

```rust
let root_entity = commands.spawn_arrangement(
    triangle_connections,
    Accordion::default(),
);
```

The same `triangle_connections` component can be reused with different folding
behavior without redefining membership or physical connections. `FoldSequence`
remains optional orchestration; its ownership and membership model are decided
under A9.

**Status:** Decided.

### A6.0. Topology-provider classification

A6 is the prerequisite for A9.2.

**Decision:** Model this responsibility with `TopologyProvider`, not as a
connection rule. A provider owns a complete logical pattern and materializes
the physical graph used by Hana. It may
expose structural capabilities for recipe-compatible fold directions. Each
direction can provide ordered, consistently oriented connection groups and
additional calibration required by compatible folding recipes, while topology
construction and folding behavior remain separate under A5.

A provider may know logical adjacencies that do not each become an
`AnchoredTo` relationship. The materialized `AnchoredTo` relationships remain
the acyclic, one-target-per-`Member` graph used for transform evaluation; the
provider is responsible for guaranteeing that its generated fold groups remain
coherent with the complete logical pattern.

Regular triangle-sheet, quad-sheet, and hex-sheet providers (exact public type
names TBD) should derive recipe-compatible directions for arbitrary finite
dimensions and support Accordion, Coil, and Wrap behavior. A strip is the
one-dimensional degenerate case in which each connection group contains one
connection. Wrap may require more provider-supplied geometry or calibration
than Accordion or Coil, but that requirement does not make regular sheets
inherently bespoke.

```rust
let sheet = TriangleSheet::new(8, 12); // provider name TBD
let groups = sheet.fold_groups(TriangleDirection::A)?; // names TBD

commands.apply_fold(
    arrangement,
    Wrap::new(groups), // recipe representation TBD under A9.2
)?;
```

A9.2 still decides the fold-group value and capability types, compatibility
checks, precise Accordion/Coil/Wrap behavior, wrap calibration, and application
lifecycle.

**Closure gate:** A6 closes only when its exact types and methods can materialize
a branching graph with multiple unanchored `Member` entities without
mentioning Accordion, Coil, Wrap, or `FoldSequence`, and direct `AnchoredTo` /
`Hinge` authoring remains valid without a provider.

**Status:** Decided.

### A6.1. `AnchoredTo` presence and target ownership

**Decision:** For every `Member`, `TopologyProvider` decides
whether the physical member has no `AnchoredTo` or has one `AnchoredTo`, and it
selects the entity returned by `AnchoredTo::target()`. A `Member` without
`AnchoredTo` is unanchored in the physical graph. Hana validates and
materializes the provider's graph but never infers the preceding `Member` as a
default target.

This ownership supports multiple unanchored `Member` entities, branching
connection trees, sheets, and polyhedral nets while keeping `Member` / `Members`
order separate from physical topology. Direct authoring may produce the same
ordinary components without using a topology provider. `AnchoredTo` remains
acyclic; F14 separately owns future closed-loop constraints.

```rust
commands.apply_topology(arrangement, topology)?;

// Result selected by the topology provider:
// center: no AnchoredTo
// north:  AnchoredTo::target() == center
// south:  AnchoredTo::target() == center
// lid:    AnchoredTo::target() == north
```

`TopologyProvider::Position` and the supplied entity lookup identify these
`Member` entities. `MemberTopology` represents the generated result, and
`TopologyError` reports immediate validation failures.

**Status:** Decided.

### A6.2. Provider-owned pattern-position mapping

**Decision:** Each `TopologyProvider` defines its own `Position` type for
positions in its logical pattern. The construction path creates or accepts the
association from those positions to the actual `Entity` values carrying
`Member`, then supplies that lookup to the provider while it generates
Hana-facing connection results. Both the source `Member` and the entity
returned by `AnchoredTo::target()` are identified with `Entity`.

Triangle, quad, hex, and future providers may use their own coordinates or keys
internally. Hana does not introduce a universal `TileId`, `CellId`, logical
member component, or coordinate-to-entity registry. The normal construction
helper creates the entities and its temporary lookup; the existing-member path
accepts an application-supplied association. F12 owns regeneration when
dynamic membership changes.

```rust
let topology = commands.spawn_arrangement(
    TriangleSheet::new(4, 6),
    |position| triangle_panel_bundle(position),
)?;

// The helper maps each TriangleSheet position to the Entity it spawned and
// supplies that lookup while TriangleSheet generates MemberTopology values.
```

`MemberTopology` and `Connection` represent the generated values;
`TopologyError` reports incomplete or invalid entity associations.

**Status:** Decided.

### A6.3. Unanchored-or-anchored topology result

The review has already decided that a topology provider returns a named sum
value rather than using `Option` to distinguish an unanchored `Member` from an
anchored `Member`. The remaining decisions are split as follows:

1. **A6.3.1 — Outer value:** Name the unanchored-or-anchored authoring value and
   its two variants.
2. **A6.3.2 — Connected payload:** Name and define the value containing the
   source `Member` entity, `AnchoredTo`, member-local `Edge`, and fixed
   connection `Angle`.

### A6.3.1. Authoring-only member-topology variants

**Decision for now:** Use the authoring-only sum type `MemberTopology` with
`Unanchored` and `Anchored` variants. It is not an ECS component. Topology
materialization consumes it, inserts or omits `AnchoredTo`, passes its connected
data through any remaining authoring work, and then drops it.

```rust
pub enum MemberTopology {
    Unanchored {
        member_entity: Entity,
    },
    Anchored(Connection), // payload decided under A6.3.2
}
```

`Unanchored` means the `Member` receives no `AnchoredTo`; `Anchored` means the
connected payload supplies the `AnchoredTo` to insert. Neither variant uses
"root": the arrangement root remains only the controller entity referenced by
`Member::root_entity`. A later topology change creates a new transient set of
`MemberTopology` values. A9.2.3 separately decides which provider information
must remain available to regenerate them.

**Status:** Decided for now; the name may be revisited if later API composition
reveals a clearer replacement.

### A6.3.2. Connected topology payload

**Decision for now:** Name the authoring-only connected payload `Connection`.
It stores the source `Member` entity, the complete `AnchoredTo` relationship to
insert, the ordered member-local `Edge`, and the fixed connection `Angle`.

```rust
pub struct Connection {
    pub member_entity: Entity,
    pub anchored_to: AnchoredTo,
    pub member_edge: Edge,
    pub connection_angle: Angle,
}

let topology = MemberTopology::Anchored(Connection {
    member_entity: lid,
    anchored_to: AnchoredTo::new(
        north,
        lid_inner_anchor,
        north_outer_anchor,
    ),
    member_edge: lid_hinge_edge,
    connection_angle: Angle::ZERO,
});
```

`AnchoredTo` already contains the target entity, source and target anchors, and
connection offset, so `Connection` does not duplicate those fields. Folded and
unfolded endpoint angles and `Hinge::pivot_offset` remain folding-recipe output
under A9.2. `Connection` may be grouped during authoring, but the value itself
is dropped after its ordinary ECS outputs are committed.

This structure is accepted as a practical starting point rather than an
irreversible API commitment. Implementation and example friction should drive
any later simplification or renaming.

**Status:** Decided for now.

**A6.3 status:** Decided for now.

### A6.4. Topology-provider generation and failure contract

A6.4 is split so the producer method can be understood independently from the
validation transaction:

1. **A6.4.1 — Generation:** Decide the provider method and the
   `MemberTopology` values it yields.
2. **A6.4.2 — Structural validation:** Decide the complete-candidate
   validation contract through three subitems:
   - **A6.4.2.1 — Construction guarantees:** Separate facts guaranteed by
     topology-driven spawning from values that still require defensive
     validation.
   - **A6.4.2.2 — Graph invariants:** Decide the structural requirements for
     every complete generated topology.
   - **A6.4.2.3 — Atomic commit:** Decide whether validation and materialization
     use an all-or-nothing boundary.
3. **A6.4.3 — Failure delivery:** Decide how deferred-command errors,
   unavailable runtime inputs, diagnostics, and retry are represented.

The explicit application command and whether providers persist as ECS state
remain separate under A6.5.

### A6.4.1. Pure whole-topology generation

**Decision:** A `TopologyProvider` enumerates its provider-specific
logical positions and generates its complete candidate outside the Bevy
`World`. Generation receives the position-to-`Entity` lookup created by the
selected construction path:

```rust
pub trait TopologyProvider {
    type Position;

    fn positions(&self) -> impl Iterator<Item = Self::Position>;

    fn generate_topology(
        &self,
        entity_for: impl Fn(&Self::Position) -> Entity,
    ) -> Vec<MemberTopology>;
}

let topology = sheet.generate_topology(|position| entities[position]);
commands.apply_topology(arrangement, topology)?;
```

The lookup is an ordinary Rust value, not access to ECS. The provider does not
receive `World`, `Commands`, `Members`, or the arrangement controller. `&self`
permits later regeneration. Passing the lookup directly avoids a second
provider type whose only purpose would be to hold entity bindings. The owned
`Vec<MemberTopology>` contains one explicit entry for every provider-associated
member, including `Unanchored` entries, so A6.4.2 can inspect the complete
candidate before any ECS mutation.

Vector order is deterministic authoring and diagnostic order only. It does not
define `Members` order, connection dependency order, or fold order. A6.4.2 may
lift the return to a named `Result` if provider-local failure requires it
without reopening the pure, outside-`World`, whole-result boundary.

**Status:** Decided.

### A6.4.2.1. Construction guarantees versus defensive validation

**Decision:** The topology-driven construction path guarantees these facts by
construction:

- it calls the application bundle factory once per logical position;
- it spawns exactly one entity from each factory result;
- it inserts `Member::of(arrangement)` on every spawned entity; and
- it creates a one-to-one logical-position-to-`Entity` association.

Both construction paths defensively validate that the provider-authored
`Vec<MemberTopology>` corresponds exactly to the bound entity set: no bound
entity is omitted or represented more than once, and every entity referenced by
the generated topology belongs to the binding. This does not revalidate member
spawning; it validates the provider's generated connection description before
that description becomes `AnchoredTo`.

The existing-member path additionally verifies at application time that every
bound entity still exists and carries `Member` targeting the intended
`Arrangement` controller. A6.4.2.2 defines the remaining graph invariants, and
A6.4.3 defines how deferred-state failures are delivered.

```rust
let topology = commands.spawn_arrangement(
    TriangleSheet::new(4, 6), // provider type name TBD
    |cell| triangle_panel_bundle(cell),
)?;

// Internally, the helper supplies its new position-to-Entity lookup to the
// provider, validates the complete candidate, and only then queues its writes.
```

**Status:** Decided.

### A6.4.2.2. Complete generated-topology invariants

**Decision:** After the A6.4.2.1 coverage checks, accept a complete generated
topology when its `member_entity -> AnchoredTo::target()` references form a
forest. Following `AnchoredTo::target()` from any
`MemberTopology::Anchored(Connection)` must eventually reach a
`MemberTopology::Unanchored` member.

The contract accepts an empty topology for an arrangement with no members,
multiple unanchored members, multiple disconnected connection trees,
branching, and repeated targets. It does not require one global unanchored
member or one connected graph.

Reject self-targets and longer cycles. Also reject geometry-independent invalid
payload values: a non-finite `AnchoredTo::offset`, a non-finite
`connection_angle`, or a `member_edge` whose `start` and `end` are the same
`AnchorId`. Finite `Angle` values remain signed and unwrapped; multi-turn values
such as 720 degrees are valid.

```rust
let topology = vec![
    MemberTopology::Unanchored {
        member_entity: base,
    },
    MemberTopology::Anchored(Connection {
        member_entity: wall,
        anchored_to: AnchoredTo::new(
            base,
            AnchorId::EdgeMid(0),
            AnchorId::EdgeMid(2),
        ),
        member_edge: Edge {
            start: AnchorId::Vertex(0),
            end: AnchorId::Vertex(1),
        },
        connection_angle: Angle::ZERO,
    }),
];

validate_topology(&topology)?; // function name TBD
```

Whether referenced anchors exist, differently named edge endpoints are
physically separated, orientations are compatible, and required geometry and
transforms are ready depends on ECS geometry and remains A7 and A6.4.3.
Folding-recipe, final `Hinge`, and `FoldSequence` compatibility remains A9.2
and A10. Explicit closed-loop constraints remain F14 and do not become
`AnchoredTo` cycles.

**Status:** Decided.

### A6.4.2.3. Validated full replacement

**Decision:** Validate the complete topology before queuing any topology
writes, then use ordinary `Commands` to queue the complete replacement. A
known-invalid candidate queues no changes. A valid candidate queues removal of
`AnchoredTo` for every `MemberTopology::Unanchored` entry and insertion or
replacement of the complete `AnchoredTo` for every
`MemberTopology::Anchored(Connection)` entry.

```rust
let plan = validate_topology(&members, candidate)?; // names TBD
plan.queue(&mut commands);
```

The public application API remains a `Commands` extension and exposes neither
`World` nor `&mut World`:

```rust
commands.apply_topology(arrangement, candidate)?;
```

Do not require an exclusive custom command or claim database-style rollback.
If unrelated code despawns or retargets a member after validation but before
the queued writes run, A6.4.3 handles the resulting failure or retry. The
guarantee is a completely validated replacement plan, not isolation from
arbitrary concurrent ECS mutation.

**Status:** Decided.

**A6.4.2 status:** Decided.

### A6.4.3. Failure delivery, readiness, and retry

F13 retains ownership of the final diagnostic storage, history, and warning
deduplication policy.

**Decision:** Deliver a failure according to when Hana can know it. Generation,
binding, coverage, graph, and geometry-independent payload failures are known
before topology writes are queued and return immediately as
`Result<_, TopologyError>`.

```rust
let authored = commands.spawn_arrangement(
    TriangleSheet::new(4, 6),
    |cell| triangle_panel_bundle(cell),
)?;

commands.apply_topology(arrangement, candidate)?;
```

Failures caused only by later ECS state cannot be returned from a call that has
already completed. Report those failures against the `Arrangement` controller
through the diagnostic channel (`name TBD`) whose storage, history, and warning
policy F13 decides. Do not introduce a separate failure event or status
component through this decision.

The immediate `Result` covers everything knowable from the candidate and its
bound entity mapping; it does not promise that those entities remain unchanged
until deferred commands run. Entity lifetime, despawning, and replacement are
entirely application-owned; Hana adds no cleanup or entity-lifecycle policy.

**Status:** Decided.

**A6.4 status:** Decided.

### A6.5. Provider invocation and lifecycle

A6.5 is split into two decisions:

1. **A6.5.1 — Construction paths:** Decide topology-driven member spawning and
   the separate path for existing member entities.
2. **A6.5.2 — Persistence:** Decide which provider or topology-authoring state
   persists after topology materialization, and where.

A6.5.1 was reviewed before A6.4.2 because the normal construction path should
guarantee member coverage by construction. A6.4.2 therefore defines only the
defensive checks still needed for existing-member application, deferred
commands, scene loading, and direct ECS mutation.

#### A6.5.1. Topology-driven spawning and existing-member application

**Decision:** Provide two construction paths. The normal path lets a logical
topology drive member creation through an application-supplied bundle factory.
The helper:

1. spawns one controller carrying `Arrangement`;
2. iterates every `Position` supplied by `TopologyProvider::positions()`;
3. calls the bundle factory exactly once for each logical position;
4. spawns the returned bundle with `Member::of(arrangement)`;
5. associates each logical position with its new `Entity`;
6. supplies that association to the pure `generate_topology()` decided by
   A6.4.1; and
7. materializes each generated `MemberTopology` as durable connection data.

The factory supplies application-owned mesh, material, anchor geometry, and
other member components. The helper owns member entity creation and the
`Member` relationship, so every topology-declared logical position initially
produces exactly one `Member` by construction.

```rust
let authored = commands.spawn_arrangement(
    TriangleSheet::new(4, 6), // provider type name TBD
    |cell| triangle_panel_bundle(cell),
);

commands.apply_folding_recipe( // name TBD
    &authored,
    Accordion::default(), // recipe representation TBD
);
```

The second path binds a logical topology to already-existing `Member` entities
before invoking `generate_topology()`. It supports scene-loaded entities,
entities created by other systems, and later topology replacement. A6.4.2
provides its defensive validation and atomic-application contract.

Both paths produce `Topology` rather than
discarding the topology after inserting `AnchoredTo`. That result exposes the
logical-position-to-`Entity` association and compatible fold-group
capabilities needed by Accordion and future folding recipes. A compatible
recipe uses those capabilities to author complete `Hinge` endpoints, which a
`FoldSequence` can animate.

Whether `Topology` is transient or retained outside ECS remains the caller's
choice. A9.2.3 decides its folding-capability contents.

**Status:** Decided.

#### A6.5.2. Provider and topology-authoring persistence

**Decision:** Hana automatically persists neither the concrete topology
provider nor `Topology` in ECS. The
provider remains an ordinary Rust authoring value and may be discarded after
generation. Both construction paths return the result as an ordinary
Rust value containing the logical-position-to-`Entity` association and the
normalized capabilities needed by compatible folding recipes.

```rust
let topology = commands.spawn_arrangement(
    TriangleSheet::new(4, 6),
    |cell| triangle_panel_bundle(cell),
)?;

commands.apply_folding_recipe(&topology, Accordion::default())?;

// Drop `topology` after one-shot authoring, or retain it in
// application-owned storage for later recipe replacement.
```

Runtime folding uses the durable `Hinge` values produced by recipe authoring
and does not require either authoring value. An application that needs runtime
recipe replacement may retain the returned result in its own resource,
asset, or component. Hana does not add a trait-object provider component,
provider-specific systems, or a duplicate persistent topology graph.

A retained result is an ordinary snapshot containing `Entity` values.
Every reuse passes through the normal validation contract. Hana adds no stale
state, revision counter, observer, or automatic regeneration scheduler. A
caller that needs a different value explicitly runs a provider again. A9.2.3
defines the exact normalized fold-group capability inside `Topology`.

**Status:** Decided.

The provisional A6.5.2.2 invalidation-policy and A6.5.2.3 regeneration-scheduler
items were removed from the review because they would add lifecycle machinery
that A6.5.2 explicitly declines to retain.

**A6.5 status:** Decided.

### A6.6. Public topology-authoring vocabulary

**Decision:** Use `TopologyProvider` for the ordinary Rust provider trait, its
provider-specific logical location as `Position`, and its methods as
`positions()` and `generate_topology()`. Pass the position-to-`Entity` lookup
directly into generation, so no separate bound-provider type is needed.

```rust
pub trait TopologyProvider {
    type Position;

    fn positions(&self) -> impl Iterator<Item = Self::Position>;

    fn generate_topology(
        &self,
        entity_for: impl Fn(&Self::Position) -> Entity,
    ) -> Vec<MemberTopology>;
}
```

Name the ordinary caller-owned materialized result `Topology`, the
existing-member `Commands` extension `apply_topology()`, and the immediate
authoring and validation error `TopologyError`. Retain the previously approved
`spawn_arrangement()` name for topology-driven construction.

```rust
let topology = commands.spawn_arrangement(
    TriangleSheet::new(4, 6),
    |position| triangle_panel_bundle(position),
)?;
```

**Status:** Decided.

### A6.7. Logical topology-key vocabulary

The shared-semantic-math correction introduces `bevy_kana::Position` as the
type of `AnchorPoint::position`. The already approved
`TopologyProvider::Position` has a different meaning: it is a provider-specific
logical value used to map one planned member to an `Entity`, and may be a grid
coordinate, enum, or other key rather than a spatial point.

A6.7 must remove that public naming collision without changing the approved
provider lookup flow.

**Status:** Pending.

### A7. Anchor geometry vocabulary

The data flow is settled: `ResolvedAnchorGeometry` catalogs local anchor
points; `Edge` is an ordered pair of keys into that catalog and derives a local
axis; `AnchoredTo` is the immutable relationship selecting one source point,
one target point, and the target entity. Corrected A7.1 settles position and
orientation storage with shared `bevy_kana` types.

The remaining work is tracked through these atomic decisions:

1. **A7.2 — Point value:** Retain `AnchorPoint` for the stored
   position-plus-orientation value.
2. **A7.3 — Catalog key:** Replace or retain `AnchorId` for the
   provider-authored key that selects an `AnchorPoint`.
3. **A7.4 — Orientation validity:** Decide where invalid
   `bevy_kana::Orientation` values in anchor geometry are rejected.
4. **A7.5 — Semantic-math audit:** Apply the shared `bevy_kana`-type rule to
   every remaining raw spatial field in the anchor and hinge APIs.

**Status:** Partial; A7.2 decided and A7.3-A7.5 pending.

### A7.1. Anchor position and orientation representation

**Corrected decision:** Use the existing shared semantic types
`bevy_kana::Position` and `bevy_kana::Orientation`:

```rust
use bevy_kana::Orientation;
use bevy_kana::Position;

pub struct AnchorPoint {
    pub position: Position,
    pub orientation: Orientation,
}

let anchor = AnchorPoint {
    position: Position::default(),
    orientation: Orientation::default(),
};
```

`Position` wraps `Vec3` and `Orientation` wraps `Quat`. The original
Hana-specific `AnchorOrientation` proposal is superseded and adds no public
type. `Orientation::default()` means the anchor uses the entity's own local
orientation; the API does not encode that case as `None`.

`anchor_placement()` reads the source and target `AnchorPoint::orientation`
values when `resolve_anchors()` resolves an `AnchoredTo` relationship.
`Edge::axis()` reads both endpoint orientations when checking whether an
edge's local axes are compatible, and `ResolvedAnchorGeometry::validate()`
performs the same compatibility check before resolver systems consume provider
geometry. A7.4 decides where invalid shared `Orientation` values are rejected.

This correction removes the ambiguous distinction between a missing quaternion
and an explicit identity quaternion while following the shared semantic-math
rule instead of introducing a Hana-specific wrapper.

**Status:** Corrected and decided.

### A7.2. Point-value name

**Decision:** Retain `AnchorPoint`. It names one local attachment location in
`ResolvedAnchorGeometry`; `Position` and `Orientation` name its two spatial
values. Do not rename it to `AnchorPose`, which is the existing ECS component
used for animation-provided rotation and translation, or to `AnchorFrame`,
which adds no distinction needed by the API.

```rust
let point = AnchorPoint {
    position: Position::new(0.0, 1.0, 0.0),
    orientation: Orientation::default(),
};

geometry.points.insert(anchor_key, point); // key type pending A7.3
```

**Status:** Decided.

### A7.5. Shared semantic-math audit scope

The consistency audit identified these surviving or planned conversions for
later A7.5 review:

- `AnchoredTo::offset: Displacement`;
- `ResolvedAnchorOffset(Displacement)`, retaining the outer ECS component;
- `AnchorPose { rotation: Orientation, translation: Displacement }`;
- `ResolvedAnchorWorld::points` values as `Position`;
- `Hinge::pivot_offset: Displacement`; and
- hinge endpoints, `Connection::connection_angle`, and surviving recipe angle
  offsets as the planned `Angle`.

Raw `Vec3`, `Quat`, and scalar values remain appropriate at Bevy transform,
mesh, gizmo, axis-calculation, progress, easing, rate, and time boundaries.
This inventory records the audit scope but does not decide the individual
conversions before A7.5.

**Status:** Pending.

### A8. Placement wrappers and angle representation

**Decision:** Remove `ArrangementPlacement` and `MemberPlacement`.
`MemberTopology::Anchored(Connection)` retains `AnchoredTo`, the member-local
`Edge`, and the fixed connection `Angle`. Reconciliation combines that fixed angle with the built-in
recipe's endpoint offsets and stores only the resulting two `Angle` endpoints
on `Hinge`.

Add `Angle` to `bevy_kana`'s default math API as a shared, plain value stored in
radians. It represents a signed, unwrapped angular displacement; it is not an
ECS component, an orientation, a normalized rotation, or dimensionless
progress.

```rust
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Reflect)]
pub struct Angle(f32);

impl Angle {
    pub const ZERO: Self = Self(0.0);

    pub const fn from_radians(radians: f32) -> Self;
    pub fn from_degrees(degrees: f32) -> Self;

    pub const fn as_radians(self) -> f32;
    pub fn as_degrees(self) -> f32;
    pub const fn is_finite(self) -> bool;
}
```

Provide angle addition and subtraction, negation, and multiplication by `f32`.
Do not provide `From<f32>`, `Into<f32>`, `Deref`, a public tuple field, or
automatic wrapping. Conversion occurs explicitly at APIs that require radians:

```rust
let rotation = Quat::from_axis_angle(axis, angle.as_radians());
```

Finite multi-turn values are intentional. For example,
`Angle::from_degrees(720.0)` represents two complete turns; scalar interpolation
preserves both turns even though its final quaternion orientation equals the
initial orientation. Do not impose a magnitude range or normalize finite
angles. A10 owns rejection of non-finite resolved hinge calibration, while F13
owns how that failure is reported.

Use this shared type for Hana Valence's fixed connection angles, recipe angle
offsets, and `Hinge` endpoints. The original A8 decision also selected `Angle`
for `HingePivot::reference_angle`; A9.4 subsequently removes that field as
redundant with the unfolded `Hinge` endpoint and then merges the remaining
pivot offset into `Hinge`. This preserves the decision history while
superseding the field conversion and separate `HingePivot` type. Keep fold
progress and easing fractions as `f32`; use `bevy_kana::Orientation` for
semantic spatial-orientation fields and convert to `Quat` or `Rot2` at math
boundaries. After Hana Valence adopts `Angle`, audit the rest of the workspace
for scalar angular-displacement fields. Do not blanket-convert dimensionless
progress values or angular rates.

**Status:** Decided.

### A9.1. Hinge endpoint ownership and derived angle

**Current implementation:** `Hinge` stores `edge` plus one mutable current
`angle`. `FoldAngles` separately stores `unfolded` and `folded` endpoints.
`drive_arrangement_hinges()`, `actuate_fold_hinges()`, and `HingeAngleLens`
can all change the physical angle observed by `hinge_to_pose()`.

**Decision:** Make `Hinge` the one angle-bearing component. It stores its
`Edge` and its unfolded and folded endpoint `Angle` values. `hinge_to_pose()`
derives the current angle from those endpoints and the selected fold progress,
then writes `AnchorPose`; the current angle is not stored as component state.

```rust
pub struct Hinge {
    pub edge: Edge,
    pub unfolded_angle: Angle, // field name TBD
    pub folded_angle: Angle,   // field name TBD
}

let stage = fold_sequence.stage_for(member_entity); // method name TBD
let fraction = sequence_state.member_progress(stage, member_entity); // name TBD
let angle = hinge.unfolded_angle
    + (hinge.folded_angle - hinge.unfolded_angle) * fraction;
```

Remove `FoldAngles`, `Hinge::angle`, `drive_arrangement_hinges()`, and
`actuate_fold_hinges()`. `HingeAngleLens` cannot remain an angle writer once
the current angle is derived; A9.3 decides how external progress enters the
sequence model before its replacement is specified. Keep `AnchorPose`,
`FoldSequence`, `FoldStage`, and `FoldSequenceState`; they have separate
responsibilities pending their collective review under A9.3. A9.4 subsequently
merges the demonstrated `HingePivot` data into `Hinge::pivot_offset`, while A9.5
removes `FoldMember` / `FoldMembers`; A9.3.1.1 stores complete `FoldStage`
values in `FoldSequence` instead of putting numeric stage components on
participating `Member` entities.

The endpoint field names remain undecided. A9.2 decides how compatible,
potentially transient folding recipes materialize those endpoint values without
presuming that the POC `Strip`, `Accordion`, and `Coil` components survive.
A9.4 audits the remaining fold types and reports the final net concept count.

**Status:** Decided.

### A9.2. Fold-recipe authoring and topology capabilities

A9.2 no longer assumes that the POC `Strip`, `Accordion`, and `Coil` components
survive. A6 has now defined the `TopologyProvider` boundary. A9.2 will resolve
these decisions in order:

1. **A9.2.1 — Durable result:** Decide whether a folding recipe is transient
   authoring input whose durable ECS result is complete `Hinge` data.
2. **A9.2.2 — Fold-group value:** Use `FoldGroup` as the ordinary,
   authoring-only, nonempty ordered collection of `Member` entity values. It
   stores no `Connection`, recipe, timing, or ECS state.
   - **A9.2.2.1 — One reusable group:** **Decision:** use `FoldGroup` itself as
     the member selection shared by topology-aware recipe application and
     coordinated playback. Remove proposed `MemberSelection` and
     `SharedFoldStage`; `Topology` resolves group members to `Connection`
     values for a compatible recipe, while A9.3 decides how timing turns a
     group into `FoldStage`.
3. **A9.2.3 — Provider capability:** Define the optional topology-provider
   capability (`name TBD`) that returns fold groups and what provider data must
   remain available for runtime re-authoring.
   - **A9.2.3.1 — Alternative groupings:** Decide the structure through which a
     provider exposes alternative `FoldGroups`, including only the identity or
     selection information supported by concrete domain examples.
4. **A9.2.4 — Compatibility:** Define nonempty, membership, connectivity,
   orientation, uniqueness, finite-calibration, overlap, and failure
   invariants.
5. **A9.2.5 — Accordion:** Define alternating physical fold sense without
   reducing it to raw local-angle sign parity.
   - **A9.2.5.1 — Alternation position:** Decide whether Accordion alternates
     between outer `FoldGroup` positions, inner member positions, or a
     combination. **Decision:** alternate by outer group position only; inner
     position remains available to other recipes, and recipe authoring remains
     a pure iterator-to-`Vec<HingeAssignment>` calculation.
   - **A9.2.5.2 — Signed fold offset:** Decide whether one signed `Angle` on
     Accordion supplies both the first group's physical fold sense and the
     magnitude negated for each following group. **Decision:** yes; negating
     the recipe value reverses the pattern, while live transport remains in
     `FoldSequence` / `FoldSequenceState`.
     - **A9.2.5.2.1 — Default offset:** Decide whether
       `Accordion::default()` supplies a signed half-turn while explicit
       construction accepts any finite signed `Angle`. **Decision:** yes;
       default to `+pi` radians while keeping every finite signed offset
       explicitly authorable.
   - **A9.2.5.3 — Endpoint calculation:** Decide whether
     `Connection::connection_angle` supplies one complete `Hinge` endpoint and
     Accordion's alternating signed offset produces the other.
6. **A9.2.6 — Coil and Wrap:** Separate cumulative turning from physically
   exterior wrapping and define required clearance or pivot calibration.
7. **A9.2.7 — Application lifecycle:** Define initial application, runtime
   replacement, neutral-state travel, and all-or-nothing commit behavior with
   F9 and A10.

**Closure gate:** A9.2 closes only after final, non-TBD API examples cover a
triangle strip, quad grid, hex grid, direct-authored box or polyhedral net that
implements no fold-group capability, and one incompatible request that writes
nothing. The final type inventory must distinguish persistent ECS state from
authoring-only values and report the net public-type cost.

**Status:** Pending; A6 dependency resolved.

### A9.3. Fold transport, stage timing, and easing

**Existing types:** `FoldSequence` contains authored step duration, easing, and
initial endpoint. `FoldSequenceState` contains validated runtime transport
state and currently copies the authored easing. `FoldStage` is currently the
zero-based value used to calculate simultaneous member progress. `FoldCommand`
is carried by the entity-targeted `FoldCommandEvent` and changes
`FoldSequenceState`.

**Decisions within this active item:** Keep the `FoldStage` domain concept as
coordinated playback over a `FoldGroup`. Under A9.3.1.1, `FoldSequence` owns
complete `FoldStage` values, each stage owns one `FoldGroup`, and the public
numeric per-member stage component is removed. Keep one raw controller-level
position in `FoldSequenceState`. Remove the copied easing field from
`FoldSequenceState`; runtime transport remains raw.

A9.3.1.2.2 defines one complete resolved timing value (`FoldTiming`, name TBD)
per participating member. It contains a stage-relative start offset, duration,
and easing. `hinge_to_pose()` derives raw member progress from the stage clock,
then applies that member's easing exactly once:

```rust
let local_progress = timing.progress_at(state.stage_elapsed()); // names TBD
let fraction = timing.easing.sample_clamped(local_progress);
```

A stage ends at the greatest member `start_offset + duration`. Successive
stages remain non-overlapping, so advancement consumes elapsed time stage by
stage, including crossing multiple differently timed stages in one frame. It
can no longer divide the entire frame delta by one sequence-wide
`step_seconds` value. A9.3.1.2.3 resolves the complete per-member value through
whole-value precedence: member `Cascade::Override`, stage
`Cascade::Override`, then the required sequence default. `Cascade::Inherit`
falls through to the next scope. Exact member-entry storage remains A9.3.2.

**Command decision:** Keep all playback requests in the existing
entity-targeted `FoldCommandEvent`, carrying this command surface:

```rust
pub enum FoldCommand {
    Step(FoldDirection),
    PlayTo(FoldEndpoint),
    Pause,
    Resume,
    Reverse,
}
```

`Step` targets an adjacent stage boundary. `PlayTo` targets the named terminal
endpoint and is idempotent when repeated. `Pause` retains raw position, target,
direction, and whether the journey is a step or continuous play; `Resume`
continues that exact journey. `Reverse` preserves raw position and journey
kind while selecting the opposite adjacent boundary or terminal endpoint. It
leaves a paused journey paused and is a no-op while idle. Keep `FoldMotion` as
`Idle`, `Step`, or `Play`; represent pausing with one private orthogonal flag
and expose it through `FoldSequenceState::is_paused()`. Remove the current
context-sensitive directionless `FoldCommand::Play`.

**Component-boundary decision:** Keep `FoldSequence` as a separate optional
component on the same controller entity as `Arrangement`. Do not put
`Option<FoldSequence>` inside `Arrangement`: nesting removes no concept, makes
playback systems inspect an option, broadens `Changed<Arrangement>`, and loses
the direct insert/remove/change lifecycle for `FoldSequence`. Component
presence denotes staged-playback capability; `FoldSequenceState` exists only
while that capability is present. `spawn_arrangement(...)` supplies the guided
co-location path, while the A4 validity contract decides how to diagnose a
stray `FoldSequence`.

**Still pending within A9.3:** Decide the representation of a future external
raw-progress adapter, whether overshooting easing output may exceed the
calibrated `Hinge` endpoints, and exact timing and easing names. A9.4 owns the
remaining cleanup audit; it has already removed `FoldFromArrangement`.

**Canonical transport decision:** Keep `FoldSequenceState` and its
fold-specific target, direction, stage-boundary, step/play, readiness, and
revision semantics rather than replacing them with
`bevy_time_runner::TimeRunner`. `FoldCommandEvent` is the common initiation and
control path. A `bevy_tween` timeline can trigger a command without an adapter;
continuously driving progress remains a future adapter concern. Such an
adapter must select external ownership even while paused and must supply raw
progress because Hana retains the timing cascade and applies easing once.

**Status:** Partial; stage timing, easing ownership and precedence, component
boundaries, commands, and canonical transport decided. Exact names,
overshooting easing, and the future external-driver representation remain
pending.

### A9.4. Fold-pipeline simplification audit

**Decision within this active item:** Remove `FoldFromArrangement`, its
retained-request marker, snapshot system, snapshot diagnostic types, and the
validation exception that waits for the request. Its existing job is to copy
all arrangement members into the duplicate `FoldMember` relationship with one
member per consecutive numeric stage; A9.5 removes that relationship, and not
every `Member` must participate in playback.

Keep `FoldSequenceBuilder`, retargeted to the `Arrangement` controller, and
have it accept explicit `FoldGroup` values for stage authoring. The triangle
example collects its moving members and authors one single-member group per
stage. A newly added `Member` remains fixed until an author includes it in a
playback stage; member insertion must not silently renumber existing stages.
The exact builder method and stored `FoldStage` representation remain A9.3.1
and A9.3.2.

```rust
let mut builder = FoldSequenceBuilder::new(&mut commands, controller);
for member in folding_members {
    builder = builder.stage_with_timing( // method name TBD
        FoldGroup::from(member),
        stage_timing, // type name TBD
    );
}
builder.finish()?;
```

A serial-stage convenience (`name TBD`) may be considered under F15 after the
primary model is settled; it is not another ECS request component.

**Decision within this active item:** Remove `MemberPlacement`. It is an
existing plain value that wraps `ArrangementPlacement` and adds only the live
hinge `angle`. A9.1 removes that stored live angle and derives it in
`hinge_to_pose()`, so the wrapper has no remaining data or responsibility.

**Decision within this active item:** Remove the `ArrangementPlacement` name
and standalone public value. Its `AnchoredTo`, member-local `Edge`, and fixed
connection `Angle` remain together as the connected payload of the A6 result
(`name TBD`) because changing any one invalidates the others. Reconciliation
combines the fixed connection `Angle` with two endpoint-recipe `Angle` offsets
and stores only the resulting endpoint `Angle` values on `Hinge`; it does not
store a third base angle. A6.3 uses a named unanchored-or-anchored sum type rather
than `Option`, while its exact type, variant, and connected-payload names remain
pending.

**Decision within this active item:** Remove the public
`FoldSystems::Actuate` system-set variant. Its only system is
`actuate_fold_hinges()`, which A9.1 removes together with `FoldAngles` and
mutable `Hinge::angle`. The redesigned `hinge_to_pose()` directly reads the
member's stage participation from the controller's `FoldSequence`, the
controller's `FoldSequenceState`, and the member's endpoint-bearing `Hinge`,
then writes `AnchorPose`; no separate actuation phase remains.

Retain `FoldSystems::Advance` as the public boundary after controller transport
advancement. Existing consumers use it to read the newly advanced
`FoldSequenceState` or update data before `hinge_to_pose()`, and the future
single-writer raw-progress adapter will need the same ordering boundary. Order
`hinge_to_pose()` after `FoldSystems::Advance` without introducing a replacement
actuation set.

```rust
#[derive(SystemSet, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum FoldSystems {
    Advance,
}

app.configure_sets(
    PostUpdate,
    FoldSystems::Advance
        .in_set(AnchorSystems::AnimatePose)
        .before(hana_valence::hinge_to_pose),
);
```

**Decision within this active item:** Remove `FoldAngleInvalidReason`,
`FoldAngleDiagnostic`, and `FoldAngleDiagnostics`, including the resource
initialization in `FoldPlugin` and the now-empty `fold::hinge` adapter module.
Their only producer is the removed `actuate_fold_hinges()`, and no runtime
system reads the retained history to affect behavior. Do not merge these
member-local hinge failures into controller-level `FoldDiagnostics`.

`hinge_to_pose()` continues the existing safe-skip behavior: it checks the
derived `Angle` before converting it to radians or writing `AnchorPose`, emits a
trace warning for a non-finite result, and leaves the existing pose unchanged.
F13 must later review one coherent anchor-and-hinge diagnostic API, including
whether failures need retained history, warning deduplication or throttling,
and structured reasons for non-finite `Angle` endpoints and interpolation.

```rust
let angle = hinge.unfolded_angle
    + (hinge.folded_angle - hinge.unfolded_angle) * fraction;

if !angle.is_finite() {
    tracing::warn!(entity = ?entity, "hinge angle is non-finite");
    continue;
}

let rotation = Quat::from_axis_angle(*axis, angle.as_radians());
```

**Decision within this active item:** Remove the public `Hinge::rotation()`
method without replacement. The current method has one unambiguous result only
because the implemented `Hinge` stores one mutable current angle. Under A9.1,
`Hinge` instead stores two endpoint `Angle` values, so selecting a current
rotation requires the member's participation in a `FoldStage`, the controller's
`FoldSequenceState`, and resolved easing.

Do not add `rotation_at()` or `rotation_at_progress()`: either would recreate a
second fold-angle evaluation path beside `hinge_to_pose()`. Authors supply
calibration and playback intent, while `hinge_to_pose()` remains the one system
that selects the current angle and writes `AnchorPose`. `Edge::axis()` remains
available for non-fold geometry use.

```rust
commands.entity(lid).insert((
    Member::of(box_arrangement),
    Hinge {
        edge: lid_edge,
        unfolded_angle: Angle::ZERO,             // field name TBD
        folded_angle: Angle::from_degrees(90.0), // field name TBD
    },
));

sequence.stage_with_timing( // method name TBD
    FoldGroup::from(lid),
    lid_timing, // type name TBD
);

commands.trigger(FoldCommandEvent::new(
    box_arrangement,
    FoldCommand::PlayTo(FoldEndpoint::Folded),
));
```

**Decision within this active item:** Keep `HingePivot` as the optional
component for an external pivot-line offset, but remove
`HingePivot::reference_angle`. Define `HingePivot::offset` in the conformation
represented by the unfolded `Hinge` endpoint (`field name TBD`).
`hinge_to_pose()` calculates pivot translation from the difference between the
derived current angle and that endpoint.

Every shipped example that combines `HingePivot` with fold endpoints already
uses the unfolded endpoint as its reference angle. The new invariant removes
one duplicated `Angle` field and prevents the pivot reference from disagreeing
with the unfolded conformation. It intentionally removes undemonstrated support
for declaring zero pivot translation at an arbitrary non-endpoint angle.

```rust
pub struct HingePivot {
    pub offset: Vec3,
}

let pivot_delta =
    current_angle - hinge.unfolded_angle; // endpoint field name TBD
let pivot_rotation =
    Quat::from_axis_angle(*axis, pivot_delta.as_radians());
let translation = pivot.offset - pivot_rotation * pivot.offset;
```

This intermediate decision to retain a separate, offset-only `HingePivot` is
superseded by the later A9.4 merge below. Its removal of `reference_angle` and
unfolded-endpoint calibration remain part of the merged representation.

**Decision within this active item:** Declare `AnchorPose` as a Bevy required
component of `Hinge`. After removal of public `Hinge::rotation()`,
`hinge_to_pose()` is the supported interpretation path and requires mutable
`AnchorPose`; a missing pose would otherwise silently exclude the entity from
the system. Every surviving production path and direct-authoring example
already inserts `AnchorPose::default()` beside `Hinge`.

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[require(AnchorPose)]
#[reflect(Component)]
pub struct Hinge {
    pub edge: Edge,
    pub unfolded_angle: Angle, // field name TBD
    pub folded_angle: Angle,   // field name TBD
}
```

Bevy inserts `AnchorPose::default()` only when no explicit `AnchorPose` is
supplied. Ordinary `remove::<Hinge>()` leaves `AnchorPose` in place so another
animation system can take ownership; a caller deliberately wanting both gone
can use Bevy's required-component removal operation. This insertion guarantee
does not prevent a caller from explicitly removing `AnchorPose` later, so F13
and A10 still own diagnostics and broader validity policy.

Do not require `AnchoredTo`, `ResolvedAnchorGeometry`, or `Member`: physical
connection roots, directly authored or fixed hinges, and geometry that arrives
later remain valid. `FoldStage` is sequence-owned rather than an ECS component.

**Decision within this active item:** Merge the remaining `HingePivot::offset`
into `Hinge::pivot_offset` and remove the `HingePivot` component. `Vec3::ZERO`
is the exact identity for no pivot compensation; a small nonzero value remains
meaningful geometry and must not be collapsed through an epsilon or `Option`.
`hinge_to_pose()` is the only runtime reader of both values, and every
meaningful authoring site supplies them together. The merge makes endpoint and
pivot calibration one atomic `Hinge` update.

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[require(AnchorPose)]
#[reflect(Component)]
pub struct Hinge {
    pub edge: Edge,
    pub unfolded_angle: Angle, // field name TBD
    pub folded_angle: Angle,   // field name TBD
    pub pivot_offset: Vec3,
}

let thin_hinge = Hinge {
    edge,
    unfolded_angle: Angle::ZERO,             // field name TBD
    folded_angle: Angle::from_degrees(90.0), // field name TBD
    pivot_offset: Vec3::ZERO,
};
```

For every `Hinge` carrying `AnchoredTo`, `hinge_to_pose()` converts the edge
axis through the `AnchoredTo::source_anchor` tangent frame; `pivot_offset`
controls translation only. An unanchored `Member` without `AnchoredTo` may use
`Vec3::ZERO`; a nonzero offset without a source anchor frame is invalid and is
handled by A10 and F13.

**Decision within this active item:** Remove `AnchorPoseLens`, the resulting
empty `tween` module and crate feature, and Hana Valence's optional
`bevy_tween` dependency. No production path, example, or test registers or
constructs the lens. It cannot safely share an entity with `Hinge` because
`hinge_to_pose()` writes the complete `AnchorPose`, and it is not a fold
transport adapter because direct pose writes bypass `FoldSequenceState`,
`FoldStage`, calibrated endpoints, easing, and pivot compensation.

Keep `AnchorPose` and direct non-hinge pose animation. The panel fan and morph
demonstrate purpose-built systems writing `AnchorPose`, while F2, F3, and F11
require capture, coordinated relationship commits, or continuous multi-turn
composition that the generic lens does not provide. A9.3 retains the future
single-writer raw-progress adapter as the correct continuous fold integration.

**Superseding A9.2.2.1 decision:** Do not add `SharedFoldStage`. `FoldGroup` is
the one plain authoring value containing a nonempty ordered collection of
`Member` entity values. Preserve the already approved ergonomic construction
and nonempty invariant on that shared type:

```rust
pub struct FoldGroup {
    members: Vec<Entity>,
}

impl From<Entity> for FoldGroup {
    fn from(member: Entity) -> Self {
        Self {
            members: vec![member],
        }
    }
}

impl<I> From<(Entity, I)> for FoldGroup
where
    I: IntoIterator<Item = Entity>,
{
    fn from((first, remaining): (Entity, I)) -> Self {
        let mut members = vec![first];
        members.extend(remaining);
        Self { members }
    }
}
```

Runtime collections use `FoldGroup::try_from_iter()` or
`TryFrom<Vec<Entity>>`; an empty collection returns
`FoldAuthorError::EmptyStage` before a group can enter the builder. Do not
implement infallible `From<Vec<Entity>>` or `FromIterator<Entity>`, because both
can receive zero entities.

```rust
let walls = FoldGroup::new(north, [south, east, west]);

FoldSequenceBuilder::new(&mut commands, arrangement)
    .stage_with_timing(FoldGroup::from(lid), lid_timing) // names TBD
    .stage_with_timing(walls, wall_timing)               // names TBD
    .finish()?;
```

Keep `FoldAuthorError::DuplicateMember(Entity)`: entity equality is runtime
data and cannot be proven by typestate. `finish()` validates all stages before
queuing any command, so a duplicate never creates partial ECS state. A
zero-stage builder remains valid so an arrangement may receive stages later.
No typestate markers or secondary stage builder are added. A9.3 may later add
stage-level timing around `FoldGroup` without changing its nonempty invariant.

The retained public concepts now each have one responsibility: `FoldPlugin`
installs validation, command handling, and playback; `FoldSystems::Advance`
marks the controller-transport scheduling boundary; `FoldSequenceBuilder` and
`FoldAuthorError` provide transactional stage authoring; and `FoldGroup` names
the one reusable nonempty member group without becoming ECS state.

The eleven previously approved public-type removals, one public system-set
variant removal, one public method removal, and optional tween integration
removal remain decided. Eliminating the proposed `SharedFoldStage` addition and
retaining `FoldStage` as a sequence-owned struct means this local
simplification audit removes eleven public types and adds none. The complete A9
concept count remains a closure-gate calculation after A9.2 and A9.3 settle all
new authoring values. Direct purpose-built `AnchorPose` writers remain
supported, and F13 owns the replacement diagnostic policy.

**Status:** Removals and transactional validation decided; `SharedFoldStage`
superseded by `FoldGroup`; local net reduction is eleven public types; complete
A9 concept count remains pending the A9 closure gate.

### A9.5. Sequence participation and arrangement membership

**Decision:** Remove the `FoldMember` / `FoldMembers` relationship. Every
physical entity carries the one `Member` relationship targeting its
`Arrangement` controller. Do not create a second relationship merely to record
sequence participation.

```rust
commands.entity(fixed_face).insert(Member::of(controller));

let walls = FoldGroup::new(north, [south, east, west]);
sequence.stage_with_timing(walls, wall_timing); // names TBD
```

The optional `FoldSequence` is colocated with `Arrangement` on that controller.
The Bevy-maintained `Members` collection remains the authoritative arrangement
membership. `FoldGroup` selects members for recipe or playback authoring
without becoming another ECS relationship. This retains independent physical
topology through `AnchoredTo` and independent animation order through
`FoldStage`.

A9.2.2.1 supersedes the earlier decision to put the existing numeric
`FoldStage` directly on participating `Member` entities. A9.3.1.1 decides that
`FoldSequence` owns the complete stage collection and each `FoldStage` owns its
`FoldGroup`. No public numeric marker remains. A fixed member is a `Member`
omitted from every active playback stage.

This model supports one active `FoldSequence` per `Arrangement`. The existing
`FoldMember` already permits only one sequence assignment per entity, so the
decision removes no demonstrated multi-sequence behavior. A new assignment
model should be introduced only if a future feature requires overlapping
independent sequences.

`FoldFromArrangement` depended on the duplicate relationship and is removed
under A9.4. `FoldSequenceBuilder` remains the explicit authoring helper.

**Status:** `FoldMember` / `FoldMembers` removal and single `Member` / `Members`
relationship decided; stage participation is stored in sequence-owned
`FoldStage` / `FoldGroup` values.

The boundary used here is:

- `hana_valence` owns relationships, ordered arrangements, pose and hinge
  motion, transitions between authored relationships, and related diagnostics;
- adapters such as `hana_diegetic` provide geometry and coordinate-space
  conversion;
- applications own controls, labels, colors, camera presentation, and teaching
  UI.

## Existing foundation

Several demonstrated capabilities are already present and should be reused:

- `AnchoredTo` and `AnchoredHere` maintain source-to-target relationships.
- `ResolvedAnchorOffset` supplies a live offset without replacing the authored
  relationship.
- `AnchorPose` and `Hinge` separate animation from resolved transforms.
- `AnchorPoseLens` and `HingeAngleLens` currently provide tween adapters; A9.4
  and A9.1 remove both unused or conflicting adapters.
- `ArrangementMembers` and `MemberIndex` maintain insertion order as members
  are added and removed.
- `Strip`, `Accordion`, `Coil`, and `TilingRule` describe linear arrangements.
- `FoldSequence` provides staged fold playback with step and continuous play
  commands.

The candidates below concern behavior still implemented by the example or
behavior whose existing API is not sufficient for the example.

## Candidate features

### F1. Arrangement activation and suspension

**Demonstrated behavior:** One persistent entity set alternates between a
freely authored anchor fan and an arrangement-driven hinge chain. Arrangement
placement and hinge writing must stop while the fan or a transition owns the
same entities, then resume without losing member order.

**Candidate:** Add an explicit active/suspended state for an arrangement. A
suspended arrangement would retain its root, members, and relationship order while
its placement and hinge systems perform no writes. Reactivation would mark
members for placement again and resume angle driving.

**Why it belongs:** Writer ownership and member lifecycle are properties of the
arrangement runtime, not of panel UI.

**Review questions:**

- Should suspension retain `Hinge` and `AnchoredTo`, or remove runtime-owned
  outputs and reconstruct them on activation?
- Should activation be a component, an entity-targeted event, or both?
- How should activation interact with `Hinge` endpoint materialization and
  optional `FoldSequence` playback?

### F2. Smooth relationship replacement

**Demonstrated behavior:** Changing either the source or target anchor moves an
attached entity from its current resolved position to the newly authored
relationship without a jump. The example replaces the relationship, computes
the old-to-new displacement in the target frame, and eases an `AnchorPose`
translation back to zero.

**Candidate:** Provide a relationship-transition request that replaces an
`AnchoredTo` relationship while preserving the current resolved pose, then
eases to the new relationship. It should support source-anchor changes,
target-anchor changes, target-entity changes, and offset changes.

**Why it belongs:** The operation depends on Valence geometry, relationship
hooks, pose ownership, and resolver scheduling.

**Review questions:**

- Should transition requests capture the old resolved world pose immediately
  or at the next resolver pass?
- Which changes can be represented by translation alone, and which require
  rotation interpolation?
- What diagnostics should be recorded when either relationship cannot resolve?

### F3. Multi-entity relationship morphs

**Demonstrated behavior:** The same entities move between two complete layouts
without respawning. The example captures every live pose, keeps the outgoing
relationships during the ease, computes poses that reproduce the incoming
layout, and replaces the relationships only after the visual transition has
landed.

**Candidate:** Add a coordinated transition for a set of relationship changes.
The transition should capture all starting poses, prevent competing writers,
advance one shared progress value, and commit the replacement relationships as
one operation.

**Why it belongs:** This is the set-level form of smooth relationship
replacement and requires dependency-ordered resolution.

**Review questions:**

- Is this a general attachment transaction or an arrangement-to-arrangement
  transition?
- Must the old and new relationship graphs contain the same entities?
- How should cycles or partially invalid destination graphs be rejected before
  motion begins?

### F4. Arrangement-type replacement

**Demonstrated behavior:** A hinge chain changes between accordion and coil
behavior. A selection made while folded does not alter the live pose; the
change is committed only when the next fold begins through the flat state.

**Candidate:** Provide an entity-targeted operation that replaces the current
arrangement component and guarantees that `Strip`, `Accordion`, and `Coil` are
mutually exclusive. Replacement should preserve members and relationship order and mark
the arrangement for recalculation.

**Why it belongs:** Arrangement identity and the exactly-one invariant are core
arrangement concerns.

The short-term observer design is investigated separately in
[`mutex_arrangements.md`](mutex_arrangements.md).

### F5. Scalar arrangement-fold transport

**Demonstrated behavior:** Every hinge in an accordion or coil shares one fold
progress value. A full transition has constant duration regardless of member
count, unlike a staged sequence where members advance at successive stage
boundaries.

**Candidate:** Add a small transport for driving the `fold` field on one
arrangement component. It would own progress, endpoint, timing, easing,
direction, and motion state while the existing arrangement system converts the
result into member hinge angles.

**Why it belongs:** It is reusable motion over the public arrangement
components and is distinct from `FoldSequence`'s staged member playback.

**Review questions:**

- Should this extend `FoldSequence`, share its playback internals, or be a
  separate `ArrangementFold` component?
- Should duration describe the whole arrangement transition or radians per
  second?
- Should it support both `Accordion` and `Coil` through one command API?

### F6. Explicit terminal commands

**Demonstrated behavior:** Fold and reset commands target a known terminal
state: fully folded or fully spread out. The command does not depend on which
endpoint is currently active.

**Decision from A9.3:** Use `PlayTo(FoldEndpoint)` for explicit terminal
travel. It selects continuous playback, and requesting the same endpoint again
is an idempotent no-op. Keep `Step(FoldDirection)` for adjacent-boundary
travel. Do not add separate `Fold` and `Unfold` command variants.

**Status:** Decided.

### F7. Pause and resume

**Demonstrated behavior:** Both spin and hinge-fold motion can freeze at the
current continuous position and resume without resetting timers, endpoints, or
direction.

**Decision from A9.3:** Add `FoldCommand::Pause` and `FoldCommand::Resume`.
Pausing retains the raw position, target, direction, and `FoldMotion`; resuming
continues that exact journey. Expose the state through
`FoldSequenceState::is_paused()`. Virtual-time pause is not sufficient when one
animation must pause while unrelated application motion continues.

**Why it belongs:** The current folding as-built explicitly has no pause state,
so applications must currently duplicate this transport concern.

**Status:** Decided.

### F8. Step versus continuous travel

**Demonstrated behavior:** `Step` advances one rest state and stops. `Glide`
continues through the intermediate flat state to the requested destination.

**Decision from A9.3:** Choose transport policy per command:
`Step(FoldDirection)` selects adjacent-boundary travel and
`PlayTo(FoldEndpoint)` selects terminal travel. Retain `FoldMotion::Step` and
`FoldMotion::Play` as runtime state instead of maintaining an application
state machine.

**Remaining question:** Is the example's two-state scalar fold a one-stage
`FoldSequence` or does it need the separate transport considered by F5?

**Status:** Partial; command policy decided, scalar transport reuse pending F5.

### F9. Safe reconfiguration through a neutral state

**Demonstrated behavior:** Changing accordion/coil selection or fold direction
while the assembly is folded first unfolds it, commits the pending
configuration only when it is flat, and then refolds. This avoids discontinuous
geometry.

**Candidate:** Add a queued reconfiguration transaction with three steps:
travel to a required neutral endpoint, replace selected arrangement data, then
optionally return to the requested terminal endpoint.

**Why it belongs:** The safe commit point depends on arrangement geometry and
transport state. Applications should issue an intent without reproducing the
transition rules.

### F10. Live resolved-offset animation

**Demonstrated behavior:** The depth between linked panels changes continuously
along the target anchor frame while the authored relationship stays intact.
Every member in the chain reflects the per-link offset.

**Candidate:** Add a tween lens or small driver for `ResolvedAnchorOffset`, plus
an optional helper for applying one per-link offset across an ordered
arrangement.

**Why it belongs:** `ResolvedAnchorOffset` already establishes the runtime
override; only reusable animation and arrangement propagation are missing.

### F11. Anchored pose spin envelope

**Demonstrated behavior:** An anchored assembly accelerates into rotation about
the anchor plane normal, can pause, decelerates symmetrically, and settles on
the nearest full turn so the resting orientation is stable.

**Candidate:** Provide an optional pose-motion adapter for continuous rotation
with rise, hold, fall, pause, and terminal-angle snapping. Axis selection should
come from resolved anchor or edge geometry rather than panel coordinates.

**Review questions:**

- Is continuous spin common enough for core `hana_valence`, or should it live in
  a separate animation adapter crate?
- Should the adapter write `AnchorPose` directly or produce a scalar angle for
  another system to compose?

### F12. Dynamic member reconciliation

**Demonstrated behavior:** Members can be appended and removed while any
capability is active. Existing entities keep their relative order, and new
members join the current arrangement and animation state.

**Candidate:** Derive positions from the current `Members` order, then add an
explicit reactivation/replacement path that marks all retained members pending
when an arrangement's topology or ownership changes. Document how insertion,
removal, and retargeting affect relationship order.

**Current support:** Addition, removal, insertion order, and stored stable
indices are implemented today. A3 replaces those indices with derived
positions. This review concerns reactivation and topology changes, not basic
member storage.

### F13. Anchor and relationship diagnostics

**Demonstrated behavior:** The example can display active source and target
anchor locations and draw the world-space link between separated anchors.

**Candidate:** Add an optional diagnostic/gizmo plugin that can render resolved
anchor points, local frames, ordered edges, relationship links, hinge axes, and
skipped relationships. This should consume `ResolvedAnchorGeometry`,
`ResolvedAnchorWorld`, and existing diagnostics without depending on panel
layout types.

**Why it belongs:** The information is useful to every geometry provider and
would make authored relationship failures easier to inspect.

**Required review from A9.4:** The removal of the adapter-specific
`FoldAngleInvalidReason`, `FoldAngleDiagnostic`, and `FoldAngleDiagnostics`
family must not end the diagnostic discussion. Decide one coherent policy for
anchor and hinge failures, including non-finite `Angle` endpoints or derived
angles, retained versus transient diagnostics, structured failure reasons, and
warning deduplication or throttling. Keep controller-level `FoldDiagnostics`
separate unless this review demonstrates a useful common representation.

### F14. Closed-loop connection constraints

**Motivating behavior:** A kaleidocycle is a moving closed ring. Cutting one
connection produces an acyclic tree that Hana can resolve, but animating the
closed mechanism requires the omitted seam to remain satisfied throughout the
motion.

**Candidate:** Add an explicit closure constraint that is separate from
`AnchoredTo`. Keep transform resolution acyclic while a specialized kinematic
rule or solver computes compatible hinge values and reports an unsatisfied
closing seam.

**Review questions:**

- Is a declarative closing-seam constraint sufficient, or is a general
  kinematic constraint solver required?
- Does the first implementation target one analytically solvable loop, such as
  a kaleidocycle, or arbitrary loops?
- How are impossible configurations and accumulated numerical error reported?

### F15. Pattern-oriented arrangement builders

**Motivation:** After the primary arrangement, connection, hinge, and sequence
model is settled, common arrangements should not require repetitive manual
setup. Authors should be able to intelligently apply or "paste" reusable
patterns over an explicit member selection.

**Candidate:** Review builder and arrangement-authoring features for materializing
common patterns such as serial `FoldStage` construction, simultaneous or
staggered `FoldGroup` timing, fixed-member exclusions, repeated connection
layouts, and reusable endpoint recipes. These helpers must produce the same
ordinary `Member`, `AnchoredTo`, `Hinge`, `FoldGroup`, and sequence-owned
`FoldStage` data as direct authoring rather than create a parallel runtime
model.

**Scheduling:** Review this only after the primary A1-A10 decisions are
complete, so the convenience API is built over the final concepts instead of
preserving obsolete setup mechanisms such as `FoldFromArrangement`.

**Review questions:**

- Which repeated setup patterns are demonstrated strongly enough to deserve a
  named builder operation?
- Should a pattern copy concrete authored data, apply a reusable recipe, or
  support both through separate operations?
- How does a helper report partial matches, incompatible member geometry, or
  invalid topology without silently guessing?
- Which generated values remain individually editable after the pattern is
  applied?

## Keep outside `hana_valence`

These example features do not belong in the core crate:

- keyboard mappings, key-repeat timing, and highlighted control chips;
- capability menus and explanatory screen panels;
- panel colors, borders, labels, and anchor-marker layout elements;
- the panel-specific 3-by-3 anchor navigation model and human-readable anchor
  names;
- camera orbit presets, viewport-overflow detection, and automatic camera
  fitting;
- panel rebuilding thresholds used to limit presentation updates;
- `hana_diegetic` unit conversion and panel geometry publication.

The general operations behind some of these features can still be Hana APIs.
For example, the 3-by-3 keyboard navigator stays in the example, while smooth
replacement of one `AnchorId` with another is a Valence concern.

## Review order

1. Decide arrangement activation/suspension semantics; the migrated example
   exposes this boundary immediately.
2. Review the mutually exclusive arrangement design in
   [`mutex_arrangements.md`](mutex_arrangements.md).
3. Decide whether smooth relationship replacement and coordinated set morphs
   are one API or two layers.
4. Decide whether scalar arrangement folding reuses `FoldSequence` playback
   internals or receives its own public transport.
5. Review pause/resume, explicit endpoint commands, travel policy, and neutral
   reconfiguration together because they affect one state model.
6. Decide whether offset animation and spin are core adapters or deferred
   extensions.
7. Decide whether diagnostic gizmos belong behind a `hana_valence` feature or
   in a companion integration crate.
8. Decide whether closed-loop constraints are in scope for the first general
   arrangement plan or explicitly deferred.
9. After the primary A1-A10 decisions, review pattern-oriented builders that
   intelligently apply common setup patterns without introducing parallel
   runtime concepts.

## Consequences from the arrangement migration

The migration removed the example's manual hinge-chain relationship authoring,
per-member accordion/coil angle calculation, and direct hinge-pose writer.
Tile zero now carries `QuadTiling` plus the committed `Accordion` or `Coil`,
later tiles join through `ArrangedPanel`, and the installed arrangement and
hinge systems own placement and rotation.

The example still owns `HingeChain` interaction state: selected versus
committed arrangement, direction, action, step/glide choice, pause state,
timers, unfold-first behavior, and queued refolding. Each frame it converts the
committed state into the root component's `fold` and `lean` fields. This is
direct evidence for F5 through F9 rather than evidence that the
arrangement geometry API is incomplete.

Two current API limitations required explicit lifecycle work in the example:

- `reconcile_hinge_arrangement` removes arrangement membership, runtime hinges,
  and root arrangement components while the fan or a mode morph owns the
  entities, then reconstructs membership when the hinge capability becomes
  active. F1 would replace this application-owned ownership handoff.
- Switching accordion/coil commits by explicitly removing both sibling
  arrangement types before inserting the selected component. F4 and
  [`mutex_arrangements.md`](mutex_arrangements.md) address that invariant.

The fan-to-hinge visual transition remains application-owned because Hana has
no coordinated relationship morph. Dynamic growth and stable ordering did not
require a new core mechanism: existing member observers and indices handled
new members once the example inserted `ArrangedPanel` in tile order.

## Accepted work and implementation order

- **A1:** Convert arrangement membership to a Bevy relationship and use its
  relationship target to maintain the reverse member collection. Remove custom
  reverse-list maintenance.
- **A2.1:** Name the `Member` component's arrangement-root `Entity` field
  `root_entity`.
- **A2.2:** Name the membership relationship pair `Member` / `Members`.
- **A3:** Remove `MemberIndex` and its assignment/repair systems. Derive current
  position by enumerating `Members`; introduce stored indexing later only if an
  accepted feature requires it.
- **A4.1:** Use a separate controller entity as the arrangement root. Make every
  physical tile a `Member`, keep physical `AnchoredTo` connection trees
  independent from membership order, and permit multiple physical connection
  roots. Keep `AnchoredTo` acyclic; track moving closed-loop support under F14.
- **A4.2 (reopened):** Add `Arrangement` as the controller identity and allow
  zero-member arrangements. Do not require `Strip`, `Accordion`, or `Coil`;
  directly authored `AnchoredTo` and `Hinge` data is valid. Decide the remaining
  topology-provider validity contract under A6 and the optional built-in recipe
  representation under A9.2. The A6 behavior is complete; A6.7 retains only a
  naming correction, and A9.2 remains open.
- **A4.3:** Add a `Commands` extension method named `spawn_arrangement` without
  hidden defaults. Keep invalid resolved data and membership intact but make
  dependent writers inert, emit a non-spamming diagnostic, and reconcile
  automatically after validity is restored. Finalize the helper parameters
  after A4.2 and A9.2.
- **A5:** Keep the topology-provider responsibility separate from
  `Strip`, `Accordion`, and `Coil`. Allow one connection topology to be reused
  with different folding behavior; keep staged `FoldSequence` orchestration
  outside both responsibilities.
- **A6.0:** Model logical-pattern and physical-graph authoring with
  `TopologyProvider`, which may expose recipe-compatible fold directions.
  Expect regular triangle, quad, and hex sheets to support arbitrary finite
  dimensions plus Accordion, Coil, and Wrap; treat a strip as their
  one-dimensional degenerate case.
- **A6.1:** Let `TopologyProvider` decide independently for every `Member`
  whether it has no `AnchoredTo` or has one whose `AnchoredTo::target()` it
  selects. Never infer the preceding `Member` as a physical connection.
- **A6.2:** Let each `TopologyProvider` define its own `Position` type.
  Have the selected construction path associate those positions with actual
  `Member` entities and supply that lookup during generation. Require
  Hana-facing results to identify source and target with `Entity`; add no
  universal logical member identity.
- **A6.3.1:** Use transient, authoring-only
  `MemberTopology::{Unanchored, Anchored}`. Materialization consumes it and
  leaves ordinary `Member` plus optional `AnchoredTo` ECS state; reserve
  "arrangement root" for the controller referenced by `Member::root_entity`.
- **A6.3.2:** Use authoring-only
  `Connection { member_entity, anchored_to, member_edge, connection_angle }`
  as the payload of `MemberTopology::Anchored`. Treat it as a revisitable
  starting point and refine it when implementation friction provides evidence.
- **A6.4.1:** Generate topology outside the Bevy `World` from a supplied
  provider-position-to-`Entity` lookup. Let the provider enumerate its logical
  positions and return one explicit `MemberTopology` per associated member;
  vector order is diagnostic only and expresses no connection or fold order.
- **A6.4.2.1:** Let topology-driven spawning guarantee one factory call, one
  entity, one `Member` relationship, and one binding per logical position.
  Validate the provider-authored `MemberTopology` values against that binding
  on both construction paths. On the existing-member path, additionally verify
  current entity existence and `Member` ownership by the intended controller.
- **A6.4.2.2:** Require generated `AnchoredTo` references to form a forest over
  the exactly covered bound members. Permit an empty topology, multiple
  unanchored members, disconnected trees, branching, repeated targets, and
  finite multi-turn angles. Reject self-targets, longer cycles, non-finite
  offsets or connection angles, and `member_edge` values with identical anchor
  identifiers. Defer geometry-dependent checks.
- **A6.4.2.3:** Validate the full topology before queuing topology writes. An
  invalid candidate queues nothing; a valid candidate queues the complete
  `AnchoredTo` removal-and-replacement plan through ordinary `Commands`. Keep
  `World` and `&mut World` out of the public API, require no exclusive custom
  command, and do not promise rollback against unrelated mutation before the
  queued writes run.
- **A6.4.3:** Return generation and structural-validation failures
  immediately as `Result<_, TopologyError>`. Report failures that
  arise only from later ECS state against the `Arrangement` controller through
  the F13 diagnostic channel (`name TBD`); add no separate failure event or
  status component. Keep entity lifetime and despawning entirely
  application-owned.
- **A6.5.1:** Make topology-driven spawning the normal construction path. Let
  `TopologyProvider::positions()` enumerate logical positions, call an
  application-supplied bundle factory once per position, and have the helper
  spawn each entity with `Member::of(arrangement)`. Supply the resulting entity
  lookup to topology generation. Retain `apply_topology()` for binding existing
  `Member` entities. Have both paths return `Topology`, whose entity mapping
  and compatible fold-group capabilities remain available to Accordion and
  future folding recipes.
- **A6.5.2:** Persist neither concrete topology providers nor `Topology`
  automatically in ECS. Return `Topology` as an
  ordinary Rust value that callers may use once and drop or retain in
  application-owned storage for later recipe re-authoring. Runtime playback
  depends only on the durable authored ECS outputs. Reuse validates the value
  normally, and callers explicitly create replacements; add no invalidation or
  regeneration lifecycle machinery.
- **A6.6:** Use `TopologyProvider`, `Position`, `positions()`,
  `generate_topology()`, caller-owned `Topology`, `apply_topology()`, and
  `TopologyError`; retain `spawn_arrangement()`. Pass the provider-specific
  position-to-`Entity` lookup directly into generation and add no separate
  bound-provider type.
- **A7.1 (corrected):** Represent `AnchorPoint::position` with
  `bevy_kana::Position` and `AnchorPoint::orientation` with
  `bevy_kana::Orientation`. Both are non-optional; origin and identity are
  their defaults. Remove the superseded `AnchorOrientation` proposal.
- **A7.2:** Retain `AnchorPoint` for the local attachment position and
  orientation value. Keep `AnchorPose` as the separate runtime animation input.
- **A8:** Remove `ArrangementPlacement` and `MemberPlacement`. Add the shared,
  signed, unwrapped `bevy_kana::Angle` value and use it for Hana Valence's fixed
  connection angles, recipe offsets, and `Hinge` endpoints. The initially
  approved `HingePivot::reference_angle` conversion is superseded by its A9.4
  removal. Keep unit conversion explicit, then audit the rest of the workspace
  for scalar angular-displacement fields without blanket-converting
  orientations, progress values, or angular rates.
- **A9.1:** Move the unfolded and folded endpoint angles into `Hinge`, remove
  stored current-angle state and `FoldAngles`, and derive the current angle in
  `hinge_to_pose()`. Remove the two built-in angle writers; decide external
  progress under A9.3. Endpoint field names remain pending.
- **A9.3 (partial):** Keep `FoldStage` as the named coordinated-playback
  concept over a `FoldGroup`. `FoldSequence` owns `Vec<FoldStage>`, every stage
  owns one group, and no public numeric per-member stage component remains.
  Successive stages are non-overlapping `Step` boundaries; simultaneous,
  staggered, and wave overlap belongs inside one stage. Each resolved member
  timing contains a stage-relative start offset, duration, and easing; the
  stage ends at the latest member end. Resolve that complete value through a
  required sequence default and whole-value `Cascade<FoldTiming>` overrides at
  the stage and member-entry scopes.
  Keep only raw progress in
  `FoldSequenceState` and apply the selected curve in `hinge_to_pose()`. Keep
  `FoldSequenceState` as the canonical transport and `FoldCommandEvent` as its
  request path. Give `FoldCommand` the variants `Step(FoldDirection)`,
  `PlayTo(FoldEndpoint)`, `Pause`, `Resume`, and `Reverse`; retain paused
  journeys and preserve raw position across reversal. Keep `FoldSequence` as
  a separate optional component colocated with `Arrangement`, with
  `FoldSequenceState` as its derived runtime state. Defer continuous external
  driving to a single-writer raw-progress adapter.
- **A9.4:** Remove `FoldFromArrangement` and its snapshot request,
  diagnostic, system, and validation machinery. Retarget
  `FoldSequenceBuilder` to accept explicit `FoldGroup` values. Do not infer
  sequence participation from membership or renumber stages when members
  change. Remove `MemberPlacement`, whose only
  additional value was the live hinge angle eliminated by A9.1. Replace
  `ArrangementPlacement` with the connected payload of the named A6 result;
  keep `AnchoredTo`, `Edge`, and one fixed connection `Angle` together and
  store only final endpoint `Angle` values on `Hinge`. Remove
  `FoldSystems::Actuate` with `actuate_fold_hinges()` and retain
  `FoldSystems::Advance` as the controller-transport boundary before
  `hinge_to_pose()`. Remove `FoldAngleInvalidReason`, `FoldAngleDiagnostic`, and
  `FoldAngleDiagnostics`; defer a unified retained hinge-and-anchor diagnostic
  policy, including non-finite `Angle` failures and warning throttling, to F13.
  Remove public `Hinge::rotation()` without replacement so
  `hinge_to_pose()` remains the only fold-angle evaluation path. Keep
  the offset-pivot capability, remove `HingePivot::reference_angle`, calibrate
  the offset at the unfolded endpoint, and merge it into
  `Hinge::pivot_offset`. Remove the `HingePivot` component. Declare `AnchorPose`
  as a required component of `Hinge`; ordinary `Hinge` removal leaves the pose
  available for another writer. Remove unused `AnchorPoseLens` and the
  resulting empty tween integration; retain direct purpose-built `AnchorPose`
  writers and the future raw fold-progress adapter. Reuse nonempty,
  authoring-only `FoldGroup` instead of adding `SharedFoldStage`; retain
  fallible runtime collection conversion and duplicate-member validation, and
  queue no commands before validation succeeds. The local simplification audit
  removes eleven public types and adds none; calculate the complete A9 concept
  count at its closure gate.
- **A9.5:** Remove `FoldMember` / `FoldMembers`. Use `Member` / `Members` as
  the one arrangement-membership relationship. Store stage participation in
  the `FoldGroup` owned by each sequence-owned `FoldStage`, with no public
  per-member stage component. Support one active `FoldSequence` per `Arrangement`
  until a demonstrated feature requires overlapping sequence assignments.
- **F6:** Use `PlayTo(FoldEndpoint)` for explicit, idempotent terminal travel.
- **F7:** Add pause and resume commands plus
  `FoldSequenceState::is_paused()`.
- **F8 (partial):** Select adjacent-boundary versus terminal travel per command
  through `Step` versus `PlayTo`; decide scalar transport reuse under F5.
- **F13 (required follow-up):** Review one coherent retained diagnostic policy
  for anchor and hinge failures after removal of the adapter-specific fold-angle
  diagnostic family.
- **F15 (review item):** After A1-A10 are settled, review pattern-oriented
  builders that intelligently materialize common setup patterns through the
  final arrangement types.

As further decisions are confirmed, this section will record each deliverable,
its dependencies, and its ordering. Later decisions may reorder accepted work
without changing what was approved.

## Scope and phased-plan handoff

In-scope and out-of-scope work will be selected after the API, feature, and
mutual-exclusion reviews are complete. This document will then be converted
into a phased implementation plan; phase structure is intentionally not being
authored during the decision review.

## Deferred and rejected work

No work has been deferred or rejected yet.
