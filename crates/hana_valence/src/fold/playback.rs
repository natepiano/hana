use bevy_kana::ToF32;
use bevy_reflect::Reflect;

use super::FoldEndpoint;

/// Remembered direction for fold commands and terminal playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Clone)]
pub enum FoldDirection {
    /// Moves from boundary zero toward the terminal folded boundary.
    Folding,
    /// Moves from the terminal folded boundary toward boundary zero.
    Unfolding,
}

/// Current kind of fold playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Clone)]
pub enum FoldMotion {
    /// No fold playback is active.
    Idle,
    /// Playback is moving to one or more queued stage boundaries.
    Step,
    /// Playback is moving to the terminal boundary in the remembered direction.
    Play,
}

#[derive(Clone, Copy, Debug, Reflect)]
pub(super) struct FoldPlayback {
    position:  f32,
    target:    usize,
    direction: FoldDirection,
    motion:    FoldMotion,
}

impl FoldPlayback {
    pub(super) fn initial(endpoint: FoldEndpoint, stages: usize) -> Self {
        match endpoint {
            FoldEndpoint::Unfolded => Self {
                position:  0.0,
                target:    0,
                direction: FoldDirection::Folding,
                motion:    FoldMotion::Idle,
            },
            FoldEndpoint::Folded => Self {
                position:  stages.to_f32(),
                target:    stages,
                direction: FoldDirection::Unfolding,
                motion:    FoldMotion::Idle,
            },
        }
    }

    pub(super) const fn uninitialized() -> Self {
        Self {
            position:  0.0,
            target:    0,
            direction: FoldDirection::Folding,
            motion:    FoldMotion::Idle,
        }
    }

    pub(super) fn clamp_to(&mut self, stages: usize) {
        self.position = self.position.min(stages.to_f32());
        self.target = self.target.min(stages);
    }

    pub(super) const fn stop(&mut self) { self.motion = FoldMotion::Idle; }

    pub(super) const fn position(&self) -> f32 { self.position }

    pub(super) const fn target(&self) -> usize { self.target }

    pub(super) const fn direction(&self) -> FoldDirection { self.direction }

    pub(super) const fn motion(&self) -> FoldMotion { self.motion }
}
