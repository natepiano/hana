# hana_valence

hana_valence -- shapes expose connection points and bond into animatable
assemblies; named for valence, an atom's capacity to bond.

> **Work in progress.** This crate is in active development and is not subject
> to semver stability guarantees. APIs may change between commits.

## What it does

In chemistry, an atom's valence is its capacity to bond: the number and
arrangement of connection points it offers. This crate gives authored geometry
the same capability. Providers publish anchor points, entities bond through
those anchors, and systems animate those bonds as assemblies form, separate, and
reconfigure.

- **Anchor geometry** -- providers fill `ResolvedAnchorGeometry` with vertices,
  edge midpoints, centers, and the edges that define hinge axes.
- **Entity bonds** -- `AnchoredTo` connects one entity anchor to another entity
  anchor, while `resolve_anchors` writes the resulting `Transform`.
- **Hinge animation** -- `Hinge` drives `AnchorPose` so an anchored entity folds
  around one of its authored edges.
- **Arrangements** -- `Member` entities can be ordered under a `Strip`,
  `Accordion`, or `Coil`, then placed and folded by a `TilingRule` such as
  `QuadTiling`.

The crate is named `hana_valence`, but the concrete API keeps the **anchor**
noun: `AnchorId`, `AnchoredTo`, `AnchorPose`. An anchor point is the connection
site. 

## Quick start

`hana_valence` exposes components and systems, not a plugin. Consumers configure
the system sets in their own app:

```rust
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use hana_valence::*;

App::new()
    .add_plugins(DefaultPlugins)
    .init_resource::<ResolveDiagnostics>()
    .add_observer(on_member_added)
    .add_observer(on_member_removed)
    .configure_sets(
        PostUpdate,
        (
            AnchorSystems::FillGeometry,
            AnchorSystems::AnimatePose,
            AnchorSystems::Resolve,
        )
            .chain()
            .before(TransformSystems::Propagate),
    )
    .add_systems(
        PostUpdate,
        (
            (
                assign_member_indices,
                ApplyDeferred,
                apply_member_placements::<QuadTiling>,
                ApplyDeferred,
            )
                .chain()
                .after(AnchorSystems::FillGeometry)
                .before(AnchorSystems::AnimatePose),
            drive_arrangement_hinges::<QuadTiling>.in_set(AnchorSystems::AnimatePose),
            hinge_to_pose
                .in_set(AnchorSystems::AnimatePose)
                .after(drive_arrangement_hinges::<QuadTiling>),
            resolve_anchors.in_set(AnchorSystems::Resolve),
        ),
    );
```

An arrangement root carries both an arrangement driver and its tiling rule:

```rust
commands.spawn((
    ResolvedAnchorGeometry { /* provider-filled anchors */ },
    Transform::default(),
    GlobalTransform::default(),
    Accordion::default(),
    QuadTiling,
));
```

Each member then inserts `Member { arrangement }`. The observer and systems
assign `MemberIndex`, place the member with `AnchoredTo`, add `AnchorPose` and
`Hinge`, and drive the hinge angle each frame.

`Strip` keeps every member at its tiling rule's rest angle. `Accordion` folds
adjacent hinges in alternating directions. `Coil` folds every hinge in the same
direction so rotations accumulate down the member set.

## Examples

The examples in `examples/` are standalone Bevy apps:

- `staggered_unfold` -- five quads with staggered `bevy_tween` hinge animation.
- `triangles` -- equilateral triangles using an example-defined
  `TriangleTiling` implementation.
- `box` -- a six-quad cross net that folds into a closed box with direct
  `AnchoredTo` and `Hinge` relations.

Run the default-feature examples with:

```sh
cargo run -p hana_valence --example triangles
cargo run -p hana_valence --example box
```

`staggered_unfold` requires the default `tween` feature:

```sh
cargo run -p hana_valence --example staggered_unfold
```

## Bevy compatibility

| hana_valence | Bevy |
|--------------|------|
| main         | 0.19 |

## License

MIT OR Apache-2.0
