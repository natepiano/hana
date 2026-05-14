//! Capability: a generalized "home" pose for the spawned `OrbitCam`.
//!
//! Registers an invisible cube entity at the caller-supplied [`Transform`] and
//! wires up `H` to [`bevy_lagrange::AnimateToFit`] that entity. The Transform's
//! `scale` defines the region the camera frames; the builder's `yaw`/`pitch`
//! set the orbit orientation. The home pose drives both the startup framing
//! (instant) and the `H` key animation. If a title bar is installed, the
//! `H Home` control chip is prepended automatically.

use std::time::Duration;

use bevy::prelude::*;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationSource;

use crate::orbit_cam::FairyDustOrbitCam;
use crate::screen_panels::ControlActivation;
use crate::screen_panels::TitleBarControlState;

pub(crate) const HOME_CONTROL: &str = "H Home";
pub(crate) const HOME_DEFAULT_DURATION: Duration = Duration::from_millis(800);
pub(crate) const HOME_DEFAULT_MARGIN: f32 = 0.15;
const HOME_KEY: KeyCode = KeyCode::KeyH;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum InitialAnimateState {
    #[default]
    Pending,
    Fired,
}

#[derive(Component)]
struct CameraHomeMarker;

/// Stashed home configuration. Read by the title-bar installer to decide
/// whether to prepend the `H Home` chip.
#[derive(Resource, Clone)]
pub(crate) struct CameraHomeConfig {
    pub transform: Transform,
    pub yaw:       f32,
    pub pitch:     f32,
    pub duration:  Duration,
    pub margin:    f32,
}

#[derive(Resource)]
struct CameraHomeEntity(Entity);

pub(crate) fn install(app: &mut App, config: CameraHomeConfig) {
    app.insert_resource(config);
    app.add_systems(Startup, spawn_home_marker);
    app.add_systems(Update, (trigger_initial_animate, handle_home_key));
    app.add_observer(on_home_animation_begin);
    app.add_observer(on_home_animation_end);
}

fn spawn_home_marker(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    config: Res<CameraHomeConfig>,
) {
    let mesh = meshes.add(Cuboid::from_size(Vec3::ONE));
    let entity = commands
        .spawn((
            CameraHomeMarker,
            Mesh3d(mesh),
            config.transform,
            Visibility::Hidden,
        ))
        .id();
    commands.insert_resource(CameraHomeEntity(entity));
}

fn trigger_initial_animate(
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
    mut state: Local<InitialAnimateState>,
) {
    if *state == InitialAnimateState::Fired {
        return;
    }
    let Some(home) = home else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };
    commands.trigger(
        AnimateToFit::new(camera, home.0)
            .yaw(config.yaw)
            .pitch(config.pitch)
            .margin(config.margin)
            .duration(Duration::ZERO),
    );
    *state = InitialAnimateState::Fired;
}

fn handle_home_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    home: Option<Res<CameraHomeEntity>>,
    config: Res<CameraHomeConfig>,
    cameras: Query<Entity, With<FairyDustOrbitCam>>,
) {
    if !keys.just_pressed(HOME_KEY) {
        return;
    }
    let Some(home) = home else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };
    commands.trigger(
        AnimateToFit::new(camera, home.0)
            .yaw(config.yaw)
            .pitch(config.pitch)
            .margin(config.margin)
            .duration(config.duration),
    );
}

fn on_home_animation_begin(
    trigger: On<AnimationBegin>,
    home: Option<Res<CameraHomeEntity>>,
    mut bars: Query<&mut TitleBarControlState>,
) {
    if home.is_none() || trigger.source != AnimationSource::AnimateToFit {
        return;
    }
    for mut bar in &mut bars {
        bar.set_active(HOME_CONTROL, ControlActivation::Active);
    }
}

fn on_home_animation_end(
    trigger: On<AnimationEnd>,
    home: Option<Res<CameraHomeEntity>>,
    mut bars: Query<&mut TitleBarControlState>,
) {
    if home.is_none() || trigger.source != AnimationSource::AnimateToFit {
        return;
    }
    for mut bar in &mut bars {
        bar.set_active(HOME_CONTROL, ControlActivation::Inactive);
    }
}
