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
use super::descriptor::InputBindingDescriptor;
use super::descriptor::InputBindingEntry;
use super::descriptor::InputBindingTransform;
use crate::input::CameraInteractionSources;

/// A held enhanced-input binding made from a value binding and an engagement binding.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct OrbitCamHeldBinding {
    pub(super) motion:     OrbitCamInputBinding,
    pub(super) engagement: OrbitCamInputBinding,
    pub(super) sources:    CameraInteractionSources,
    pub(super) route:      BindingRoutePolicy,
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
            sources,
            route,
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
}

impl From<OrbitCamHeldBinding> for HeldBindingDescriptor {
    fn from(binding: OrbitCamHeldBinding) -> Self {
        Self {
            motion:             binding.motion.descriptor(),
            engagement:         Some(binding.engagement.descriptor()),
            sources:            binding.sources,
            engagement_sources: binding.sources,
            route:              binding.route,
        }
    }
}

/// A BEI-style input binding plus `OrbitCam` composite helpers.
#[derive(Clone, Debug, PartialEq, Reflect)]
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

    pub(super) fn descriptor(&self) -> InputBindingDescriptor {
        match *self {
            Self::Binding(binding) => InputBindingDescriptor::single(binding),
            Self::CardinalKeys(north, east, south, west) => InputBindingDescriptor::entries([
                InputBindingEntry::new(Binding::from(east), InputBindingTransform::None),
                InputBindingEntry::new(Binding::from(west), InputBindingTransform::Negate),
                InputBindingEntry::new(Binding::from(north), InputBindingTransform::Swizzle),
                InputBindingEntry::new(Binding::from(south), InputBindingTransform::SwizzleNegate),
            ]),
            Self::BidirectionalKeys(positive, negative) => InputBindingDescriptor::entries([
                InputBindingEntry::new(Binding::from(positive), InputBindingTransform::None),
                InputBindingEntry::new(Binding::from(negative), InputBindingTransform::Negate),
            ]),
            Self::GamepadAxes2d(x, y) => InputBindingDescriptor::entries([
                InputBindingEntry::new(Binding::GamepadAxis(x), InputBindingTransform::None),
                InputBindingEntry::new(Binding::GamepadAxis(y), InputBindingTransform::Swizzle),
            ]),
            Self::BidirectionalGamepadButtons(positive, negative) => {
                InputBindingDescriptor::entries([
                    InputBindingEntry::new(
                        Binding::GamepadButton(positive),
                        InputBindingTransform::None,
                    ),
                    InputBindingEntry::new(
                        Binding::GamepadButton(negative),
                        InputBindingTransform::Negate,
                    ),
                ])
            },
        }
    }

    const fn sources(&self) -> CameraInteractionSources {
        match *self {
            Self::Binding(binding) => sources_for_binding(binding),
            Self::CardinalKeys(..) | Self::BidirectionalKeys(..) => {
                CameraInteractionSources::KEYBOARD
            },
            Self::GamepadAxes2d(..) | Self::BidirectionalGamepadButtons(..) => {
                CameraInteractionSources::GAMEPAD
            },
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
        Binding::AnyKey => CameraInteractionSources::KEYBOARD
            .union(CameraInteractionSources::MOUSE)
            .union(CameraInteractionSources::GAMEPAD),
        Binding::None => CameraInteractionSources::NONE,
    }
}
