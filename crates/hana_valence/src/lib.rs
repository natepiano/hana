//! Anchor-point relationships for animatable Bevy assemblies.
//!
//! `hana_valence` lets authored geometry expose local anchor points and edges,
//! then connects entities by component data. The contract is
//! [`ResolvedAnchorGeometry`] on each entity, not dynamic dispatch or a global
//! anchor table. Providers fill one `ResolvedAnchorGeometry` per source entity,
//! drivers animate [`AnchorPose`], and consumers run their resolver systems in
//! [`AnchorSystems`].
//!
//! The crate exposes components and system sets only. Consumers own plugin
//! wiring and type registration:
//!
//! ```rust,ignore
//! use bevy::app::PostUpdate;
//! use bevy::transform::TransformSystems;
//! use hana_valence::AnchorSystems;
//!
//! app.configure_sets(
//!     PostUpdate,
//!     (
//!         AnchorSystems::FillGeometry,
//!         AnchorSystems::AnimatePose,
//!         AnchorSystems::Resolve,
//!     )
//!         .chain()
//!         .before(TransformSystems::Propagate),
//! );
//! ```
//!
//! Animator systems may tween three valence inputs: [`AnchorPose`],
//! [`Hinge::angle`], and [`bevy_transform::prelude::Transform`] on entities
//! that do not carry [`AnchoredTo`]. Register those systems in
//! [`AnchorSystems::AnimatePose`] so their writes happen before
//! [`resolve_anchors`] runs in [`AnchorSystems::Resolve`].
//!
//! With the `tween` feature enabled, `HingeAngleLens` and `AnchorPoseLens`
//! are `bevy_tween` component interpolators for the two valence components:
//!
//! ```rust,ignore
//! use bevy::app::PostUpdate;
//! use bevy_tween::BevyTweenRegisterSystems;
//! use bevy_tween::TweenSystemSet;
//! use bevy_tween::tween::component_tween_system;
//! use hana_valence::AnchorPoseLens;
//! use hana_valence::AnchorSystems;
//! use hana_valence::HingeAngleLens;
//! use hana_valence::hinge_to_pose;
//!
//! app.add_tween_systems(
//!     PostUpdate,
//!     (
//!         component_tween_system::<HingeAngleLens>(),
//!         component_tween_system::<AnchorPoseLens>(),
//!     ),
//! );
//! app.configure_sets(
//!     PostUpdate,
//!     TweenSystemSet::ApplyTween.in_set(AnchorSystems::AnimatePose),
//! );
//! app.add_systems(
//!     PostUpdate,
//!     hinge_to_pose
//!         .in_set(AnchorSystems::AnimatePose)
//!         .after(TweenSystemSet::ApplyTween),
//! );
//! ```
//!
//! `AnchorPoseLens` and [`Hinge`] are mutually exclusive on one entity.
//! [`hinge_to_pose`] overwrites the whole [`AnchorPose`] every frame and resets
//! [`AnchorPose::translation`] to [`bevy_math::Vec3::ZERO`], so a direct
//! `AnchorPose` tween on a hinged entity is discarded. Debug builds warn when
//! `hinge_to_pose` sees an earlier same-frame `AnchorPose` change.
//! `bevy_animation` property adapters can be added later without changing this
//! component contract.
//!
//! Arrangements add another driver layer: [`Accordion`] and [`Strip`] write
//! [`Hinge::angle`] for their [`Member`] entities every frame. The animatable
//! surface for an arranged set is the arrangement component, for example
//! [`Accordion::fold`] and [`Accordion::lean`]. A direct `HingeAngleLens` on a
//! member is overwritten by [`drive_arrangement_hinges`], just as a direct
//! `AnchorPoseLens` is overwritten by [`hinge_to_pose`] on a hinged entity.
//!
//! Anchor naming has three tiers. Generated geometry should use ids derived
//! from adjacency and never require authored names. Hand-authored regular
//! geometry should use provider names such as `Anchor::TopLeft` when offered.
//! One-off geometry can use raw [`AnchorId`] values. Pick the highest tier that
//! matches the data you own; reusable recipes ask [`ResolvedAnchorGeometry`]
//! which edge is shared with the predecessor instead of hardcoding ids.
//!
//! Resolver math for an entity with [`AnchoredTo`] is:
//!
//! ```text
//! target_world = parent.global * parent.geometry[target_anchor].position
//! source_local = child.geometry[source_anchor].position
//! base         = parent.global.rotation * target_point.rotation()
//! rot          = base * pose.rotation * source_point.rotation().inverse()
//! offset_eff   = resolved_anchor_offset.unwrap_or(anchored_to.offset)
//! child.translation = target_world + base * (offset_eff + pose.translation)
//!                   - rot * (child_global_scale * source_local)
//! child.rotation    = rot
//! ```
//!
//! `child_global_scale` supports uniform scale on the child entity. Non-uniform
//! child scale is unsupported because the source-anchor subtraction applies
//! scale in the child frame before rotating into the target-anchor frame.
//!
//! `offset_eff` and `pose.translation` are evaluated in the target-anchor frame
//! and are independent of `pose.rotation`. For example, with identity frames,
//! a target anchor at `(3, 3, 0)`, and raw offset `(0.25, -0.5, 0)`, the child
//! anchor lands at `(3.25, 2.5, 0)`. Unit conversion, DPI conversion, and
//! coordinate-system sign changes happen in provider crates before data reaches
//! this resolver.
//!
//! [`resolve_anchors`] writes local [`bevy_transform::prelude::Transform`]
//! values. Same-frame reads of anchored entities'
//! [`bevy_transform::prelude::GlobalTransform`] components are stale until the
//! consumer runs transform propagation after [`AnchorSystems::Resolve`].
//!
//! [`ResolvedAnchorWorld`] is recomputed every frame, never
//! change-detection-gated. Entities resolved in the current frame get cache
//! points from their just-resolved global transform. Entities that carry the
//! cache but were not resolved in the current frame get points from the previous
//! propagation pass's `GlobalTransform`, the same one-frame staleness that
//! applies to every `GlobalTransform` read by the resolver because
//! [`AnchorSystems::Resolve`] runs before `TransformSystems::Propagate`. The
//! cache has the same freshness as the resolve pass that writes it.

// Lets the shared `../fixtures.rs` include reference this crate by name from the
// `resolve` unit tests, matching the external examples and integration test.
extern crate self as hana_valence;

mod arrange;
mod attachment;
mod geometry;
mod hinge;
mod pose;
mod relation;
mod resolve;
#[cfg(feature = "tween")]
mod tween;

pub use arrange::Accordion;
pub use arrange::ArrangementMembers;
pub use arrange::ArrangementPlacement;
pub use arrange::FoldPattern;
pub use arrange::Member;
pub use arrange::MemberIndex;
pub use arrange::MemberPlacement;
pub use arrange::PendingMemberPlacement;
pub use arrange::QuadTiling;
pub use arrange::Strip;
pub use arrange::TilingRule;
pub use arrange::apply_member_placements;
pub use arrange::assign_member_indices;
pub use arrange::drive_arrangement_hinges;
pub use arrange::member_placement;
pub use arrange::on_member_added;
pub use arrange::on_member_removed;
pub use attachment::AttachmentResolveAction;
pub use attachment::AttachmentResolveCandidate;
pub use attachment::AttachmentResolveDiagnostic;
pub use attachment::AttachmentResolveDiagnostics;
pub use attachment::AttachmentResolveReasons;
pub use attachment::resolve_attachments;
pub use geometry::AnchorId;
pub use geometry::AnchorPoint;
pub use geometry::Edge;
pub use geometry::EdgeAxisError;
pub use geometry::GeometryError;
pub use geometry::ResolvedAnchorGeometry;
pub use hinge::Hinge;
pub use hinge::hinge_to_pose;
pub use pose::AnchorPose;
pub use pose::AnchorSystems;
pub use pose::ResolvedAnchorWorld;
pub use relation::AnchoredHere;
pub use relation::AnchoredTo;
pub use relation::ResolvedAnchorOffset;
pub use resolve::ResolveDiagnostics;
pub use resolve::ResolveSkip;
pub use resolve::resolve_anchors;
#[cfg(feature = "tween")]
pub use tween::AnchorPoseLens;
#[cfg(feature = "tween")]
pub use tween::HingeAngleLens;
