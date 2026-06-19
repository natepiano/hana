//! Capability: add a thresholded bloom to the orbit camera so over-bright
//! (>1.0) colors glow while normal-range content stays crisp.
//!
//! Requires HDR on the camera; [`Bloom`] pulls in the `Hdr` component via its
//! required-components, so this works without [`crate::SprinkleBuilder::with_hdr`],
//! but pairing both keeps every camera in the render chain HDR.
//!
//! Gated behind the `SprinkleBuilder<WithOrbitCam>` typestate — see
//! [`crate::SprinkleBuilder::with_bloom`].

use bevy::post_process::bloom::Bloom;
use bevy::post_process::bloom::BloomCompositeMode;
use bevy::post_process::bloom::BloomPrefilter;
use bevy::prelude::*;

use crate::orbit_cam::FairyDustOrbitCam;

/// Only pixels brighter than this (pre-tonemap luminance) contribute to bloom.
/// Lit colored text/shapes peak above 1.0 under studio lighting, so the
/// threshold sits above them and below the over-bright emissive readout, which
/// is the only content meant to glow.
const BLOOM_THRESHOLD: f32 = 3.0;
const BLOOM_THRESHOLD_SOFTNESS: f32 = 0.2;
const BLOOM_INTENSITY: f32 = 0.25;

pub(crate) fn install(app: &mut App) { app.add_observer(insert_bloom); }

fn insert_bloom(trigger: On<Add, FairyDustOrbitCam>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(Bloom {
        intensity: BLOOM_INTENSITY,
        prefilter: BloomPrefilter {
            threshold:          BLOOM_THRESHOLD,
            threshold_softness: BLOOM_THRESHOLD_SOFTNESS,
        },
        composite_mode: BloomCompositeMode::Additive,
        ..Bloom::OLD_SCHOOL
    });
}
