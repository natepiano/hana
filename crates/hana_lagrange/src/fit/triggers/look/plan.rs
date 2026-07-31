use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_kana::Displacement;

use crate::CameraBasis;
use crate::animation;
use crate::animation::CameraMove;
use crate::operation::LookAngles;
use crate::operation::Roll;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LookAtPlan {
    pub(super) camera_position: Vec3,
    pub(super) target_position: Vec3,
    pub(super) yaw:             f32,
    pub(super) pitch:           f32,
    pub(super) radius:          f32,
}

impl LookAtPlan {
    pub(super) fn from_world_positions(camera_position: Vec3, target_position: Vec3) -> Self {
        let (yaw, pitch, radius) = animation::orbital_parameters_from_offset(Displacement(
            camera_position - target_position,
        ));
        Self {
            camera_position,
            target_position,
            yaw,
            pitch,
            radius,
        }
    }

    pub(super) fn from_free_camera(
        camera_position: Vec3,
        target_position: Vec3,
        basis: CameraBasis,
    ) -> Self {
        let local_offset = basis.rotation().inverse() * (camera_position - target_position);
        let (yaw, pitch, radius) =
            animation::orbital_parameters_from_offset(Displacement(local_offset));
        Self {
            camera_position,
            target_position,
            yaw,
            pitch,
            radius,
        }
    }

    pub(super) const fn look_angles(self) -> LookAngles {
        LookAngles {
            yaw:   self.yaw,
            pitch: self.pitch,
        }
    }

    pub(super) const fn to_look_move(
        self,
        roll: Option<Roll>,
        duration: Duration,
        easing: EaseFunction,
    ) -> CameraMove {
        CameraMove::ToLookAt {
            position: self.camera_position,
            target: self.target_position,
            roll,
            duration,
            easing,
        }
    }
}
