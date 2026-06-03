//! Swaps an `OrbitCam`'s `Projection` between `OrthographicProjection` and
//! `PerspectiveProjection` at runtime. `switch_projection` swaps the component,
//! calls `OrbitCam::force_update()` so the camera re-derives its cached state,
//! and updates the cube-face labels. `widen_shadow_cascade` (ordered after
//! `FairyDustStudioLightingSet`) enlarges the directional light's cascade so
//! the orthographic far plane stays within shadow range.
//!
//! Controls:
//!   O — orthographic projection
//!   P — perspective projection

use bevy::camera::ScalingMode;
use bevy::light::CascadeShadowConfig;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_diegetic::DiegeticTextMut;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::CubeFaceLabel;
use fairy_dust::Face;
use fairy_dust::FairyDustStudioLightingSet;
use fairy_dust::TitleBar;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .face_label(Face::Front, ORTHOGRAPHIC_LABEL)
        .face_label(Face::Back, ORTHOGRAPHIC_LABEL)
        .face_label(Face::Left, ORTHOGRAPHIC_LABEL)
        .face_label(Face::Right, ORTHOGRAPHIC_LABEL)
        .face_label(Face::Top, ORTHOGRAPHIC_LABEL)
        .face_label(Face::Bottom, ORTHOGRAPHIC_LABEL)
        .insert(CameraHomeTarget)
        .with_orbit_cam_preset_bundle(
            |_| {},
            OrbitCamPreset::BlenderLike,
            orthographic_projection(),
        )
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Orthographic")
                .with_anchor(Anchor::TopLeft)
                .control(ORTHOGRAPHIC_CONTROL)
                .control(PERSPECTIVE_CONTROL),
        )
        .wire_chip_to_state::<ProjectionChoice, _>(ORTHOGRAPHIC_CONTROL, |choice| {
            activation_for(*choice == ProjectionChoice::Orthographic)
        })
        .wire_chip_to_state::<ProjectionChoice, _>(PERSPECTIVE_CONTROL, |choice| {
            activation_for(*choice == ProjectionChoice::Perspective)
        })
        .init_resource::<ProjectionChoice>()
        .with_camera_control_panel()
        // Cascade widening runs after the studio rig spawns its directional light.
        .add_systems(
            Startup,
            widen_shadow_cascade.after(FairyDustStudioLightingSet),
        )
        // `O` / `P` run through Fairy Dust's shortcut binding, which fires each
        // only when no modifier is held.
        .with_shortcut(KeyCode::KeyO, select_orthographic)
        .with_shortcut(KeyCode::KeyP, select_perspective)
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// PROJECTION SWAP — Projection component + OrbitCam::force_update + cascade
// widening for the orthographic far plane.
//
// How it works:
//   1. `orthographic_projection()` is passed into `with_orbit_cam` so the camera spawns with an
//      `OrthographicProjection` (fixed vertical viewport, 40-unit far plane).
//   2. `widen_shadow_cascade` runs once at startup, after the studio lighting rig spawns, to extend
//      the directional light's cascade `maximum_distance` so shadows survive across the wider
//      orthographic far plane.
//   3. On **O** or **P**, Fairy Dust's shortcut binding runs `select_orthographic` /
//      `select_perspective`, which call `switch_projection` to overwrite the `Projection` component
//      with the orthographic or perspective variant, refresh the cube-face labels via
//      `update_face_labels`, and call `OrbitCam::force_update()` so the camera re-derives its
//      cached state under the new projection.
// ═════════════════════════════════════════════════════════════════════════════

const HOME_PITCH: f32 = 0.42;
const HOME_YAW: f32 = -0.28;
const HOME_MARGIN: f32 = 0.6;
const ORTHOGRAPHIC_FAR_PLANE: f32 = 40.0;
const ORTHOGRAPHIC_SHADOW_FIRST_CASCADE: f32 = 12.0;
const ORTHOGRAPHIC_SHADOW_MAX_DISTANCE: f32 = 60.0;
const ORTHOGRAPHIC_VIEWPORT_HEIGHT: f32 = 1.0;

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum ProjectionChoice {
    #[default]
    Orthographic,
    Perspective,
}

fn widen_shadow_cascade(mut lights: Query<&mut CascadeShadowConfig, With<DirectionalLight>>) {
    for mut cascade in &mut lights {
        *cascade = CascadeShadowConfigBuilder {
            maximum_distance: ORTHOGRAPHIC_SHADOW_MAX_DISTANCE,
            first_cascade_far_bound: ORTHOGRAPHIC_SHADOW_FIRST_CASCADE,
            ..default()
        }
        .build();
    }
}

fn select_orthographic(
    choice: ResMut<ProjectionChoice>,
    camera_query: Query<(&mut OrbitCam, &mut Projection)>,
    face_labels: DiegeticTextMut<CubeFaceLabel>,
) {
    switch_projection(
        ProjectionChoice::Orthographic,
        choice,
        camera_query,
        face_labels,
    );
}

fn select_perspective(
    choice: ResMut<ProjectionChoice>,
    camera_query: Query<(&mut OrbitCam, &mut Projection)>,
    face_labels: DiegeticTextMut<CubeFaceLabel>,
) {
    switch_projection(
        ProjectionChoice::Perspective,
        choice,
        camera_query,
        face_labels,
    );
}

/// Swaps the camera's `Projection` to `next_choice` (unless already there),
/// refreshes the cube-face labels, and calls `force_update` so `OrbitCam`
/// re-derives its cached state under the new projection.
fn switch_projection(
    next_choice: ProjectionChoice,
    mut choice: ResMut<ProjectionChoice>,
    mut camera_query: Query<(&mut OrbitCam, &mut Projection)>,
    mut face_labels: DiegeticTextMut<CubeFaceLabel>,
) {
    if *choice == next_choice {
        return;
    }

    let Ok((mut camera, mut projection)) = camera_query.single_mut() else {
        return;
    };

    *projection = match next_choice {
        ProjectionChoice::Orthographic => orthographic_projection(),
        ProjectionChoice::Perspective => perspective_projection(),
    };
    *choice = next_choice;
    update_face_labels(&mut face_labels, next_choice);
    camera.force_update();
}

fn orthographic_projection() -> Projection {
    Projection::from(OrthographicProjection {
        scaling_mode: ScalingMode::FixedVertical {
            viewport_height: ORTHOGRAPHIC_VIEWPORT_HEIGHT,
        },
        far: ORTHOGRAPHIC_FAR_PLANE,
        ..OrthographicProjection::default_3d()
    })
}

fn perspective_projection() -> Projection {
    Projection::Perspective(PerspectiveProjection::default())
}

// ═════════════════════════════════════════════════════════════════════════════
// SCENE SCAFFOLDING — cube body and ground sized to match.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_COLOR: Color = fairy_dust::EXAMPLE_CUBE_COLOR;
const CUBE_SIZE: f32 = fairy_dust::EXAMPLE_CUBE_SIZE;
const CUBE_TRANSLATION: Vec3 = fairy_dust::example_cube_on_ground(0.1);

const GROUND_SIZE: f32 = fairy_dust::EXAMPLE_GROUND_SIZE;

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE LABELS — world-space DiegeticText labels on every cube face that swap when the
// projection mode changes.
// ═════════════════════════════════════════════════════════════════════════════

const ORTHOGRAPHIC_LABEL: &str = "Orthographic";
const PERSPECTIVE_LABEL: &str = "Perspective";

fn update_face_labels(face_labels: &mut DiegeticTextMut<CubeFaceLabel>, choice: ProjectionChoice) {
    let label = match choice {
        ProjectionChoice::Orthographic => ORTHOGRAPHIC_LABEL,
        ProjectionChoice::Perspective => PERSPECTIVE_LABEL,
    };
    face_labels.set(label);
}

// ═════════════════════════════════════════════════════════════════════════════
// CONTROL CHIPS — title-bar chip strings and the active/inactive mapper used
// by `wire_chip_to_state`.
// ═════════════════════════════════════════════════════════════════════════════

const ORTHOGRAPHIC_CONTROL: &str = "O Orthographic";
const PERSPECTIVE_CONTROL: &str = "P Perspective";

const fn activation_for(active: bool) -> ControlActivation {
    if active {
        ControlActivation::Active
    } else {
        ControlActivation::Inactive
    }
}
