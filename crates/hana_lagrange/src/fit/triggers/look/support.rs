use bevy::prelude::*;

use crate::animation::AnimationBegin;
use crate::animation::AnimationEnd;
use crate::animation::AnimationReason;
use crate::animation::AnimationSource;
use crate::animation::CameraMove;
use crate::animation::PlayAnimation;

pub(super) fn trigger_timed_animation(
    commands: &mut Commands,
    camera: Entity,
    target: Entity,
    source: AnimationSource,
    camera_moves: impl IntoIterator<Item = CameraMove>,
) {
    commands.trigger(
        PlayAnimation::new(camera, camera_moves)
            .source(source)
            .target(target),
    );
}

pub(super) fn trigger_completed_animation(
    commands: &mut Commands,
    camera: Entity,
    target: Entity,
    source: AnimationSource,
) {
    commands.trigger(AnimationBegin {
        camera,
        source,
        target: Some(target),
    });
    commands.trigger(AnimationEnd {
        camera,
        source,
        target: Some(target),
        reason: AnimationReason::Completed,
    });
}
