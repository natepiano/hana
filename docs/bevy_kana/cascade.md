# Shared cascades

## Status

As-built. Implemented 2026-07-15 before the planned `hana_diegetic` widget
work.

## Purpose

`bevy_kana` owns a reusable cascade engine. A consuming crate chooses which
values cascade, which entities participate, each root default, its public
domain verbs, and how resolved values are used. It does not implement hierarchy
walking, dirty-subtree propagation, resolved-value caching, or lifecycle
handling.

The engine supports two related uses:

- `Cascade<T>` is the existing storage-independent authored value used by
  ordinary structs such as a future fold sequence, stage, and member entry.
- The ECS engine propagates attributes over its own explicit relationship and
  maintains a resolved value on participating entities.

## Ownership boundary

`bevy_kana` owns:

- `Cascade<T>`;
- the dedicated cascade relationship and its Bevy-maintained reverse
  collection;
- the generic attribute contract, root-default resource, resolved cache,
  plugin, system set, commands, and readers;
- initial resolution, change detection, dirty-subtree collection, reparenting
  and removal handling, cycle/depth protection, and change-guarded cache
  writes.

`hana_diegetic` owns:

- attributes such as `ShadowCasting`, `TextAlpha`, and material handles;
- the decision to connect two entities through the cascade relationship;
- domain methods such as `override_shadow_casting`,
  `inherit_shadow_casting`, and `resolved_shadow_casting`;
- panel-, text-, and widget-specific construction bridges;
- systems that consume resolved values.

`hana_diegetic` must not re-export `Cascade<T>` or return it from public fields,
arguments, or methods. Existing public global-default and scheduling APIs may
remain available as curated facades over the shared engine. Raw authored and
resolved ECS storage remains internal to the consuming crate's implementation.

## ECS model

Use one dedicated relationship for all shared cascade attributes. Names are
fixed for this implementation:

```rust
/// This entity obtains inherited cascade values from `target`.
pub struct CascadeFrom {
    target: Entity,
}

/// Entities whose `CascadeFrom` relationship targets this entity.
pub struct CascadeChildren(Vec<Entity>);
```

`CascadeFrom` is independent of `ChildOf`. Transform ownership, despawn
ownership, domain membership, and cascade inheritance are separate facts. A
consumer inserts `CascadeFrom` only when inheritance is intended. Bevy
maintains `CascadeChildren`; the relationship must not use linked despawn.

An entity without `CascadeFrom` resolves its local override or the registered
root default. The engine follows at most one `CascadeFrom` target per entity and
accepts a forest with any number of roots.

Use `Cascade<A>` itself as the ECS authored-state component:

```rust
Cascade::<A>::Inherit
Cascade::Override(value)
```

This distinguishes three states without `Option` conversions:

- no `Cascade<A>` component: the entity does not participate in this
  attribute's cached cascade;
- `Cascade<A>::Inherit`: the entity participates and consults `CascadeFrom` or
  the root default;
- `Cascade<A>::Override(value)`: the entity participates and authors the local
  winning value.

The shared engine stores the effective value in `Resolved<A>` and the root in
`CascadeDefault<A>`. `CascadePlugin<A>` installs one propagation system for the
attribute in `CascadeSet::Propagate`.

## Consumer construction seeds

A consumer builder may retain domain values long enough to initialize a newly
spawned entity. Those values are one-time construction seeds, not a second
runtime authoring store. A consumer bridge may read them when the domain
component is added and insert the corresponding `Cascade<A>` only when the
entity does not already carry one. This preserves explicit component commands
queued with or after spawn.

After that initial insertion, the entity's `Cascade<A>` is the only live
authored value and `Resolved<A>` is the engine-owned derived cache. A change to
another domain component must not read the builder seed again or write it back
to `Cascade<A>`. In particular, changing layout data, dimensions, or coordinate
metadata cannot replay a panel's construction values.

## Propagation contract

The engine must react to:

- an added or changed `Cascade<A>` authored state;
- removal of `Cascade<A>`;
- an added, changed, retargeted, or removed `CascadeFrom` relationship;
- a changed `CascadeDefault<A>`.

For each affected entity carrying `Cascade<A>`, resolution checks:

1. its own `Cascade::Override(value)`;
2. then each `CascadeFrom::target()` in order until an override is found;
3. otherwise `CascadeDefault<A>`.

`Cascade::Inherit` continues the walk. An ancestor without `Cascade<A>` is
transparent. Propagation uses `CascadeChildren` to update affected descendants
and writes `Resolved<A>` only when the value differs. Removal of an override or
relationship must be observed in the same way as insertion and mutation.

A cycle or a walk beyond the shared depth limit diagnoses once per triggering
resolution and uses the root default. It must never hang or panic.

Adding a participating `Cascade<A>` must seed `Resolved<A>` without requiring a
domain-specific resolver. Generic entity-command helpers must support:

```rust
entity.set_cascade(Cascade::<A>::Inherit);
entity.override_cascade(value);
entity.inherit_cascade::<A>();
```

Exact trait organization may follow existing `bevy_kana` conventions, but the
operations and semantics above are required. `inherit_cascade` keeps the entity
participating; a separate removal operation may stop participation.

## Consumer example

`hana_diegetic` registration and public vocabulary should reduce to domain
choices and one-line adapters:

```rust
app.add_plugins(CascadePlugin::<ShadowCasting>::new(ShadowCasting::On));

commands.entity(label).insert(CascadeFrom::new(panel));

// Public hana_diegetic verb delegating to the shared generic command.
commands
    .entity(label)
    .override_shadow_casting(ShadowCasting::Off);
```

## Implementation

The generic machinery formerly under `crates/hana_diegetic/src/cascade/` now
lives in `crates/bevy_kana/src/cascade.rs` and uses the explicit relationship
and authored component model. The storage-independent `Cascade<T>` API remains
available without `Option` conversions.

Every existing `hana_diegetic` attribute, construction bridge, command,
reader, render query, example, and cascade test uses the shared engine.
`hana_diegetic` retains typed public verbs and resolved readers without a
public `Cascade<T>` export or public method returning `Cascade<T>`.

Updated documentation:

- `crates/bevy_kana/README.md` and both crate changelogs;
- `docs/hana_diegetic/as-built/cascade.md`;
- `docs/hana_diegetic/as-built/shadow-casting.md` where ownership is described;
- `docs/hana_valence/panel_anchoring_features.md` decision A9.3.1.2.3.1;
- this document from implementation contract to as-built status.

Do not touch unrelated work, including
`docs/bevy_clerestory/restore_after_reconnect.md`.

## Verified behavior

- A second crate can register an attribute and use the ECS engine without
  copying propagation code from `hana_diegetic`.
- A `ChildOf` edge alone does not create cascade inheritance.
- Root defaults, local overrides, inheritance, retargeting, relationship
  removal, authored-state removal, and multi-level propagation work.
- Unchanged resolved values do not trigger downstream change detection.
- Consumer construction values seed once; unrelated component changes never
  replay them over live `Cascade<A>` authoring.
- Cycles and excessive depth terminate at the root default.
- `crates/bevy_kana/examples/cascade.rs` demonstrates generic authoring,
  inheritance, retargeting, and `Resolved<A>` consumption.
- `crates/hana_diegetic/examples/text_cascade.rs` demonstrates the typed
  `hana_diegetic` text-alpha and font-unit facade.
- `hana_diegetic` examples compile without importing `Cascade<T>`.
- `hana_diegetic` contains no duplicate hierarchy walk or dirty-subtree
  propagation implementation.
- Existing domain behavior and public typed verbs remain intact.
- `cargo +nightly fmt --all`, targeted `cargo nextest run`, and strict Clippy
  pass for `bevy_kana` and `hana_diegetic`.
