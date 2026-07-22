# Shared authored easing curves

## Status and scope

This is a deferred, standalone initiative. It must not expand or block the
current Hana Valence folding closeout. The existing folding implementation can
finish with Bevy's `EaseFunction`; the work described here begins separately
after that closeout.

The goal is to give every motion-authoring surface a common choice between a
stock Bevy easing and an editable curve asset. `bevy_kana` owns the common
authoring vocabulary, `bevy_lookup_curve 0.12` supplies the spline machinery,
motion libraries own their timing and state machines, and the Hana editor owns
the editing experience.

This proposal covers:

- the public `bevy_kana` types and sampling service;
- the boundary that keeps `bevy_lookup_curve` out of downstream public APIs;
- migration of authored Bevy easing uses across this workspace and the Hana
  application;
- an `EasingCurve` demonstration in
  `crates/hana_valence/examples/staggered_unfold.rs`;
- validation, loading, reflection, live editing, and migration tests.

It does not make every mathematical interpolation asset-driven. Fixed internal
smoothing that is not an authoring choice remains local unless a concrete use
case requires configurability.

## Ownership and dependency boundary

### `bevy_kana` owns the stable public model

The shared public type should be named `bevy_kana::Easing`. `HanaEasing` would
incorrectly imply that the type is limited to the Hana application even though
Hana Valence, `bevy_lagrange`, Fairy Dust, and other Bevy consumers need it.

The intended model is:

```rust
#[derive(Clone, Debug, Reflect)]
pub enum Easing {
    Bevy(EaseFunction),
    Curve(Handle<EasingCurve>),
}

#[derive(Asset, Clone, Debug, Reflect)]
pub struct EasingCurve {
    // Private implementation backed by bevy_lookup_curve::LookupCurve.
    inner: LookupCurve,
}
```

`EasingCurve` is our asset type. Its serialized representation, constructors,
validation, and editing methods are APIs we control. The public enum must not
contain `Handle<bevy_lookup_curve::LookupCurve>` because that would make the
third-party crate and its asset schema part of every downstream public API.

`bevy_lookup_curve` remains a private implementation dependency of
`bevy_kana`. It supplies knot lookup, tangent handling, constant/linear/cubic
interpolation, weighted cubic evaluation, and lookup caching. If it is later
replaced or forked, downstream `Easing` and `EasingCurve` APIs remain stable.

### Intentional `bevy_kana` public type

Using `Easing` in `hana_valence::FoldSequence` and
`bevy_lagrange::CameraMove` intentionally exposes a `bevy_kana` type. This is
different from accidentally leaking a convenience math wrapper. There is no
way to let callers choose one shared, reflected, asset-backed easing without
exposing some common authoring representation.

Before implementation, confirm `Easing` and `EasingCurve` as deliberate
additions under the workspace's narrow exception for stable cross-crate
protocols and curated facades. `CascadeDefault` and `CascadeSet` already use
this boundary pattern. Numeric conversion traits and semantic math wrappers
remain internal implementation conveniences.

### Cargo features

Add `bevy_lookup_curve 0.12` to the workspace dependency table. `bevy_kana`
should disable its default features and enable only the asset/reflection support
needed by the shared runtime:

```toml
bevy_lookup_curve = {
    version = "0.12",
    default-features = false,
    features = ["bevy_asset", "bevy_reflect"],
}
```

Do not enable `bevy_lookup_curve`'s default `editor_bevy` feature. It would pull
the dependency's editor and Egui integration into users that only need curve
sampling. The Hana editor will provide the product-facing editing surface.

Expose the new module through a `bevy_kana` `easing` feature. Crates whose
public APIs contain `Easing` enable that feature explicitly; cast-only and
input-only users do not acquire asset dependencies unnecessarily.

## Public API

### Authoring types

Provide these public types under the `easing` feature:

- `Easing` — selects `Bevy(EaseFunction)` or
  `Curve(Handle<EasingCurve>)`.
- `EasingCurve` — our reflected, serializable Bevy asset.
- `EasingKnot` — an owned knot description with normalized input, output,
  interpolation, and tangent data.
- `EasingInterpolation` — the supported constant, linear, and cubic choices.
- `EasingCurveBuilder` — ergonomic programmatic construction for examples,
  tests, and generated motion.
- `EasingPlugin` — registers the asset, loader, validation, and sampling
  support.
- `EasingSampler` — a Bevy `SystemParam` that resolves and samples either
  source without exposing the implementation dependency.
- `EasingSampleError` or an equivalent readiness result — distinguishes an
  unloaded asset from invalid authored data.

Provide conversions so existing builder call sites remain compact:

```rust
impl From<EaseFunction> for Easing;
impl From<Handle<EasingCurve>> for Easing;
```

Motion builders should accept `impl Into<Easing>`:

```rust
pub fn easing(mut self, easing: impl Into<Easing>) -> Self {
    self.easing = easing.into();
    self
}
```

Consequently, existing calls continue to work:

```rust
.easing(EaseFunction::CubicOut)
```

and authored assets use the same entry point:

```rust
.easing(curve_handle)
```

Struct literals that remain public require `.into()`, but builders and
constructors should be preferred when migrating APIs.

### Sampling

Consumers must not match on `Easing` and reach into `Assets<EasingCurve>`
themselves. They use the common sampler:

```rust
let eased = match easing_sampler.sample(&motion.easing, raw_progress) {
    Ok(eased) => eased,
    Err(EasingSampleError::NotLoaded(_)) => return,
    Err(error) => {
        // Diagnose once through the owning motion state, then remain idle.
        return;
    }
};
```

The sampler applies `EaseFunction::sample_clamped` for the Bevy variant and
delegates to the private lookup curve for the asset variant. Lookup caches are
runtime sampling state, not authored `EasingCurve` data; if measurement shows a
cache is useful, keep one per active animation or sampler cursor rather than
mutating the shared asset.

Consumer plugins that require `Easing` should install `EasingPlugin`
idempotently. Applications should not need to know which lower-level motion
plugin first introduced the dependency.

## Easing contract

All consumers rely on one semantic contract:

- input progress is normalized to `0..=1`;
- the curve evaluates to exactly `0` at input `0` and exactly `1` at input `1`;
- interior values may exceed `0..=1` so anticipation, elastic motion, and
  overshoot remain possible;
- output is not silently clamped because that would erase authored motion;
- non-finite knots, duplicate input positions, missing endpoints, and invalid
  tangent data are rejected;
- a missing asset is `NotLoaded`, not an instruction to substitute linear
  easing;
- an animation does not advance its raw clock while its easing asset is
  unavailable, preventing a jump when loading finishes;
- reverse motion traverses the same curve backward rather than inventing a
  second easing;
- final state machines still write their exact authored terminal pose when a
  move completes.

Validation belongs in `bevy_kana`; individual camera, folding, and selection
systems must not reproduce these rules. Diagnostics should be transition-based
so an invalid or unloaded asset does not log every frame.

Live editing changes the next sample taken from an asset. This provides the
immediate response expected in the Hana editor, although a large edit during
playback can visibly jump. The editor should encourage editing while paused;
deterministic offline rendering may later snapshot an asset revision when a
move begins.

## Migration inventory

Migration applies to authored or configurable easing surfaces. Tests and
examples follow the API they exercise. Fixed internal smoothing is reviewed but
does not automatically become asset-backed.

### `bevy_kana`

- Add the `easing` module, public types, builder, asset loader, plugin, sampler,
  validation, and reflection support.
- Re-export the intended authoring types from the crate root and prelude when
  the feature is enabled.
- Add unit tests for built-in parity, asset sampling, validation, loading,
  reflection, conversion ergonomics, unavailable assets, and live asset edits.

### Hana Valence folding

- Change `FoldSequence::easing` from `EaseFunction` to `Easing`.
- Because `Easing` contains an asset handle and is not `Copy`, remove `Copy`
  assumptions from `FoldSequence` and `FoldSequenceState`; clone handles only
  at authored/runtime state boundaries where ownership requires it.
- Add `FoldSequence::with_easing(impl Into<Easing>)`.
- Make validation distinguish an invalid fold sequence from a valid sequence
  waiting for its easing asset.
- Sample through `EasingSampler` inside Hana Valence. Hana Valence continues to
  own stage timing, playback direction, endpoint selection, reversal,
  membership, and hinge actuation.
- Update fold unit tests to cover both variants, reverse traversal, asset
  readiness, and exact terminal poses.
- Convert the explicit `EaseFunction` in `triangles.rs` and test fixtures
  mechanically after the shared API lands.

This migration is follow-up work and must not be folded into the current
folding implementation or its closeout documentation.

### `bevy_lagrange`

`bevy_lagrange` is the largest public migration because easing is part of its
camera-move protocol.

- Change both `CameraMove::ToLookAt::easing` and
  `CameraMove::ToOrbitalLookAt::easing` from `EaseFunction` to `Easing`.
- Change `CameraMove::easing()` from a `const fn` returning a copied
  `EaseFunction` to a normal accessor returning `&Easing`.
- Route orbit-camera and free-camera interpolation through `EasingSampler`.
- Preserve exact final-frame camera poses independently of curve overshoot.
- Change the public `LookAt`, `LookAtAndZoomToFit`, `ZoomToFit`, and
  `AnimateToFit` easing builders to accept `impl Into<Easing>`.
- Change internal look-plan construction and animation lifecycle moves to carry
  `Easing` without resolving it early.
- Update `CameraMoveBegin` and `CameraMoveEnd` consumers for non-`Copy` moves;
  keep lifecycle events descriptive rather than embedding resolved sampler
  state.
- Migrate camera-home, input-lifecycle, system-set, fit-trigger, queue, and
  conflict tests.
- Change the animation showcase's `ActiveEasing` from `EaseFunction` to
  `Easing`. Its existing stock-easing randomizer can continue wrapping Bevy
  variants and later gain an asset selector.
- Migrate `examples/animation.rs` and `examples/swapped_axis.rs` so their
  authored steps use the shared type.

This is a public API change for individually published `bevy_lagrange`; its
release notes must call out the new `bevy_kana/easing` dependency and the loss
of `Copy`/`const` easing accessors.

### Fairy Dust and examples in this workspace

- Change Fairy Dust camera restart/home motion to pass `Easing` through the
  updated `bevy_lagrange` builders. Fairy Dust remains responsible for controls
  and presentation, not sampling.
- Mechanically migrate the `hana_conduit` playground camera calls.
- Mechanically migrate the `hana_diegetic` `aa_text` camera-animation example.
- Leave examples that only choose a stock easing concise by relying on
  `From<EaseFunction>` and `impl Into<Easing>`.

### Hana application and editor

The sibling Hana application currently uses Bevy easings in camera and
selection paths.

- Migrate `crates/hana/src/camera/editor_camera.rs`, `flyover.rs`, and
  `zoom_to_target.rs` through the updated `bevy_lagrange` APIs.
- Replace the stock-only selection animation configuration in
  `crates/hana/src/selection/animation.rs` with `Easing` fields for its in, out,
  and in-out profiles. Preserve the existing Bevy defaults through
  `Easing::Bevy`.
- Add an editor property row for every reflected `Easing`: a source selector,
  Bevy easing dropdown, or `EasingCurve` asset selector.
- For a selected curve asset, show knots, tangents, interpolation modes, a
  graph, current preview sample, validation, save/duplicate actions, and all
  known users of the shared handle.
- Mutate `Assets<EasingCurve>` so every referencing motion sees an edit on its
  next sample.
- Keep the small hard-coded `smoothstep` helpers in selection rotate and
  dimension-lock affordances as local Bevy math unless those affordances become
  authored editor properties. They are response shaping, not currently user
  selected motion.

## Hana Valence example: authored accordion easing

Update `crates/hana_valence/examples/staggered_unfold.rs` after the shared API
and Hana Valence migration are complete. It is the clearest demonstration
because its five stages repeat the same curve and make the authored timing easy
to compare visually.

The first version should construct an `EasingCurve` asset synchronously in
`setup` rather than require an external file:

```rust
fn setup(
    mut commands: Commands,
    mut easing_curves: ResMut<Assets<EasingCurve>>,
    // Existing mesh and material resources.
) {
    let mechanical_latch = easing_curves.add(
        EasingCurve::builder()
            .knot(0.00, 0.00)
            .knot(0.20, 0.02)
            .knot(0.55, 0.35)
            .knot(0.82, 0.92)
            .knot(1.00, 1.00)
            .cubic()
            .build(),
    );

    let sequence = commands
        .spawn(
            FoldSequence::new(FOLD_SECONDS)
                .with_easing(mechanical_latch),
        )
        .id();

    // Existing mount and panel construction follows unchanged.
}
```

The precise builder API and tangent defaults should be finalized in
`bevy_kana`; the example must not import `bevy_lookup_curve` types. The curve
should remain bounded between its endpoints so the accordion never crosses its
physical fold limits: a brief hinge resistance, a fast swing, and a gentle
latch are visually distinctive without making panels pass through one another.

Keep the existing title bar restricted to controls. Add the teaching material
to screen-space content:

- identify the shared `bevy_kana::EasingCurve` asset;
- explain "slow release -> fast swing -> soft latch";
- state that all five stages sample the same curve;
- show a compact curve graph with a marker for the active stage's raw and eased
  progress if the necessary read-only state is available without duplicating
  Hana Valence logic.

Space, Shift+Space, and P keep their existing fold semantics. No additional
playback state or example-local easing system is introduced. A later Hana
editor integration can replace the programmatic asset with a loaded
`easing/mechanical-latch.easing.ron` asset and edit it live.

## Verification

The implementation is complete only when:

- `bevy_lookup_curve` types do not appear in downstream public signatures or
  rustdoc outside an explicitly named integration module;
- stock Bevy easing produces the same samples as before migration;
- a programmatic and a loaded `EasingCurve` both drive Hana Valence folding and
  `bevy_lagrange` camera moves;
- unavailable assets hold motion without advancing or falling back;
- forward and reverse playback reach exact endpoints;
- reflected assets can be selected and edited in Hana;
- a live edit is visible to all users of the same handle;
- the staggered accordion demonstrates the custom timing while preserving its
  physically correct fold limits;
- all affected crates pass nightly formatting, Clippy, and `cargo nextest run`
  according to workspace policy.

Treat this work as its own plan, review, implementation sequence, and release
decision after the current folding work is complete.
