//! Demonstrates a swapped-up-axis `OrbitCam` by rotating a labeled ±X/±Y/±Z
//! axis gizmo between a Z-up and a Y-up presentation.
//!
//! The camera stays put; pressing `Y` or `Z` spins the whole gizmo so the
//! chosen axis stands up and the other faces the viewer. Each press plays a
//! two-phase move: first the target axis tilts to vertical, then the gizmo
//! spins about that axis until the second arrow points at the camera. Targets
//! are derived from the live camera, so they stay correct after you orbit.
//!
//! The gizmo arrows are immediate-mode; the letters are unlit world text
//! parented to the gizmo root, so the scene needs no ground plane or lights.
//!
//! Controls:
//!   Y - rotate so +Y stands up and +Z faces the camera
//!   Z - rotate so +Z stands up and +Y faces the camera
//!   H - return to the home pose

use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_diegetic::StableTransparency;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::OrbitCam;
use fairy_dust::Anchor;
use fairy_dust::ControlActivation;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::TitleBar;

// camera
const SWAPPED_AXIS: [Vec3; 3] = [Vec3::X, Vec3::Z, Vec3::Y];
const Y_UP_KEY: KeyCode = KeyCode::KeyY;
const Z_UP_KEY: KeyCode = KeyCode::KeyZ;

// home pose — frames the gizmo so the swapped (Z-up) axis renders blue/Z
// standing vertical, looking back along -Y.
const HOME_FRAME_SIZE: f32 = 2.89;
const HOME_PITCH: f32 = -1.283;
const HOME_YAW: f32 = -3.507;

// title bar
const Y_UP_LABEL: &str = "Y Y-up";
const Z_UP_LABEL: &str = "Z Z-up";

// axis gizmo
const AXIS_GIZMO_LENGTH: f32 = 2.0;
const AXIS_X_COLOR: Color = Color::srgb(0.90, 0.20, 0.20);
const AXIS_Y_COLOR: Color = Color::srgb(0.20, 0.80, 0.25);
const AXIS_Z_COLOR: Color = Color::srgb(0.30, 0.45, 0.95);

// axis labels — billboarded world text just past each arrow tip
const AXIS_LABEL_SIZE: f32 = 0.18;
const AXIS_LABEL_OFFSET: f32 = 0.25;
// labels only lift clear once their arm points this much toward/away from the
// camera; below the threshold they stay colinear with the arrow
const LABEL_OCCLUSION_THRESHOLD: f32 = 0.6;
// max screen-vertical nudge once a label is fully occluded (down for the one in
// front, up for the one behind)
const LABEL_DEPTH_LIFT: f32 = 0.3;

// yaw the gizmo this far off the view axis so the toward/away arrow keeps some
// length instead of collapsing to a single pixel
const FACE_YAW_OFFSET: f32 = 0.30;

// transition — each press plays a tilt phase then a spin phase, this long each
const STEP_DURATION: f32 = 0.5;

/// Which axis the gizmo stands up. Drives the title-bar chips and the rotation
/// targets the `Y`/`Z` keys animate to.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AxisMode {
    YUp,
    #[default]
    ZUp,
}

impl AxisMode {
    /// `(up_axis, face_axis)` — the local gizmo axis to stand up, and the one to
    /// point at the camera.
    const fn axes(self) -> (Vec3, Vec3) {
        match self {
            Self::YUp => (Vec3::Y, Vec3::Z),
            Self::ZUp => (Vec3::Z, Vec3::Y),
        }
    }
}

/// Parent transform for the whole gizmo. Rotating this rotates every arm and
/// label together.
#[derive(Component)]
struct GizmoRoot;

/// Pending rotation targets for the gizmo root plus the in-flight tween. The
/// `Y`/`Z` keys push a [tilt, spin] pair; `drive_gizmo_motion` plays them in
/// order.
#[derive(Component, Default)]
struct GizmoMotion {
    queue: VecDeque<Quat>,
    tween: Option<RotationTween>,
}

/// One ±axis world-text label. `direction` is the arm it sits on; the billboard
/// system uses it to face the camera and to lift the label clear of a
/// foreshortened arrow.
#[derive(Component)]
struct AxisLabel {
    direction: Vec3,
}

/// Smooth rotation from `from` to `to` over [`STEP_DURATION`].
#[derive(Clone, Copy)]
struct RotationTween {
    from:    Quat,
    to:      Quat,
    elapsed: f32,
}

impl RotationTween {
    const fn new(from: Quat, to: Quat) -> Self {
        Self {
            from,
            to,
            elapsed: 0.0,
        }
    }

    /// Advances by `dt` and returns the eased rotation plus whether it finished.
    fn advance(&mut self, dt: f32) -> (Quat, bool) {
        self.elapsed += dt;
        let t = (self.elapsed / STEP_DURATION).clamp(0.0, 1.0);
        let eased = t * t * 2.0f32.mul_add(-t, 3.0);
        (
            self.from.slerp(self.to, eased),
            self.elapsed >= STEP_DURATION,
        )
    }
}

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_camera_home(Transform::from_scale(Vec3::splat(HOME_FRAME_SIZE)))
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .with_title_bar(
            TitleBar::new()
                .with_anchor(Anchor::TopLeft)
                .control(Y_UP_LABEL)
                .active_control(Z_UP_LABEL),
        )
        .wire_chip_to_state::<AxisMode, _>(Y_UP_LABEL, |mode| match mode {
            AxisMode::YUp => ControlActivation::Active,
            AxisMode::ZUp => ControlActivation::Inactive,
        })
        .wire_chip_to_state::<AxisMode, _>(Z_UP_LABEL, |mode| match mode {
            AxisMode::ZUp => ControlActivation::Active,
            AxisMode::YUp => ControlActivation::Inactive,
        })
        .with_camera_control_panel()
        .init_resource::<AxisMode>()
        .add_systems(Startup, (spawn_camera, spawn_gizmo))
        .add_systems(
            Update,
            (
                select_axis,
                establish_initial_pose,
                drive_gizmo_motion,
                draw_axis_gizmo,
                billboard_axis_labels,
            )
                .chain(),
        )
        .run();
}

fn spawn_camera(mut commands: Commands) {
    // Swap the camera axis to use Z as up instead of the default Y. Tagging the
    // entity with `FairyDustOrbitCam` lets `with_camera_home` drive the H key.
    // `StableTransparency` enables OIT for the world-text labels.
    commands.spawn((
        OrbitCam {
            axis: SWAPPED_AXIS,
            ..default()
        },
        FairyDustOrbitCam,
        StableTransparency,
    ));
}

fn spawn_gizmo(mut commands: Commands) {
    commands
        .spawn((
            GizmoRoot,
            GizmoMotion::default(),
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|root| {
            for (direction, color, positive, negative) in [
                (Vec3::X, AXIS_X_COLOR, "+x", "-x"),
                (Vec3::Y, AXIS_Y_COLOR, "+y", "-y"),
                (Vec3::Z, AXIS_Z_COLOR, "+z", "-z"),
            ] {
                spawn_label(root, direction, color, positive);
                spawn_label(root, -direction, color, negative);
            }
        });
}

fn spawn_label(root: &mut ChildSpawnerCommands, direction: Vec3, color: Color, glyph: &str) {
    let tip = direction * (AXIS_GIZMO_LENGTH + AXIS_LABEL_OFFSET);
    root.spawn((
        AxisLabel { direction },
        WorldText::new(glyph),
        WorldTextStyle::new(AXIS_LABEL_SIZE)
            .with_color(color)
            .with_anchor(Anchor::Center)
            .with_unlit(),
        Transform::from_translation(tip),
    ));
}

fn select_axis(
    keys: Res<ButtonInput<KeyCode>>,
    mut axis_mode: ResMut<AxisMode>,
    camera: Query<&GlobalTransform, With<FairyDustOrbitCam>>,
    mut gizmo: Query<(&Transform, &mut GizmoMotion), With<GizmoRoot>>,
) {
    let requested = if keys.just_pressed(Y_UP_KEY) {
        AxisMode::YUp
    } else if keys.just_pressed(Z_UP_KEY) {
        AxisMode::ZUp
    } else {
        return;
    };
    if *axis_mode == requested {
        return;
    }
    let Ok(camera) = camera.single() else {
        return;
    };
    let Ok((transform, mut motion)) = gizmo.single_mut() else {
        return;
    };
    *axis_mode = requested;

    let (up_axis, face_axis) = requested.axes();
    let world_up = (camera.rotation() * Vec3::Y).normalize();
    // Phase 1: shortest tilt that stands the target axis up. Phase 2: the full
    // target, which keeps that axis up, so the move between them is a pure spin
    // about the up axis.
    let tilt = Quat::from_rotation_arc((transform.rotation * up_axis).normalize(), world_up)
        * transform.rotation;
    let spin = gizmo_target(up_axis, face_axis, camera);
    motion.queue = VecDeque::from([tilt, spin]);
    motion.tween = None;
}

/// Snaps the gizmo to the Z-up pose once the home framing has settled, so the
/// scene opens in the same orientation a `Z` press produces.
fn establish_initial_pose(
    camera: Query<&GlobalTransform, With<FairyDustOrbitCam>>,
    mut gizmo: Query<&mut Transform, With<GizmoRoot>>,
    mut previous: Local<Option<Vec3>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Ok(camera) = camera.single() else {
        return;
    };
    let position = camera.translation();
    if let Some(previous) = *previous
        && (position - previous).length() < 1.0e-4
        && position.length() > 1.0e-3
        && let Ok(mut transform) = gizmo.single_mut()
    {
        let (up_axis, face_axis) = AxisMode::ZUp.axes();
        transform.rotation = gizmo_target(up_axis, face_axis, camera);
        *done = true;
    }
    *previous = Some(position);
}

fn drive_gizmo_motion(
    time: Res<Time>,
    mut gizmo: Query<(&mut Transform, &mut GizmoMotion), With<GizmoRoot>>,
) {
    let Ok((mut transform, mut motion)) = gizmo.single_mut() else {
        return;
    };
    if motion.tween.is_none() {
        let Some(target) = motion.queue.pop_front() else {
            return;
        };
        motion.tween = Some(RotationTween::new(transform.rotation, target));
    }
    if let Some(tween) = motion.tween.as_mut() {
        let (rotation, finished) = tween.advance(time.delta_secs());
        transform.rotation = rotation;
        if finished {
            motion.tween = None;
        }
    }
}

fn draw_axis_gizmo(mut gizmos: Gizmos, root: Query<&GlobalTransform, With<GizmoRoot>>) {
    let Ok(root) = root.single() else {
        return;
    };
    let origin = root.translation();
    let rotation = root.rotation();
    for (direction, color) in [
        (Vec3::X, AXIS_X_COLOR),
        (Vec3::Y, AXIS_Y_COLOR),
        (Vec3::Z, AXIS_Z_COLOR),
    ] {
        let arm = rotation * direction * AXIS_GIZMO_LENGTH;
        gizmos.arrow(origin, origin + arm, color);
        gizmos.arrow(origin, origin - arm, color);
    }
}

fn billboard_axis_labels(
    camera: Query<&GlobalTransform, With<OrbitCam>>,
    root: Query<&GlobalTransform, With<GizmoRoot>>,
    mut labels: Query<(&AxisLabel, &mut Transform)>,
) {
    // Labels are children of the gizmo root. Cancel the root's rotation and
    // apply the camera's so the letters stay screen-facing and upright. Then
    // slide toward/away labels along screen-up (down for the one in front, up
    // for the one behind) so they clear the foreshortened arrow.
    let Ok(camera) = camera.single() else {
        return;
    };
    let Ok(root) = root.single() else {
        return;
    };
    let root_rotation = root.rotation();
    let billboard = root_rotation.inverse() * camera.rotation();
    let screen_up = camera.rotation() * Vec3::Y;
    let to_camera = camera.translation().normalize_or_zero();
    let inverse_root = root_rotation.inverse();
    for (label, mut transform) in &mut labels {
        let world_dir = (root_rotation * label.direction).normalize_or_zero();
        let depth = world_dir.dot(to_camera);
        // Ramp the lift in only past the occlusion threshold so colinear axes
        // (x, z here) keep their labels on the arrow.
        let engage = ((depth.abs() - LABEL_OCCLUSION_THRESHOLD)
            / (1.0 - LABEL_OCCLUSION_THRESHOLD))
            .clamp(0.0, 1.0);
        let world_offset = -screen_up * depth.signum() * engage * LABEL_DEPTH_LIFT;
        let base = label.direction * (AXIS_GIZMO_LENGTH + AXIS_LABEL_OFFSET);
        transform.translation = base + inverse_root * world_offset;
        transform.rotation = billboard;
    }
}

/// World-space gizmo orientation that stands `up_axis` along the camera's up and
/// points `face_axis` toward the camera. The face direction is orthogonalized
/// against up so the result is a proper rotation.
fn gizmo_target(up_axis: Vec3, face_axis: Vec3, camera: &GlobalTransform) -> Quat {
    let world_up = (camera.rotation() * Vec3::Y).normalize();
    let to_camera = camera.translation().normalize_or_zero();
    let world_face = (to_camera - to_camera.dot(world_up) * world_up).normalize();
    let world_third = world_up.cross(world_face);
    let local = Mat3::from_cols(up_axis, face_axis, up_axis.cross(face_axis));
    let world = Mat3::from_cols(world_up, world_face, world_third);
    let aligned = Quat::from_mat3(&(world * local.transpose()));
    // Yaw off dead-center so the toward/away arm keeps some length on screen.
    Quat::from_axis_angle(world_up, FACE_YAW_OFFSET) * aligned
}
