//! Typestate builders chained from [`sprinkle_example`](crate::sprinkle_example).
//!
//! `SprinkleBuilder` is the main chain; `PrimitiveBuilder`, `CameraHomeBuilder`,
//! and `TitleBarBuilder` are nested builders that return to the main chain
//! when a non-self method is called.

use std::marker::PhantomData;

use bevy::app::App;
use bevy::ecs::system::EntityCommands;

use crate::camera_home::CameraHomeConfig;
use crate::lighting::StudioLightingConfig;
use crate::primitive::PrimitiveConfig;

/// Boxed deferred-insert closure applied to a primitive entity at spawn time.
pub(super) type PrimitiveInsert = Box<dyn FnOnce(&mut EntityCommands) + Send + Sync>;

mod camera_home;
mod primitive;
mod sprinkle;
mod studio_lighting;
mod title_bar;

/// Typestate marker: the builder has not yet spawned an `OrbitCam`.
///
/// Camera-attached capabilities are not defined for `SprinkleBuilder<NoOrbitCam>`,
/// so calling them is a compile error.
pub struct NoOrbitCam;

/// Typestate marker: the builder has spawned an `OrbitCam`.
///
/// Reached via [`SprinkleBuilder::with_orbit_cam_configured`]. Camera-attached
/// capabilities like [`SprinkleBuilder::with_restore_camera_on_restart`]
/// become callable in this state.
pub struct WithOrbitCam;

/// Builder returned by [`sprinkle_example`](crate::sprinkle_example). State-agnostic capability
/// methods are defined for any `S`; camera-attached methods are gated by
/// the typestate.
pub struct SprinkleBuilder<S> {
    pub(super) app:          App,
    pub(super) state_marker: PhantomData<S>,
}

/// Builder returned while configuring a simple scene primitive.
///
/// Calling a non-primitive builder method finalizes the primitive and returns
/// to the normal [`SprinkleBuilder`] chain.
pub struct PrimitiveBuilder<S> {
    pub(super) parent:  SprinkleBuilder<S>,
    pub(super) config:  PrimitiveConfig,
    pub(super) inserts: Vec<PrimitiveInsert>,
}

/// Builder returned while configuring a camera "home" pose.
///
/// Calling a non-home builder method finalizes the home registration and
/// returns to the normal [`SprinkleBuilder`] chain.
pub struct CameraHomeBuilder<S> {
    pub(super) parent: SprinkleBuilder<S>,
    pub(super) config: CameraHomeConfig,
}

/// Builder returned by [`SprinkleBuilder::with_studio_lighting`] for tweaking
/// the studio rig before it spawns.
///
/// Lighting tweak methods are only reachable through this type, so calling
/// [`Self::aim_at`] or [`Self::key_light_pos`] is a compile error when no
/// studio lighting has been installed. Calling a non-lighting builder method
/// finalizes the configuration and returns to the normal [`SprinkleBuilder`]
/// (or [`PrimitiveBuilder`]) chain.
pub struct StudioLightingBuilder<S> {
    pub(super) parent: SprinkleBuilder<S>,
    pub(super) config: StudioLightingConfig,
}

/// Builder returned by [`SprinkleBuilder::with_title_bar`] for wiring chip
/// highlights to event lifecycles.
///
/// Chip-wiring methods are only reachable through this type, so calling
/// [`Self::wire_chip_to_events`] is a compile error when no title bar has
/// been installed.
///
/// Calling a non-wiring builder method finalizes the title bar configuration
/// and returns to the normal [`SprinkleBuilder`] chain.
pub struct TitleBarBuilder<S> {
    pub(super) parent: SprinkleBuilder<S>,
}
