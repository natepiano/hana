# Shared cascades

`bevy_kana` provides a reusable cascade abstraction for values that either
inherit from a lower-precedence scope or override it. The same `Cascade<T>` type
works in ordinary Rust data and, through `CascadePlugin<A>`, as ECS-authored
state propagated over an explicit relationship.

A consuming crate chooses its cascade attributes, root defaults, relationship
placement, typed authoring verbs, and consumers. `bevy_kana` owns relationship
traversal, dirty-subtree propagation, lifecycle handling, cycle protection, and
resolved-value caching.

## Authored values outside ECS

```rust
pub enum Cascade<T> {
    Inherit,
    Override(T),
}

pub fn resolve_cascade<T>(
    layers: impl IntoIterator<Item = Cascade<T>>,
    root: T,
) -> T;

pub fn resolve_cascade_ref<'a, T>(
    layers: impl IntoIterator<Item = &'a Cascade<T>>,
    root: &'a T,
) -> &'a T;
```

Layers are examined from highest to lowest precedence. The first override wins;
the required root value wins when every layer inherits.

`Cascade<T>` deliberately has no conversion to or from `Option<T>`. `Inherit`
is authored behavior, not missing data. Inspection methods such as
`as_override()` may still return `Option` because they query authored state
rather than construct it.

## ECS model

The ECS engine uses these public types:

```rust
pub struct CascadeFrom {
    target: Entity,
}

pub struct CascadeChildren(Vec<Entity>);

pub struct CascadeDefault<A: CascadeAttribute>(pub A);

pub struct Resolved<A: CascadeAttribute>(pub A);

pub struct CascadePlugin<A: CascadeAttribute> {
    root: A,
}

pub enum CascadeSet {
    Propagate,
}
```

`CascadeFrom` is an immutable Bevy relationship component. Bevy maintains its
reverse `CascadeChildren` collection. The relationship allows self-reference so
the resolver can diagnose cycles itself, and it does not use linked despawn.

`CascadeFrom` is independent of `ChildOf`. Transform ownership, despawn
ownership, domain membership, and cascade inheritance are separate facts. An
entity inherits only when a consumer explicitly gives it `CascadeFrom`.

One `CascadeFrom` relationship is shared by every cascade attribute on an
entity. Consequently, an entity cannot inherit different attributes from
different source entities.

For an attribute `A`, ECS state has three distinct forms:

- no `Cascade<A>`: the entity does not participate and has no maintained
  `Resolved<A>`;
- `Cascade::<A>::Inherit`: the entity participates and consults `CascadeFrom`,
  then the root default;
- `Cascade::Override(value)`: the entity participates and authors the winning
  local value.

An ancestor without `Cascade<A>` is transparent: resolution can continue
through its `CascadeFrom` relationship.

## Registration and authoring

Any type satisfying the blanket `CascadeAttribute` contract can be registered.
In practice, downstream attributes derive `Reflect` and implement the required
`Clone`, `PartialEq`, `Send`, and `Sync` bounds.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
struct Opacity(f32);

app.add_plugins(CascadePlugin::new(Opacity(1.0)));

commands.entity(parent).override_cascade(Opacity(0.5));
commands
    .entity(child)
    .insert(CascadeFrom::new(parent))
    .inherit_cascade::<Opacity>();
```

`CascadePlugin<A>`:

- installs `CascadeDefault<A>` unless the application already supplied one;
- registers the authored, resolved, and default types for reflection;
- observes inserted `Cascade<A>` components so their cache is seeded after
  associated commands apply;
- runs propagation in `Update` under `CascadeSet::Propagate`.

`CascadeEntityCommandsExt` provides:

```rust
fn set_cascade<A>(&mut self, authored: Cascade<A>) -> &mut Self;
fn override_cascade<A>(&mut self, value: A) -> &mut Self;
fn inherit_cascade<A>(&mut self) -> &mut Self;
fn remove_cascade<A>(&mut self) -> &mut Self;
```

`inherit_cascade` keeps the entity participating. `remove_cascade` stops
participation and removes both `Cascade<A>` and `Resolved<A>`.

## Resolution and propagation

For each participating entity, resolution proceeds as follows:

1. Use the entity's own `Cascade::Override`, if present.
2. Otherwise follow `CascadeFrom::target()`.
3. At each ancestor, use the first `Cascade::Override`; an absent component or
   `Cascade::Inherit` continues the walk.
4. Use `CascadeDefault<A>` when the walk reaches a root without finding an
   override.

The propagation system reacts to:

- insertion or mutation of `Cascade<A>`;
- removal of `Cascade<A>`;
- insertion, retargeting, or removal of `CascadeFrom`;
- mutation of `CascadeDefault<A>`.

It uses `CascadeChildren` to collect affected descendants. It writes
`Resolved<A>` only when the effective value changed, preserving meaningful Bevy
change detection for consumers.

Consumers that need same-frame results should author before
`CascadeSet::Propagate` and read after it:

```rust
fn apply_opacity(
    values: Query<&Resolved<Opacity>, Changed<Resolved<Opacity>>>,
) {
    // Consume only effective-value changes.
}
```

Two readers cover different needs:

```rust
pub fn resolved_cascade<A>(
    world: &World,
    entity: Entity,
) -> Option<&A>;

pub fn resolve_entity_cascade<A>(
    world: &World,
    entity: Entity,
) -> Option<A>;
```

`resolved_cascade` reads an existing maintained cache.
`resolve_entity_cascade` walks the current authored state directly and returns
`None` only when the attribute's root-default resource is not installed.

## Construction seeds

Consumer builders may retain domain values used to initialize a new entity.
Those values are construction seeds, not a second runtime authoring store.

A construction bridge inserts `Cascade<A>` only if the entity does not already
carry one. This preserves an explicit cascade command queued during spawning.
Once seeded, `Cascade<A>` is the live authored state; later changes to unrelated
domain components must not replay the builder seed.

`hana_diegetic` follows this rule in its panel and text bridges:

- `seed_panel_overrides` observes a newly added `DiegeticPanel` and seeds its
  cascade attributes without replacing explicit authored components;
- panel-label bridges use a label's `ChildOf` parent once to establish
  `CascadeFrom(panel)`, then insert each inheriting authored value only when the
  label has no explicit authoring for that attribute.

These bridges do not make `ChildOf` a cascade relationship. They convert known
panel structure into the explicit relationship at construction time.

## `hana_diegetic` integration

`hana_diegetic` owns the domain-facing layer:

- cascade attribute types and root defaults;
- public `override_*` and `inherit_*` entity commands;
- typed `resolved_*` readers;
- panel, text, material, and rendering bridges;
- systems that consume resolved values.

Its attributes include text alpha, font unit, HDR text coverage bias, lighting,
shadow casting, glyph shadow mode, sidedness, anti-aliasing, hairline fade, and
SDF, text, and shape material handles.

`Cascade<T>`, `CascadeFrom`, `Resolved<A>`, and `CascadePlugin<A>` remain
internal implementation details in `hana_diegetic`. The crate publicly exposes
its typed verbs and readers, plus the curated `CascadeDefault<A>` and
`CascadeSet` integration points.

## Key files

| File | Role |
| --- | --- |
| `crates/bevy_kana/src/cascade.rs` | Generic authored type, ECS relationship, plugin, propagation, readers, commands, and core tests |
| `crates/bevy_kana/src/lib.rs`, `src/prelude.rs` | Public exports |
| `crates/hana_diegetic/src/cascade/mod.rs` | Ownership boundary and selected public re-exports |
| `crates/hana_diegetic/src/cascade/attributes.rs` | Typed domain commands and resolved readers |
| `crates/hana_diegetic/src/cascade/resolved.rs` | Domain attribute types and root defaults |
| `crates/hana_diegetic/src/panel/diegetic_panel.rs` | One-time panel construction seeding |
| `crates/hana_diegetic/src/render/panel_text/alpha.rs`, `glyph_cascade.rs` | Panel-label cascade relationship bridges |
| `crates/bevy_kana/examples/cascade.rs` | Generic override, inheritance, root-default, and retargeting example |
| `crates/hana_diegetic/examples/text_cascade.rs` | Typed text-alpha and font-unit consumer example |

## Invariants

- `Cascade<A>` is the only live authored ECS value after construction.
- `Resolved<A>` is derived engine state and exists only for participating
  entities.
- `Cascade::Inherit` means participation; component absence means
  non-participation.
- `ChildOf` alone never creates cascade inheritance.
- Relationship and authored-state removals must update descendants.
- Equivalent resolved values must not be rewritten.
- Consumer construction bridges must preserve explicit cascade authoring.
- `hana_diegetic` must not expose `Cascade<T>` through its public API.
- Systems requiring current values must respect `CascadeSet::Propagate`.

## Calibration and failure behavior

`CASCADE_DEPTH_LIMIT` is `64`. A relationship cycle or a walk beyond that limit
emits a warning and resolves to the root default; resolution never hangs or
panics.

Because every attribute clones its winning value during propagation, cascade
attributes should remain modest in size. Handle-based attributes are
appropriate; large payloads should normally be stored elsewhere and cascaded by
handle or compact identity.

## Why this structure

The explicit relationship prevents visual inheritance from being accidentally
coupled to transform parenting or despawn ownership. Bevy's relationship target
supplies reverse traversal without a duplicate child index. A single authored
component plus a derived cache prevents builder state and runtime authoring from
competing. Change-guarded cache writes let render and layout systems safely use
`Changed<Resolved<A>>`, while explicit `Inherit` and `Override` states keep
public authoring intent visible in the type system.
