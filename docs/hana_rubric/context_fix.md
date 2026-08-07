# Context availability and canonical context example

## Objective

Make the active keymap context truthful when an application-owned context source disappears, clean
up runtime input state at that boundary, and tell developers when a required source is missing.
Specify one runnable Fairy Dust example and an accompanying addition to
`docs/fairy_dust/canonical-example.md` that demonstrate every supported user-visible context source.
Define the downstream completion boundary for the three intended application consumers: Fairy
Dust, Hana, and Nateroids.

The committed behavior is:

- `ActiveConditionState::ContextUnavailable` reports that a registered context source currently has
  no value.
- An unavailable context routes no keymap bindings. It does not fall back to the global matcher.
- Entering or leaving unavailability is a real routing transition, with the same physical-input
  safety guarantees as a compiled-generation or ordinary context change.
- Required sources warn once per unavailable episode. Sources declared optional do not warn.
- A total `DerivedContext` remains the recommended way to keep context-aware routing available
  continuously while combining several pieces of application state.
- Rubric owns one public, headless command-palette query model. Hana, Fairy Dust, Nateroids, and the
  canonical example use it so displayed bindings resolve through the same active matcher as
  keyboard input.

## Current behavior and failure

`sync_resource_condition` and `sync_state_condition` both receive their source as `Option<Res<_>>`.
When the resource is absent, they return without changing `ActiveCondition`. If a source previously
resolved, routing therefore continues through the last condition even though that source no longer
exists.

This is particularly easy to reach with Bevy `SubStates` and optional `ComputedStates`. Bevy removes
their `State<C>` resource when their source state no longer permits them. A binding scoped to the
last substate can consequently remain active after the application leaves its parent state.

Changing only `ActiveCondition` is insufficient. `route_input` currently returns before runtime
transition handling when the condition is awaiting a value. A partial sequence or physically held
command can therefore survive unless context loss also passes through runtime cleanup.

## Public state contract

Add `ContextUnavailable` to `ActiveConditionState`:

```rust
pub enum ActiveConditionState {
    AwaitingContext,
    ContextUnavailable,
    GlobalRouting,
    ResolvedCondition {
        handle: ConditionHandle,
        name: ConditionName,
    },
}
```

The variants have distinct meanings:

- `AwaitingContext`: a context source has been registered, but its synchronization system has not
  yet reported whether a value exists.
- `ContextUnavailable`: synchronization ran and the registered source had no value.
- `GlobalRouting`: the application registered no context source.
- `ResolvedCondition`: the source supplied a registered context value.

`ContextUnavailable` is reflected through the existing `ActiveCondition` resource so BRP,
diagnostic UI, and examples can state why no binding routes. It is not a JSONC condition name and
must not appear in the generated schema. Applications that want bindings outside a scoped state
must declare a real fallback context variant instead.

Every public state that names no usable matcher maps to one internal inactive matcher state before
`route_input` can return. This includes `AwaitingContext`, `ContextUnavailable`, and a reflected
`ResolvedCondition` carrying a handle the registry never issued. The public variants remain
distinct for explanation; the internal state gives all no-matcher transitions the same cleanup
guarantees. Source synchronization must repair reflected `ActiveCondition` replacement even when
the underlying source resource did not change that frame.

## Context-source contracts

Keep the current source methods strict and add an explicit optional state method:

| API | Source contract | Missing-source behavior |
| --- | --- | --- |
| `for_context::<C>()` | `Resource<C>` is application-owned and required | Enter `ContextUnavailable`; warn once |
| `for_state_context::<C>()` | `State<C>` is required and expected to be continuously present | Enter `ContextUnavailable`; warn once |
| `for_optional_state_context::<C>()` | `State<C>` may disappear, as with `SubStates` or optional `ComputedStates` | Enter `ContextUnavailable`; do not warn |
| `for_derived_context(DerivedContext<C>)` | Rubric owns a total `State<C>` selected by rules and a fallback | Its state remains present; unexpected removal uses required-source behavior |

Do not infer whether a `States` type is optional. `for_state_context` accepts any `C: States`, and
Rust cannot use overlapping blanket behavior to distinguish ordinary states, substates, and
computed states here. The method selected by the developer is the declaration of intent.

`for_optional_state_context` does not register the Bevy state or substate. The application remains
responsible for `init_state`, `add_sub_state`, or `add_computed_state`, just as it is for the strict
state method.

An optional resource method is not part of this change. An ordinary context resource should be a
total application-owned value. An application that needs optional resource facts should map them
into a total context enum, preferably with `DerivedContext`.

`for_derived_context` must reject a context type for which the application already registered Bevy
state machinery. `FreelyMutableState` alone does not prove exclusive ownership: `SubStates` also
implements it. Before inserting the derived state, reject a pre-existing
`Messages<StateTransitionEvent<C>>` resource. Bevy 0.19 registers that resource for ordinary,
substate, and computed-state machinery even when an optional state's `State<C>` is currently
absent. Retain a context-registration failure that names the type and do not install the derived
evaluator. A derived context enum may derive `States`, but application code must not also pass that
type to `init_state`, `add_sub_state`, or `add_computed_state`.

## Developer warning

Required resource and state sources emit one developer-facing warning when synchronization first
observes that the source is unavailable. The warning is edge-triggered: continued absence produces
no additional log entries, and resolving the source rearms the warning for a later unavailable
episode.

The message names the context type, says that routing is disabled, and gives actionable remedies:

> State-backed keymap context `app::InputContext` is unavailable, so keymap routing is disabled.
> Initialize it with `init_state`, register its substate, use `DerivedContext` with a fallback, or
> select `for_optional_state_context` if absence is intentional.

Use equivalent resource-specific wording for `for_context`. Optional state absence is normal
control flow and emits no warning. `ContextUnavailable` itself remains observable in every mode.

Do not append an unavailable warning permanently to retained load failures. A recovered source
must not leave the command palette claiming that the current context is still unavailable. The
reflected state communicates current availability; the edge-triggered log communicates probable
developer error.

Persistent absence is a steady state. After the first transition, synchronization must not format
another warning, allocate, acquire mutable `ActiveCondition` access, or advance its change tick.
Use a private availability state to latch required-source warning episodes and rearm the warning on
recovery. Tests need an observable, test-only log capture mechanism; do not add warning counters or
test seams to the public runtime resources.

Split read-only transition detection from mutation so the synchronization system does not declare
`ResMut<ActiveCondition>` access on every steady frame. The mutating body runs only when source
presence or value changed, or when `ActiveCondition` changed and may need reflected-state repair.
Steady absence performs only read-only presence and change-tick checks and does not unnecessarily
serialize other readers of `ActiveCondition`.

## No-matcher runtime transition

Represent every no-matcher condition in `KeymapRuntime` as an inactive matcher state. Do not
manufacture an `ActiveKeymapScope`, because no matcher is valid while the source is awaiting a
value, unavailable, or reflected with an invalid handle.

On the first routing pass after entering the internal inactive state:

1. Cancel pending sequences in the previously active matcher without firing a deferred command.
2. Release every physical held source and publish the resulting `CustomInput` transitions.
3. Preserve semantic-event held sources; they have independent ownership and are not physical
   keyboard state.
4. Record the runtime matcher as unavailable.
5. Inhibit every key still physically pressed.
6. Return without matching or dispatching a binding.

Subsequent inactive frames do no transition work and route nothing. When a context becomes
available again, the ordinary matcher transition runs. Keys that remained pressed across the
boundary stay inhibited until released, so reappearance cannot synthesize a fresh press.

The same transition applies whether the source is absent on the first synchronization pass or
disappears after resolving. `AwaitingContext` may exist only before that first pass and routes
nothing.

## Implementation shape

- Give `ResourceContextPlugin` and `StateContextPlugin` an internal required/optional policy. The
  existing builders select required; `for_optional_state_context` selects optional.
- Generalize source synchronization so `Some(value)` resolves the condition and `None` records
  `ContextUnavailable`. It must compare the desired condition with `ActiveCondition` before taking
  mutable access, and it must resolve a present source whenever the public state does not already
  agree even if `Res::is_changed` is false. Keep repeated writes allocation-free and edge-trigger
  warning state private.
- Extend the internal active-matcher representation with an unavailable state and route it through
  the same reset transaction used by generation and condition changes. Map every public no-matcher
  state to this internal state before any early return.
- Make `evaluate_derived_context` use fallible access to its owned state resources. Missing
  `State<C>` must allow required-source synchronization to publish `ContextUnavailable` instead of
  panicking. Reinitialize a missing `NextState<C>` because Rubric owns it, but do not silently
  recreate a removed `State<C>` before the unavailable state can be observed and warned about.
- Reject pre-existing Bevy state registration for a derived context type before installing the
  Rubric-owned producer.
- Keep context registration singular. Installing any second resource, state, optional-state, or
  derived source remains an error.
- Preserve the effective global matcher and each effective condition matcher in `KeymapBindings`
  as immutable command lookup tables built when a keymap generation commits. Palette lookup reads
  those tables; it does not rebuild them when context changes and adds no work to input routing.
- Do not retain a second full clone of every inherited global command id and keystroke for every
  condition. Store global results once and represent condition differences sparsely, including an
  explicit unbound difference for tombstones, or use compact handles into shared command and
  sequence storage. Retained payload storage scales with globals plus condition differences rather
  than conditions multiplied by inherited globals.
- Add the shared headless palette query model described below. Move the duplicated title/id search,
  capability filtering, selection, rejection, ordering, and row-binding resolution out of Hana and
  Fairy Dust rather than adding a Fairy Dust-only context lookup.
- Derive `CompiledKeymap` and `KeymapBindings` from the same validated `MergedKeymap` and publish
  both in one exclusive commit transaction. A rejected reload replaces neither resource, so palette
  lookup and dispatch can never observe different keymap generations.
- Update public rustdoc, the prelude exports if a new public policy type becomes necessary, and the
  context portion of `docs/hana_rubric/as-built/v1.md` after implementation.

Prefer the dedicated builder method over a public boolean or policy enum. The call site should read
as a declaration that absence is expected, and ordinary users should not need to import a policy
type.

## Shared headless command-palette contract

Add a public, renderer-independent palette query API to `hana_rubric`. It is the single behavioral
source for Hana's palette, Fairy Dust's palette, Nateroids' palette, and the canonical context
example. It depends only on Rubric domain types and must not depend on `fairy_dust`,
`hana_diegetic`, Egui, panel trees, IME events, window focus, or application-specific diagnostics.

Expose one canonical query operation at the crate root rather than separate public matching,
selection, and binding helpers:

```rust
let result = query_command_palette(
    &command_registry,
    &keymap_bindings,
    &active_condition,
    query,
);

for row in result.rows() {
    // Render borrowed command metadata and row.binding().
}
match result.selection() {
    // Dispatch the selected palette command or render a semantic rejection.
}
```

The exact type names may follow established crate naming during implementation, but the public
shape must contain one borrowed query-result type, one borrowed row type, one opaque
palette-invocable command view, an exhaustive binding outcome, and a selection/rejection outcome.
The result lifetime borrows `CommandRegistry`, `KeymapBindings`, and `ActiveCondition`; it cannot be
stored as a long-lived Bevy resource or outlive a change to any input. Rows borrow command metadata
and `KeystrokeSequence` values, and consumers format strings only at the rendering boundary. A
public generation identity is unnecessary.

The exhaustive row binding outcome represents these states as mutually exclusive variants:

```rust
pub enum PaletteBinding<'keymap> {
    BoundTo(&'keymap KeystrokeSequence),
    Unbound,
    AwaitingContext,
    ContextUnavailable,
    InvalidConditionHandle,
}
```

Do not combine `CommandKeystroke` with a separate availability flag; that would permit a row to be
both bound and routing-unavailable. Keep the existing context-independent
`KeymapBindings::keystroke` only for source compatibility and document that it is not an
active-context display API. Contextual consumers must use the shared query. Root-export the shared
palette API, but do not add these specialized types to the general prelude and do not introduce a
palette trait.

The API returns typed query results containing:

- palette-invocable command metadata from `CommandRegistry`;
- normalized title/id matches for the current query;
- one deterministic command order, with authored title followed by command id as the tie-breaker;
- the selected opaque palette command view, or a semantic rejection distinguishing an empty query,
  no match, and matches that are all not palette-invocable;
- each row's binding outcome, distinguishing a resolved keystroke, an actually unbound command,
  and routing being unavailable because the context is awaiting synchronization, unavailable, or
  carries an invalid reflected condition handle.

The opaque command view has private construction and exposes the id, title, and description. Rubric
constructs it only after `Capability::is_palette_invocable` succeeds, so neither a row nor a
selected state can carry a held command. The shared API owns semantic states, not UI prose.
Consumers decide how to label rows and rejections, but they cannot recover a keystroke from a
routing-unavailable result or mistake it for an authored unbound command. A direct palette
invocation remains registry-wide: a one-shot or unremappable command may be invoked while keyboard
routing is unavailable. Held commands remain visible only to rejection diagnosis and are never
returned as invocable rows.

Build an immutable palette search index when `CommandRegistry` initializes. It retains normalized
title and id search text plus authored-title/id order once per command. A query normalizes its input
once and walks that preordered index once to produce both rows and selection or rejection; it does
not sort commands, renormalize registry metadata, clone row strings, or independently repeat the
search for selection.

Selection is derived from the returned row order:

- a normalized-empty query lists all invocable rows and returns `EmptyQuery`;
- a nonempty query with any invocable match selects its first row in authored-title/id order;
- `NotPaletteInvocable` means at least one declared command matched and every match was held;
- `NoMatch` means no declared command matched.

Duplicate titles use command id as the deterministic tie-breaker. A mixed held and invocable match
selects the first invocable row rather than rejecting because a held command also matched.

Binding lookup mirrors the compiled matcher exactly:

- `GlobalRouting` queries the effective global matcher.
- A valid `ResolvedCondition` queries that condition's effective matcher, including inherited
  globals, conditioned overrides, and tombstones.
- `AwaitingContext`, `ContextUnavailable`, and an invalid reflected condition handle return the
  corresponding routing-unavailable outcome for every row and no displayed keystroke.

One effective matcher may bind several keystroke sequences to the same command. The displayed
representative is the active sequence with the fewest keystrokes, then the least sequence under a
platform-independent structural order over modifiers and ordinary keys. Select it while building
the immutable command index at generation commit. Never select by hash-map iteration or formatted
platform text.

The query is a pure read over `CommandRegistry`, `KeymapBindings`, and `ActiveCondition`. Rubric
does not install a palette system or cache UI state. Building the immutable per-matcher command
indexes happens only when the committed generation replaces `KeymapBindings`; changing context
selects an existing table.

Each UI consumer retains the current committed query text on its palette entity. While a palette is
open, one consumer-owned `Update` system reruns the shared query when `KeymapBindings` or
`ActiveCondition` changed, even if no `ImeTextChanged` event occurred. `Update` runs after Rubric's
`PreUpdate` context synchronization and reload commit. Resource change ticks are the generation
signal; do not expose Rubric's private generation or read `CompiledKeymap` from the UI. A steady
frame performs only the two resource change checks and open-panel query and does not replace the
panel tree.

Move the existing private query behavior from
`crates/fairy_dust/src/command_palette/query.rs` and the Hana repository's `feature/rubric` branch
at `crates/hana/src/tool/command_palette/query.rs` behind this API. The Hana path is cross-repository
and is not a member of this workspace. Its existing registry-driven consumer is the migration
target; do not build a third implementation from the older hard-coded palette on Hana `main`.
Land the Rubric API first, update both consumers against it, and preserve the sibling path dependency
until Hana's feature branch is merged. The consumers continue to own:

- panel layout and styling;
- IME session identity, authority, acceptance, and rejection events;
- keyboard ownership while the query field is active;
- keymap failure rows and application-specific feedback;
- dispatching the selected id through `CommandRegistry::invoke`.

Rubric tests cover normalization, deterministic ordering, capability filtering, rejection states,
and binding results for global routing, inherited globals, conditioned overrides, tombstones,
unbound commands, all three unavailable reasons, multiply-bound commands built in different orders,
and context changes without a keymap rebuild. Reload tests prove successful replacement publishes
dispatch and palette data together and every rejected reload leaves both unchanged.

Hana and Fairy Dust retain only consumer tests proving that their UI passes the current resources to
the shared query, renders each typed result correctly, and invokes the selected id. Their refresh
tests keep query text fixed, change context and replace `KeymapBindings` without an IME text event,
and verify that the open palette updates once. A steady-frame test proves that neither consumer
replaces its panel tree again. The canonical example exercises this behavior through Fairy Dust's
palette rather than installing a second query consumer. Nateroids consumes the same query directly
from its Egui renderer; it must not fork Fairy Dust's panel code or restore a private query module.

## Downstream adoption and delivery scope

This document is the implementation contract for the context fix and the application work needed
to prove it. The live repositories currently stand at different starting points:

| Consumer | Current integration | Completion required by this document |
| --- | --- | --- |
| Fairy Dust | Rubric is installed; its command palette has a private query implementation | Move the query to Rubric's shared API and ship the canonical four-source context example |
| Hana | The `feature/rubric` worktree has the registry-driven palette; older `main` still has the hard-coded palette | Move the feature worktree to the shared query, validate it against Rubric, and merge that integration through Hana's normal branch process |
| Nateroids | Uses hard-coded `bevy_kana` and `bevy_enhanced_input` bindings; it has no Rubric dependency or command palette | Migrate its remappable commands and held controls to Rubric, install a total derived context, and add an Egui consumer of the shared palette query |

The work is not complete when only `hana_rubric` and the Fairy Dust example are green. Completion
requires the focused consumer tests below in all three repositories. Publication, dependency
pinning, branch integration, and commits remain execution steps rather than choices silently made
by this design document.

### Existing consumer completion

Fairy Dust removes `crates/fairy_dust/src/command_palette/query.rs` after moving its behavior behind
the shared Rubric query. Its existing palette remains the UI implementation used by the canonical
example, including keyboard ownership, failure rows, repair actions, and selected-command
invocation. The canonical example must use Fairy Dust's actual palette rather than a test-only or
example-local renderer.

Hana changes only the existing registry-driven implementation in the `feature/rubric` worktree.
Besides removing `crates/hana/src/tool/command_palette/query.rs`, update the hand-written reflected
`ActiveConditionState` decoder in `crates/hana/src/input/interaction_context.rs` to recognize
`ContextUnavailable` as a distinct state. Its BRP tests must prove all four public state shapes can
be decoded without treating unavailability as an unknown reflection payload. Hana's existing
failure repair actions, protected recovery chord, command BRP invocation, derived
`InteractionContext`, and held-command adapters remain part of the integration gate.

### Nateroids production-consumer specification

Nateroids is the third application consumer and the renderer-independence proving ground. Add
`hana_rubric` at a Bevy-compatible revision and add the public Strum derive dependency. Use the
Rubric version or sibling checkout that is being validated by Fairy Dust and Hana; do not
temporarily copy Rubric source or palette-query logic into Nateroids.

#### Total application context

Declare one application-owned context enum with the authored variants `launch`, `splash`,
`playing`, `paused`, and `game_over`. It derives `States`, `AsRefStr`, `EnumIter`, `EnumMessage`, and
the ordinary copy/equality traits required by `KeymapContext`; every variant supplies a nonempty
Strum message.

Install it through `for_derived_context` with `launch` as the conservative fallback. Its ordered
rules read Nateroids' existing `State<GameState>` and optional `State<PauseState>` and select:

- the `launch` fallback for `GameState::Launch` or a transiently incomplete state pair;
- `splash` for `GameState::Splash`;
- `playing` for `GameState::InGame` plus `PauseState::Playing`;
- `paused` for `GameState::InGame` plus `PauseState::Paused`;
- `game_over` for `GameState::GameOver`.

The evaluator must be total for every reachable world snapshot, including the transition frame in
which `GameState::InGame` exists before its substate resource appears. Select a conservative
fallback and order the `InGame` rules so a transiently absent `PauseState` cannot expose gameplay
bindings. A test must enumerate reachable game-state and pause-state combinations and prove the
Rubric-owned context never normally reaches `ContextUnavailable`.

Do not install `PauseState` itself with `for_optional_state_context`. Its absence in `Launch`,
`Splash`, and `GameOver` would disable all keymap routing, including global commands and the
permanent recovery chord. The optional-state API remains correct for an application that wants
that behavior; it is not Nateroids' whole-application context.

#### Command and binding migration

Replace the physical bindings in `src/input/global_shortcuts.rs` and
`src/input/ship_controls.rs` with application-owned `command!` declarations and one embedded JSONC
default keymap. Preserve the existing semantic command/event boundary so BRP and behavior tests can
continue to trigger app events without synthesizing keyboard input.

The migration covers all user-facing actions currently declared there:

- the camera, zoom, restart, escape, AABB, physics, focus, and inspector commands;
- `ShipAccelerate`, `ShipTurnLeft`, `ShipTurnRight`, and `ShipFire` as held commands;
- `ShipContinuousFire` as a one-shot command.

Global commands remain global only when their existing behavior is meaningful in every game state.
Restart commands and ship commands are conditioned to the appropriate `playing` and `paused`
contexts. Preserve current paused behavior first; any change to whether ship controls remain
available while paused is a separate product decision, not an accidental consequence of the
migration.

Held ship commands use `CommandRegistry::held_command_lookup` and bind the returned `CustomInput`
inside Nateroids' existing app-owned `Actions<ShipControlsContext>`. Existing systems that read
`Action<ShipAccelerate>`, `Action<ShipTurnLeft>`, `Action<ShipTurnRight>`, and `Action<ShipFire>` may
therefore keep their `TriggerState` interface. Physical keys exist only in the JSONC keymap after
migration; the enhanced-input context must not own a second hard-coded copy.

Remove `ShipShiftModifier` and `ModifySelection` if Rubric's exact modifier matching fully replaces
their internal gating role. If an internal action is still necessary, it remains unregistered and
unbound by the user. In either case preserve the existing Shift+F collision behavior: pressing
Shift+F invokes the shifted command, releasing Shift while F remains down does not toggle
continuous fire, and a later clean F press does. Preserve `require_reset` behavior so keys held
while gameplay becomes active do not synthesize ship input.

The default JSONC file is the sole authored inventory of default physical bindings. It includes
every migrated command exactly where the context table intends it to work. A registry/default
coverage test fails if a remappable command is missing from the defaults or a default names an
unknown command.

#### Egui palette and recovery path

Add an application-owned Egui command palette. Its search, order, selection, rejection, and active
binding values come only from Rubric's shared headless query. The Egui layer owns layout, focus,
labels, and invocation through `CommandRegistry::invoke`; it must not duplicate normalization,
capability filtering, or contextual binding lookup.

Declare `OpenCommandPalette` as unremappable and give it a Rust-owned, platform-appropriate
Cmd/Ctrl+P recovery chord reserved with `with_protected_keystroke`. The recovery system reads
physical input directly, cancels pending Rubric sequences, and remains able to open the palette
when the user keymap is invalid or context routing is unavailable. The palette's text field claims
keyboard ownership through `KeystrokeRouting::take_for_text_entry`, exempts only the palette-open
command if needed, and releases the same lease when the field or palette closes.

Nateroids retains and displays `KeymapLoadFailures`. From the palette, the user can identify the
failing file and reach app-owned repair actions for the user keymap, embedded defaults, and keymap
directory. An invalid edit must leave the last valid generation active and must not make the only
recovery surface inaccessible.

While the palette is open, a context transition or successful keymap replacement refreshes its
rows without requiring another text edit. `ContextUnavailable` and `AwaitingContext` clear row
keystrokes and are rendered as routing states, not as if every command were authored unbound.

#### Nateroids acceptance criteria

- Nateroids has no hard-coded physical binding for any user-remappable command after migration.
- Every declared command is reflected into `CommandRegistry`, every remappable default command is
  represented in the embedded defaults, and invocation uses the same semantic event as keyboard
  input and BRP tests.
- The total derived context reports the expected authored condition for all reachable `GameState`
  and `PauseState` combinations and does not disable global routing during splash or game over.
- Held ship actions preserve begin, active, and release behavior across context changes; a key held
  across activation remains inhibited until release and a fresh press.
- The existing Shift+F regression cases pass through Rubric-authored bindings.
- Escape still closes active inspectors before toggling pause, and restart variants preserve their
  existing state transitions.
- The Egui palette displays the active contextual representative binding, refreshes on context and
  generation changes, and invokes the selected command from the visible order.
- The protected recovery chord opens the palette with an invalid user keymap and while routing is
  unavailable; entering text does not leak keystrokes to gameplay.
- Focused Nateroids validation includes `cargo +nightly fmt --all`, `cargo check`, `cargo clippy`,
  and `cargo nextest run` under the repository's normal command policy.

### Delivery order and final gate

Implement and validate in dependency order:

1. Land the context-availability behavior and shared headless palette API in `hana_rubric`.
2. Move Fairy Dust to the shared API and add the canonical four-source example and guide.
3. Update and validate Hana's `feature/rubric` worktree, including its reflected-state decoder.
4. Migrate Nateroids against the settled API and use its Egui palette as the independent-renderer
   proof.
5. Run focused validation in every changed repository, then resolve publication or dependency
   pins and integrate the consumer branches through their normal guarded workflows.

The final evidence is a three-consumer matrix recording the Rubric revision each consumer tested,
the exact focused commands run, and their results. A green Rubric workspace alone is not the final
gate.

## Verification

Replace the test that asserts a missing state leaves the previous condition selected. Add coverage
for:

- a required resource missing on the first synchronization pass;
- a required resource disappearing after resolution;
- a required state missing initially and disappearing after resolution;
- an actual `SubStates` context entering and leaving its parent state through
  `for_optional_state_context`;
- no warning for optional absence and one warning per unavailable episode for required sources;
- reflected `ActiveConditionState::ContextUnavailable` carrying no stale condition handle or name;
- a pending multi-stroke sequence canceled on context loss;
- a physical held command released on context loss;
- a semantic-event held source preserved on context loss;
- a key held across disappearance and reappearance inhibited until release and a new press;
- a modifier pressed before the first routing pass while the source is already absent, followed by
  source recovery while it remains down, with no activation until release and a fresh press;
- a deferred one-shot prefix such as `g` and a longer `g h` sequence both suppressed when the
  timeout and context loss occur on the same routing pass;
- a bare-modifier held binding released on loss, kept inactive across repeated recovery frames
  while the modifier remains down, and activated only after release and a new press;
- no dispatch while unavailable and correct contextual dispatch after recovery;
- source-conflict checks covering the new plugin shape;
- reflected transitions from a resolved condition to both `AwaitingContext` and an invalid
  `ResolvedCondition` handle using the inactive cleanup path;
- a derived context state removed after resolution without a panic, followed by explicit recovery;
- rejection when the same type already has ordinary, substate, or computed-state machinery before
  `for_derived_context` is installed;
- warmed steady-absence tests proving no allocation, warning, `ActiveCondition` change tick, reset,
  or repeated `CustomInput` transition after the first inactive frame;
- private instrumentation proving the mutating synchronization body does not run during warmed
  absence but does run after recovery, removal, and reflected `ActiveCondition` replacement;
- schema and default-reference output excluding `ContextUnavailable`.

At least one chained ECS test must exercise the scheduled public-plugin path rather than mutating
`ActiveCondition` directly: resolve a context, start a sequence and physical hold, remove the source
or leave a real substate parent, run the application update, and verify reflection, cancellation,
physical release, semantic-source preservation, and no dispatch. Recover while a key remains down
and verify inhibition through release and a fresh press.

Run the crate's complete Rust validation after implementation, including `cargo +nightly fmt --all`,
`cargo check`, `cargo clippy`, and `cargo nextest run` using the repository's normal scoped commands.

## Canonical Fairy Dust context example specification

### Documentation outcome

Add a section named `Context-aware keymaps` to `docs/fairy_dust/canonical-example.md`. It points to
a runnable `crates/fairy_dust/examples/keymap_contexts.rs` and explains that an application installs
exactly one context source. The example therefore selects one source at process launch rather than
pretending source types can be changed while the app runs.

The guide gives these exact launch forms:

```text
cargo run -p fairy_dust --example keymap_contexts -- resource
cargo run -p fairy_dust --example keymap_contexts -- state
cargo run -p fairy_dust --example keymap_contexts -- optional-state
cargo run -p fairy_dust --example keymap_contexts -- derived
```

The section briefly tells developers when to choose each mode:

- `resource`: the application already owns a total context resource.
- `state`: an ordinary, continuously present Bevy state is the context.
- `optional-state`: a substate or computed state intentionally disappears; routing pauses while it
  is absent.
- `derived`: ordered current-world predicates select a total context with a fallback; this is the
  recommended composition path.

### Runnable example structure

The example follows the existing canonical Fairy Dust builder guidance and keeps its primary API
near the top of the file. Define four short installation functions adjacent to the context enums:

- `install_resource_context`
- `install_state_context`
- `install_optional_state_context`
- `install_derived_context`

`main` parses one required source argument, constructs the common Fairy Dust scene and keymap
configuration, obtains `&mut App` through the canonical `app_mut()` handoff, and calls exactly one
installation function. An absent or unknown argument prints the accepted values and exits without
opening a window.

Every mode uses its own context enum because each installation demonstrates the real trait bounds
and setup call. Each enum declares the same authorable names, `resting` and `active`. Every variant
has a nonempty `#[strum(message = "...")]`; deriving `EnumMessage` without those messages fails
context registration. Keeping the authored names the same lets all modes share one embedded JSONC
keymap.

The example declarations must be complete enough to copy:

- resource mode derives `Resource`, `AsRefStr`, `EnumIter`, `EnumMessage`, `Copy`, `Clone`, `Eq`, and
  `PartialEq`, then inserts its initial value;
- state mode derives `States`, `Default`, the Strum traits, and the ordinary state traits, marks one
  variant `#[default]`, then calls `init_state`;
- optional-state mode defines a `States + Default` parent and a `SubStates + Default` child with a
  concrete `#[source(ParentState = ParentState::Enabled)]` attribute and default child variant,
  then registers both in dependency order;
- derived mode derives `States`, `Default`, and the Strum traits but does not separately call
  `init_state`, `add_sub_state`, or `add_computed_state` for that context type.

The optional-state mode additionally defines a parent `States` enum and a child `SubStates` enum.
It uses `add_sub_state` and `for_optional_state_context`; leaving the parent removes the child's
`State<C>` resource and visibly produces `ContextUnavailable`.

The derived mode uses `DerivedContext::new(resting).when(active, condition)`. Its condition reads a
current-world marker or resource, not `Changed<T>`, `Added<T>`, a reader, or `Local<T>` history.

### Keymap and command behavior

Add `crates/fairy_dust/examples/keymap_contexts.keymap.jsonc`. It includes Fairy Dust's baseline
commands required by a custom `CommandPaletteKeymap`, plus example commands that make matcher
selection visible:

- one global command that remains inherited by both resolved contexts;
- one key whose command differs between `resting` and `active`;
- one contextual command with no binding outside its named context.

Commands update a small reflected example resource recording the last dispatched command and a
dispatch count. They also update an obvious scene property, such as the canonical cube's material
or rotation direction, so behavior is visible without opening diagnostics.

Source-manipulation controls must not depend on the contextual keymap they are changing. Register
them through Fairy Dust's permanent example shortcut facility or clickable UI so the user can:

- toggle the resource value in resource mode;
- request the other Bevy state in state mode;
- enter and leave the optional substate's parent and change its child while present;
- add and remove the current-world fact that selects the derived context.

In optional-state mode, the recovery control must continue working while rubric routing is
disabled. This is an acceptance requirement, not an implementation detail.

### Visible status

The running example presents a continuously updated panel containing:

- selected source mode;
- required or optional contract;
- source value, or `absent`;
- reflected `ActiveConditionState`;
- active authorable condition name when resolved;
- whether keymap routing is enabled;
- last dispatched command and dispatch count;
- the restart command for each of the other source modes.

The title bar lists only fixed source-manipulation controls and stable help text; it must not claim
that a static chip reflects a contextual or user-remapped key. The status panel is always the
authoritative explanation of source and routing state. The command palette uses Rubric's shared
headless query and must show the same effective contextual binding. When optional state is absent,
the status panel must say `ContextUnavailable — keymap routing disabled`; palette rows must clear
their keystrokes and expose routing unavailability rather than describing the commands as authored
unbound. Neither surface may display the last condition as though it were current.

### Fairy Dust integration constraint

Fairy Dust installs its baseline `KeymapPlugin` at builder execution. The example's context plugin
is a different concrete plugin type, so both configurations must carry identical defaults,
application name, and protected keystrokes. Configure `CommandPaletteKeymap` and the explicitly
installed context plugin from shared constants to satisfy rubric's single-configuration guard.

Do not add a Fairy Dust context-builder abstraction as part of this example. The owning API being
demonstrated is `hana_rubric`; its `KeymapPlugin` calls must remain directly copyable into an
application that does not use Fairy Dust.

### Example acceptance criteria

- All four launch forms compile and open the same canonical scene.
- Each installation function contains the actual public rubric call it teaches.
- The status panel agrees with `ActiveCondition` after every source transition.
- `resource`, `state`, and `derived` always resolve to either `resting` or `active`.
- `optional-state` visibly enters `ContextUnavailable` when its parent is left, routes no keymap
  command there, and recovers without a stuck hold or synthetic key press.
- Required modes produce no unavailable warning during normal operation; optional absence produces
  no warning by contract.
- Global and context-specific bindings dispatch exactly as the status panel reports; the palette
  agrees through Rubric's shared headless query.
- The example does not install more than one context source or use separate plugin configurations.
- The canonical-example guide identifies `DerivedContext` as the recommended total composition
  path and optional state as the deliberate no-routing path.
- Add an explicit `[[example]]` target for `keymap_contexts` with `test = true`. The example passes
  `cargo check -p fairy_dust --example keymap_contexts` and
  `cargo nextest run -p fairy_dust --example keymap_contexts`; its test module exercises all four
  launch modes without creating a window.

## Non-goals

- Supporting more than one simultaneous context source.
- Treating `ContextUnavailable` as an authorable JSONC condition.
- Falling back to global routing when a registered context disappears.
- Automatically registering application states, substates, or computed states.
- Inferring optionality from Bevy state traits.
- Switching context-source implementations at runtime.
- Adding a new Fairy Dust abstraction around rubric context installation.
- Rendering a command palette, owning an IME session, or moving application diagnostics into
  `hana_rubric`.

## Team review record

Cycle 1 auto-recorded these converged refinements:

- F1 accepted — all public no-matcher states use the cleanup transition, not only
  `ContextUnavailable`.
- F2 accepted — steady absence neither mutates `ActiveCondition` nor repeats warning or routing
  work.
- F3 accepted — derived evaluation uses fallible state access and cannot panic before availability
  synchronization.
- F4 accepted — Rubric-owned derived state rejects a pre-existing Bevy state producer for the same
  type.
- F5 accepted — verification names the deferred-prefix, bare-modifier, reflected-replacement, and
  scheduled source-loss branches that could otherwise escape broad tests.
- F6 accepted — the canonical example specifies compile-complete context declarations, including
  Strum messages and substate source/default attributes.
- F7 accepted — warning cardinality is verified through test-only log capture rather than a public
  runtime counter.
- F8 accepted — the example is an explicit test-enabled Cargo target so its focused ECS tests run
  under `cargo nextest`.

Cycle 1 summary: 8 mechanical and determined-correctness refinements recorded, 1 proposed user
decision. D1 was accepted after comparing Zed's context-aware binding lookup and finding duplicated
private palette-query implementations in Hana and Fairy Dust.

The shared-palette delta review auto-recorded these converged refinements:

- F9 accepted — one borrowed, exhaustive public query result is the canonical call; it prevents
  impossible bound-plus-unavailable states and avoids cloned row strings.
- F10 accepted — selection comes from the first visible invocable row in shared title/id order, and
  mixed held/invocable matches cannot produce a false rejection.
- F11 accepted — multiply-bound commands select one stable representative by sequence length and
  structural order rather than hash iteration or platform-formatted text.
- F12 accepted — each consumer retains query text and refreshes an open palette on
  `ActiveCondition` or `KeymapBindings` changes without requiring another IME edit.
- F13 accepted — dispatch and palette lookup data publish from the same validated generation in one
  transaction; rejected reloads replace neither.
- F14 accepted — normalized search metadata and title/id order compile once in `CommandRegistry`,
  and each query performs one normalized, ordered pass for rows and selection.
- F15 accepted — retained per-condition palette storage reuses globals or compact payloads instead
  of adding another full clone of inherited bindings for every condition.
- F16 accepted — steady source absence uses read-only transition detection and does not declare
  unconditional mutable `ActiveCondition` access.
- F17 accepted — first-pass unavailability inhibits keys that were already down before any matcher
  became active.
- F18 accepted — Bevy 0.19's `Messages<StateTransitionEvent<C>>` is the common registration marker
  for ordinary, substate, and computed-state ownership checks.
- F19 accepted — the Hana migration targets the existing registry-driven palette on the
  cross-repository `feature/rubric` branch, not a new copy based on older Hana `main`.

Shared-palette delta summary: 11 converged refinements recorded, 0 proposed decisions. All four
review agents produced complete findings; the external wrapper reported the same post-extraction
shell syntax error for each run, so synthesis used the intact findings files.

## Proposed user decisions

### D1 — Context-aware command-palette bindings

- **Status:** accepted 2026-08-07
- **Severity:** important
- **Source dimensions:** Rust API and Fairy Dust example usability
- **Class:** design-improvement
- **Problem:** `KeymapBindings` currently collapses global and condition-scoped bindings into one
  command-to-keystroke table. The command palette therefore cannot show which keystroke runs a
  command in the currently resolved context, and it cannot clear the displayed keystroke while
  `ContextUnavailable`. Requiring the canonical example's palette to agree with live routing is not
  implementable on the current API.
- **Decision:** Rubric owns immutable effective global and per-condition binding tables plus a
  public headless palette query. Hana, Fairy Dust, Nateroids, and the canonical example consume the
  same typed matches, selection, rejection, and context-aware binding outcomes.
  Routing-unavailable outcomes remain distinct from authored unbound commands. Palette invocation
  remains registry-wide and independent of keyboard routing.
