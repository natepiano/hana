//! Shared cube spin helpers for example presentation.

use std::marker::PhantomData;

use bevy::prelude::*;

use crate::screen_panels;
use crate::screen_panels::ControlActivation;
use crate::screen_panels::TitleBarControlState;
use crate::screen_panels::TitleChip;
use crate::screen_panels::TitleChipActivation;

/// Marker inserted by [`crate::PrimitiveBuilder::cube_spin`] for builder-owned spin.
#[derive(Component)]
pub struct FairyDustCubeSpinTarget;

/// Whether a cube spin helper is currently rotating its targets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CubeSpinMode {
    /// Rotate targets.
    #[default]
    Spinning,
    /// Keep targets still.
    Paused,
}

impl CubeSpinMode {
    /// Returns the opposite spin mode.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Spinning => Self::Paused,
            Self::Paused => Self::Spinning,
        }
    }
}

impl TitleChipActivation for CubeSpinMode {
    fn activation(&self) -> ControlActivation {
        match self {
            Self::Spinning => ControlActivation::Inactive,
            Self::Paused => ControlActivation::Active,
        }
    }
}

/// Rotation motion applied by a cube spin helper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CubeSpinMotion {
    /// Rotate around local Y by radians per second.
    Yaw(f32),
    /// Rotate around an arbitrary local axis by radians per second.
    AxisAngle {
        /// Local-space axis.
        axis:               Vec3,
        /// Angular speed.
        radians_per_second: f32,
    },
    /// Rotate around local X/Y/Z by radians per second.
    Euler {
        /// Per-axis angular speed.
        radians_per_second: Vec3,
    },
}

impl Default for CubeSpinMotion {
    fn default() -> Self { Self::Yaw(0.2) }
}

/// Time source used by cube spin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CubeSpinTimeSource {
    /// Bevy virtual time. Respects pause.
    #[default]
    Virtual,
    /// Wall-clock time. Ignores virtual-time pause.
    Real,
}

/// Configuration for [`crate::SprinkleBuilder::with_cube_spin`].
#[derive(Clone, Debug)]
pub struct CubeSpinConfig {
    /// Optional title chip added by the helper.
    pub chip:          Option<TitleChip>,
    /// Optional keyboard shortcut that toggles spin.
    pub key:           Option<KeyCode>,
    /// Spin mode that should highlight the title chip.
    pub active_mode:   CubeSpinMode,
    /// Rotation motion.
    pub motion:        CubeSpinMotion,
    /// Time source.
    pub time_source:   CubeSpinTimeSource,
    /// Initial spin mode.
    pub initial_state: CubeSpinMode,
}

impl CubeSpinConfig {
    /// Creates the canonical `P Pause` cube spin helper.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            chip:          Some(TitleChip::new("cube_spin_pause", "P Pause")),
            key:           Some(KeyCode::KeyP),
            active_mode:   CubeSpinMode::Paused,
            motion:        CubeSpinMotion::Yaw(0.2),
            time_source:   CubeSpinTimeSource::Virtual,
            initial_state: CubeSpinMode::Spinning,
        }
    }

    /// Uses a different title chip.
    #[must_use]
    pub const fn with_chip(mut self, chip: TitleChip) -> Self {
        self.chip = Some(chip);
        self
    }

    /// Disables title-chip registration and highlighting.
    #[must_use]
    pub const fn without_chip(mut self) -> Self {
        self.chip = None;
        self
    }

    /// Uses a different toggle key.
    #[must_use]
    pub const fn with_key(mut self, key: KeyCode) -> Self {
        self.key = Some(key);
        self
    }

    /// Highlights the title chip when the helper is in `active_mode`.
    #[must_use]
    pub const fn with_active_mode(mut self, active_mode: CubeSpinMode) -> Self {
        self.active_mode = active_mode;
        self
    }

    /// Disables keyboard toggling.
    #[must_use]
    pub const fn without_key(mut self) -> Self {
        self.key = None;
        self
    }

    /// Uses a different spin motion.
    #[must_use]
    pub const fn with_motion(mut self, motion: CubeSpinMotion) -> Self {
        self.motion = motion;
        self
    }

    /// Uses a different time source.
    #[must_use]
    pub const fn with_time_source(mut self, time_source: CubeSpinTimeSource) -> Self {
        self.time_source = time_source;
        self
    }

    /// Uses a different initial spin mode.
    #[must_use]
    pub const fn with_initial_state(mut self, initial_state: CubeSpinMode) -> Self {
        self.initial_state = initial_state;
        self
    }
}

impl Default for CubeSpinConfig {
    fn default() -> Self { Self::new() }
}

/// Marker-scoped cube spin state.
#[derive(Resource)]
pub struct CubeSpinControl<M> {
    mode:        CubeSpinMode,
    key:         Option<KeyCode>,
    chip_id:     Option<String>,
    active_mode: CubeSpinMode,
    motion:      CubeSpinMotion,
    time_source: CubeSpinTimeSource,
    marker:      PhantomData<M>,
}

impl<M> CubeSpinControl<M> {
    fn new(config: &CubeSpinConfig) -> Self {
        Self {
            mode:        config.initial_state,
            key:         config.key,
            chip_id:     config.chip.map(|chip| chip.id().to_string()),
            active_mode: config.active_mode,
            motion:      config.motion,
            time_source: config.time_source,
            marker:      PhantomData,
        }
    }

    /// Returns the current spin mode.
    #[must_use]
    pub const fn mode(&self) -> CubeSpinMode { self.mode }

    /// Flips between spinning and paused. Examples that drive the toggle from a
    /// non-keyboard input (a gamepad button, say) call this; the title chip
    /// follows automatically through `sync_cube_spin_chip`.
    pub const fn toggle(&mut self) { self.mode = self.mode.toggled(); }
}

impl<M: 'static> TitleChipActivation for CubeSpinControl<M> {
    fn activation(&self) -> ControlActivation {
        if self.mode == self.active_mode {
            ControlActivation::Active
        } else {
            ControlActivation::Inactive
        }
    }
}

pub(crate) fn install<M: Component>(app: &mut App, config: CubeSpinConfig) {
    if let Some(chip) = config.chip {
        screen_panels::register_title_control(app, chip);
    }
    app.insert_resource(CubeSpinControl::<M>::new(&config));
    app.add_systems(
        Update,
        (
            toggle_cube_spin::<M>,
            sync_cube_spin_chip::<M>,
            spin_cube_targets::<M>,
        ),
    );
}

fn toggle_cube_spin<M: Component>(
    key_input: Res<ButtonInput<KeyCode>>,
    mut control: ResMut<CubeSpinControl<M>>,
) {
    let Some(key) = control.key else { return };
    if key_input.just_pressed(key) {
        control.mode = control.mode.toggled();
    }
}

fn sync_cube_spin_chip<M: Component>(
    control: Res<CubeSpinControl<M>>,
    mut bars: Query<&mut TitleBarControlState>,
) {
    if !control.is_changed() {
        return;
    }
    let Some(chip_id) = control.chip_id.as_deref() else {
        return;
    };
    for mut bar in &mut bars {
        bar.set_active(chip_id, control.activation());
    }
}

fn spin_cube_targets<M: Component>(
    time_real: Res<Time<Real>>,
    time_virtual: Res<Time<Virtual>>,
    control: Res<CubeSpinControl<M>>,
    mut targets: Query<&mut Transform, With<M>>,
) {
    if control.mode != CubeSpinMode::Spinning {
        return;
    }
    let delta_secs = match control.time_source {
        CubeSpinTimeSource::Virtual => time_virtual.delta_secs(),
        CubeSpinTimeSource::Real => time_real.delta_secs(),
    };
    for mut transform in &mut targets {
        match control.motion {
            CubeSpinMotion::Yaw(speed) => transform.rotate_y(speed * delta_secs),
            CubeSpinMotion::AxisAngle {
                axis,
                radians_per_second,
            } => transform.rotate(Quat::from_axis_angle(
                axis.normalize_or_zero(),
                radians_per_second * delta_secs,
            )),
            CubeSpinMotion::Euler { radians_per_second } => {
                transform.rotate_x(radians_per_second.x * delta_secs);
                transform.rotate_y(radians_per_second.y * delta_secs);
                transform.rotate_z(radians_per_second.z * delta_secs);
            },
        }
    }
}
