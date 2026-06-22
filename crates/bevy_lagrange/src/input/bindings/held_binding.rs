//! Held-binding primitives: the BEI-style value binding plus engagement binding pair.
//!
//! Types:
//! - [`OrbitCamHeldBinding`] — a value binding (motion) paired with an engagement binding
//!   (button/key that latches the motion). Stored on [`super::action_set::HeldActionBindingEntry`]
//!   after validation.
//! - [`OrbitCamInputBinding`] — a `bevy_enhanced_input` [`Binding`] plus the composite variants
//!   (`CardinalKeys`, `BidirectionalKeys`, `GamepadAxes2d`, `BidirectionalGamepadButtons`) that
//!   expand into multiple BEI bindings.
//!
//! Helpers `route_for_sources` and `sources_for_binding` derive default routing and source
//! metadata from the underlying BEI binding.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::Binding;

use super::action_set::BindingRoutePolicy;
use super::descriptor::HeldBindingDescriptor;
use super::descriptor::InputAxisTransform;
use super::descriptor::InputBindingDescriptor;
use super::descriptor::InputBindingEntry;
use super::descriptor::InputBindingModifiers;
use super::descriptor::InputBindingOutputAxis;
use super::descriptor::InputBindingScale;
use super::descriptor::InputDeadZone;
use super::descriptor::InputSensitivity;
use crate::input::CameraInteractionSources;
use crate::input::ControlSpeed;

/// A held enhanced-input binding made from a value binding and an engagement binding.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct OrbitCamHeldBinding {
    pub(super) motion:     OrbitCamInputBinding,
    pub(super) engagement: OrbitCamInputBinding,
    pub(super) gates:      BindingGates,
    pub(super) sources:    CameraInteractionSources,
    pub(super) route:      BindingRoutePolicy,
    pub(super) speed:      ControlSpeed,
}

impl OrbitCamHeldBinding {
    /// Creates a held binding from BEI-style value and engagement bindings.
    #[must_use]
    pub fn new(
        motion: impl Into<OrbitCamInputBinding>,
        engagement: impl Into<OrbitCamInputBinding>,
    ) -> Self {
        let motion = motion.into();
        let engagement = engagement.into();
        let sources = motion.sources().union(engagement.sources());
        let route = route_for_sources(sources);
        Self {
            motion,
            engagement,
            gates: BindingGates::default(),
            sources,
            route,
            speed: ControlSpeed::Normal,
        }
    }

    /// Creates a held binding whose value binding also engages the action.
    #[must_use]
    pub fn same(binding: impl Into<OrbitCamInputBinding>) -> Self {
        let binding = binding.into();
        Self::new(binding.clone(), binding)
    }

    /// Overrides source attribution for this binding.
    #[must_use]
    pub const fn with_sources(mut self, sources: CameraInteractionSources) -> Self {
        self.sources = sources;
        self
    }

    /// Overrides routing for this binding.
    #[must_use]
    pub const fn with_route(mut self, route: BindingRoutePolicy) -> Self {
        self.route = route;
        self
    }

    /// Requires `input` while this held binding is active.
    #[must_use]
    pub fn with_required_gate(mut self, input: impl Into<OrbitCamGateInput>) -> Self {
        self.gates = self.gates.with_required(input);
        self
    }

    /// Suppresses this held binding while `input` is active.
    #[must_use]
    pub fn with_blocked_gate(mut self, input: impl Into<OrbitCamGateInput>) -> Self {
        self.gates = self.gates.with_blocked(input);
        self
    }

    /// Sets the authored sensitivity for the motion binding.
    #[must_use]
    pub fn with_sensitivity(mut self, sensitivity: f32) -> Self {
        self.motion = self.motion.with_sensitivity(sensitivity);
        self
    }

    /// Tags this binding as the normal or slow (precise) speed variant.
    #[must_use]
    pub const fn speed(mut self, speed: ControlSpeed) -> Self {
        self.speed = speed;
        self
    }
}

impl From<OrbitCamHeldBinding> for HeldBindingDescriptor {
    fn from(binding: OrbitCamHeldBinding) -> Self {
        Self {
            motion:             binding.motion.descriptor(),
            engagement:         Some(binding.engagement.descriptor()),
            gates:              binding.gates,
            sources:            binding.sources,
            engagement_sources: binding.sources,
            route:              binding.route,
            speed:              binding.speed,
        }
    }
}

/// Gate descriptors applied to both motion and engagement entries of a held binding.
#[derive(Clone, Debug, Default, PartialEq, Eq, Reflect)]
pub struct BindingGates {
    gates: Vec<OrbitCamBindingGate>,
}

impl BindingGates {
    /// Returns the gate descriptors in installation order.
    #[must_use]
    pub fn entries(&self) -> &[OrbitCamBindingGate] { &self.gates }

    fn with_required(mut self, input: impl Into<OrbitCamGateInput>) -> Self {
        self.gates.push(OrbitCamBindingGate {
            input:    input.into(),
            polarity: OrbitCamGatePolarity::Required,
        });
        self
    }

    fn with_blocked(mut self, input: impl Into<OrbitCamGateInput>) -> Self {
        self.gates.push(OrbitCamBindingGate {
            input:    input.into(),
            polarity: OrbitCamGatePolarity::Blocked,
        });
        self
    }
}

/// One symbolic gate attached to a held binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub struct OrbitCamBindingGate {
    /// Input checked by this gate.
    pub input:    OrbitCamGateInput,
    /// Whether the input is required or blocks the binding.
    pub polarity: OrbitCamGatePolarity,
}

/// Physical input used by a held-binding gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
#[non_exhaustive]
pub enum OrbitCamGateInput {
    /// Gamepad button gate.
    GamepadButton(GamepadButton),
    /// Keyboard key gate.
    Key(KeyCode),
}

impl From<GamepadButton> for OrbitCamGateInput {
    fn from(value: GamepadButton) -> Self { Self::GamepadButton(value) }
}

impl From<KeyCode> for OrbitCamGateInput {
    fn from(value: KeyCode) -> Self { Self::Key(value) }
}

/// Gate behavior for a held binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum OrbitCamGatePolarity {
    /// Gate input must be active.
    Required,
    /// Gate input suppresses the binding while active.
    Blocked,
}

/// A BEI-style input binding plus `OrbitCam` composite helpers.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(opaque)]
#[non_exhaustive]
pub enum OrbitCamInputBinding {
    /// A native `bevy_enhanced_input` binding.
    Binding(Binding),
    /// Four keyboard keys captured as positive Y, positive X, negative Y, negative X.
    CardinalKeys(KeyCode, KeyCode, KeyCode, KeyCode),
    /// Two keyboard keys captured as positive and negative 1D values.
    BidirectionalKeys(KeyCode, KeyCode),
    /// Two gamepad axes captured as X and Y.
    GamepadAxes2d(GamepadAxis, GamepadAxis),
    /// Two analog gamepad buttons captured as positive and negative 1D values.
    BidirectionalGamepadButtons(GamepadButton, GamepadButton),
    /// A single analog gamepad button captured as a signed 1D value.
    GamepadButtonAxis(GamepadButton, f32),
    /// A binding with descriptor modifiers applied after composite expansion.
    Modified {
        /// Inner binding being modified.
        binding:     Box<Self>,
        /// Modifiers applied to every expanded entry.
        modifiers:   InputBindingModifiers,
        /// Optional scale applied with composite-axis awareness.
        scale:       Option<InputBindingScale>,
        /// Authored sensitivity applied after signed scale lowering.
        sensitivity: InputSensitivity,
    },
}

impl OrbitCamInputBinding {
    /// Creates a four-key 2D binding from positive Y, positive X, negative Y, negative X.
    #[must_use]
    pub const fn cardinal_keys(
        north: KeyCode,
        east: KeyCode,
        south: KeyCode,
        west: KeyCode,
    ) -> Self {
        Self::CardinalKeys(north, east, south, west)
    }

    /// Creates a two-key 1D binding from positive and negative keys.
    #[must_use]
    pub const fn bidirectional_keys(positive: KeyCode, negative: KeyCode) -> Self {
        Self::BidirectionalKeys(positive, negative)
    }

    /// Creates a two-axis gamepad binding from X and Y axes.
    #[must_use]
    pub const fn gamepad_axes_2d(x: GamepadAxis, y: GamepadAxis) -> Self {
        Self::GamepadAxes2d(x, y)
    }

    /// Creates a two-button gamepad binding from positive and negative buttons.
    #[must_use]
    pub const fn bidirectional_gamepad_buttons(
        positive: GamepadButton,
        negative: GamepadButton,
    ) -> Self {
        Self::BidirectionalGamepadButtons(positive, negative)
    }

    /// Creates a signed single-button gamepad axis binding.
    #[must_use]
    pub const fn gamepad_button_axis(button: GamepadButton, scale: f32) -> Self {
        Self::GamepadButtonAxis(button, scale)
    }

    /// Applies a BEI scale modifier after dead-zone processing.
    #[must_use]
    pub fn with_scale(self, scale: impl Into<InputBindingScale>) -> Self {
        self.with_descriptor_update(|_, binding_scale, _| {
            *binding_scale = Some(scale.into());
        })
    }

    /// Applies an axial BEI dead-zone modifier.
    #[must_use]
    pub fn with_dead_zone(self, dead_zone: InputDeadZone) -> Self {
        self.with_descriptor_update(|modifiers, _, _| {
            *modifiers = modifiers.with_dead_zone(dead_zone);
        })
    }

    /// Scales held input by the current frame delta.
    #[must_use]
    pub fn with_delta_scale(self) -> Self {
        self.with_descriptor_update(|modifiers, _, _| {
            *modifiers = modifiers.with_delta_scale();
        })
    }

    /// Sets the authored sensitivity for this input binding.
    #[must_use]
    pub fn with_sensitivity(self, sensitivity: f32) -> Self {
        self.with_descriptor_update(|_, _, input_sensitivity| {
            *input_sensitivity = InputSensitivity(sensitivity);
        })
    }

    fn with_descriptor_update(
        self,
        update: impl FnOnce(
            &mut InputBindingModifiers,
            &mut Option<InputBindingScale>,
            &mut InputSensitivity,
        ),
    ) -> Self {
        match self {
            Self::Modified {
                binding,
                mut modifiers,
                mut scale,
                mut sensitivity,
            } => {
                update(&mut modifiers, &mut scale, &mut sensitivity);
                Self::Modified {
                    binding,
                    modifiers,
                    scale,
                    sensitivity,
                }
            },
            binding => {
                let mut modifiers = InputBindingModifiers::default();
                let mut scale = None;
                let mut sensitivity = InputSensitivity::DEFAULT;
                update(&mut modifiers, &mut scale, &mut sensitivity);
                Self::Modified {
                    binding: Box::new(binding),
                    modifiers,
                    scale,
                    sensitivity,
                }
            },
        }
    }

    pub(super) fn descriptor(&self) -> InputBindingDescriptor {
        match self {
            Self::Binding(binding) => InputBindingDescriptor::single(*binding),
            Self::CardinalKeys(north, east, south, west) => InputBindingDescriptor::entries([
                InputBindingEntry::new(Binding::from(*east), InputAxisTransform::None),
                InputBindingEntry::new(Binding::from(*west), InputAxisTransform::Negate),
                InputBindingEntry::new(Binding::from(*north), InputAxisTransform::Swizzle)
                    .with_output_axis(InputBindingOutputAxis::Y),
                InputBindingEntry::new(Binding::from(*south), InputAxisTransform::SwizzleNegate)
                    .with_output_axis(InputBindingOutputAxis::Y),
            ]),
            Self::BidirectionalKeys(positive, negative) => InputBindingDescriptor::entries([
                InputBindingEntry::new(Binding::from(*positive), InputAxisTransform::None),
                InputBindingEntry::new(Binding::from(*negative), InputAxisTransform::Negate),
            ]),
            Self::GamepadAxes2d(x, y) => InputBindingDescriptor::entries([
                InputBindingEntry::new(Binding::GamepadAxis(*x), InputAxisTransform::None),
                InputBindingEntry::new(Binding::GamepadAxis(*y), InputAxisTransform::Swizzle)
                    .with_output_axis(InputBindingOutputAxis::Y),
            ]),
            Self::BidirectionalGamepadButtons(positive, negative) => {
                InputBindingDescriptor::entries([
                    InputBindingEntry::new(
                        Binding::GamepadButton(*positive),
                        InputAxisTransform::None,
                    ),
                    InputBindingEntry::new(
                        Binding::GamepadButton(*negative),
                        InputAxisTransform::Negate,
                    ),
                ])
            },
            Self::GamepadButtonAxis(button, scale) => {
                InputBindingDescriptor::single(Binding::GamepadButton(*button))
                    .with_entry_modifiers(InputBindingModifiers::default(), Some((*scale).into()))
            },
            Self::Modified {
                binding,
                modifiers,
                scale,
                sensitivity,
            } => binding
                .descriptor()
                .with_entry_modifiers(*modifiers, *scale)
                .with_entry_sensitivity(*sensitivity),
        }
    }

    fn sources(&self) -> CameraInteractionSources {
        match self {
            Self::Binding(binding) => sources_for_binding(*binding),
            Self::CardinalKeys(..) | Self::BidirectionalKeys(..) => {
                CameraInteractionSources::KEYBOARD
            },
            Self::GamepadAxes2d(..)
            | Self::BidirectionalGamepadButtons(..)
            | Self::GamepadButtonAxis(..) => CameraInteractionSources::GAMEPAD,
            Self::Modified { binding, .. } => binding.sources(),
        }
    }
}

impl From<Binding> for OrbitCamInputBinding {
    fn from(value: Binding) -> Self { Self::Binding(value) }
}

impl From<KeyCode> for OrbitCamInputBinding {
    fn from(value: KeyCode) -> Self { Self::Binding(Binding::from(value)) }
}

impl From<MouseButton> for OrbitCamInputBinding {
    fn from(value: MouseButton) -> Self { Self::Binding(Binding::from(value)) }
}

impl From<GamepadButton> for OrbitCamInputBinding {
    fn from(value: GamepadButton) -> Self { Self::Binding(Binding::from(value)) }
}

impl From<GamepadAxis> for OrbitCamInputBinding {
    fn from(value: GamepadAxis) -> Self { Self::Binding(Binding::GamepadAxis(value)) }
}

const fn route_for_sources(sources: CameraInteractionSources) -> BindingRoutePolicy {
    if sources.contains(CameraInteractionSources::MOUSE) {
        BindingRoutePolicy::CursorPosition
    } else {
        BindingRoutePolicy::NoPosition
    }
}

const fn sources_for_binding(binding: Binding) -> CameraInteractionSources {
    match binding {
        Binding::Keyboard { .. } => CameraInteractionSources::KEYBOARD,
        Binding::MouseButton { .. } | Binding::MouseMotion { .. } => {
            CameraInteractionSources::MOUSE
        },
        Binding::MouseWheel { .. } => CameraInteractionSources::WHEEL,
        Binding::GamepadButton(_) | Binding::GamepadAxis(_) => CameraInteractionSources::GAMEPAD,
        Binding::Custom(_) | Binding::None => CameraInteractionSources::NONE,
        Binding::AnyKey => CameraInteractionSources::KEYBOARD
            .union(CameraInteractionSources::MOUSE)
            .union(CameraInteractionSources::GAMEPAD),
    }
}
