use bevy::prelude::*;
use bevy_kana::Position as WorldPosition;

use super::geometry::FitSolution;
use crate::CameraBasis;
use crate::Initialization;
use crate::free_cam::FreeCam;
use crate::operation::Focus;
use crate::operation::LookAngles;
use crate::operation::Position;
use crate::operation::Radius;
use crate::operation::Roll;
use crate::orbit_cam::OrbitCam;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FreeCamFitPose {
    pub(super) position: Position,
    pub(super) look:     LookAngles,
    pub(super) roll:     Roll,
}

impl FreeCamFitPose {
    pub(super) const fn from_free_cam_current(free_cam: &FreeCam) -> Self {
        Self {
            position: free_cam.translate.current(),
            look:     free_cam.look.current(),
            roll:     free_cam.roll.current(),
        }
    }

    pub(super) fn from_free_cam_or_transform(
        free_cam: &FreeCam,
        transform: &Transform,
        basis: CameraBasis,
    ) -> Self {
        if free_cam.initialization == Initialization::FromTransform {
            Self::from_transform(transform, basis)
        } else {
            Self::from_free_cam_current(free_cam)
        }
    }

    pub(super) fn from_transform(transform: &Transform, basis: CameraBasis) -> Self {
        let local_rotation = basis.rotation().inverse() * transform.rotation;
        let (yaw, pitch, roll) = local_rotation.to_euler(EulerRot::YXZ);
        Self {
            position: Position(transform.translation),
            look:     LookAngles { yaw, pitch: -pitch },
            roll:     Roll(roll),
        }
    }

    pub(super) fn from_fit(
        fit: FitSolution,
        projection: &Projection,
        basis: CameraBasis,
        look: LookAngles,
        roll: Roll,
    ) -> Self {
        let rotation = basis.rotation()
            * Quat::from_rotation_y(look.yaw)
            * Quat::from_rotation_x(-look.pitch)
            * Quat::from_rotation_z(roll.0);
        let radius = match projection {
            Projection::Orthographic(projection) => (projection.near + projection.far) * 0.5,
            Projection::Perspective(_) | Projection::Custom(_) => fit.radius,
        };
        Self {
            position: Position(*fit.focus + rotation * Vec3::new(0.0, 0.0, radius)),
            look,
            roll,
        }
    }
}

pub(super) const fn sync_free_cam_projection(projection: &mut Projection, fit: FitSolution) {
    if let Projection::Orthographic(projection) = projection {
        projection.scale = fit.radius;
    }
}

pub(super) fn apply_free_cam_pose(free_cam: &mut FreeCam, pose: FreeCamFitPose) {
    free_cam.translate.snap_to(pose.position);
    free_cam.look.snap_to(pose.look);
    free_cam.roll.snap_to(pose.roll);
    free_cam.initialization = Initialization::Active;
    free_cam.force_update();
}

/// Parameters for an instant orbital snap.
pub(super) struct SnapOrbit {
    pub(super) focus:  WorldPosition,
    pub(super) yaw:    Option<f32>,
    pub(super) pitch:  Option<f32>,
    pub(super) radius: f32,
}

/// Snaps the camera to an orbital position instantly and fires caller-provided
/// lifecycle events via `emit_events`.
pub(super) fn snap_to_orbit(
    commands: &mut Commands,
    orbit_cam: &mut OrbitCam,
    snap: SnapOrbit,
    emit_events: impl FnOnce(&mut Commands),
) {
    orbit_cam.pan.snap_to(Focus(*snap.focus));
    orbit_cam.zoom.snap_to(Radius(snap.radius));
    // Only the axes the caller provided are snapped; an unspecified axis keeps
    // both its current and target angle.
    let mut current = orbit_cam.orbit.current();
    let mut target = orbit_cam.orbit.target();
    if let Some(yaw) = snap.yaw {
        current.yaw = yaw;
        target.yaw = yaw;
    }
    if let Some(pitch) = snap.pitch {
        current.pitch = pitch;
        target.pitch = pitch;
    }
    orbit_cam.orbit.set_current(current);
    orbit_cam.orbit.set_target(target);
    orbit_cam.force_update();

    emit_events(commands);
}
