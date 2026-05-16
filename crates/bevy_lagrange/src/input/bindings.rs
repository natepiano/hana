use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::marker::PhantomData;

use bevy::prelude::*;
use bevy_enhanced_input::prelude::Binding;
use bevy_enhanced_input::prelude::ModKeys;

use super::CameraInteractionSources;
use super::CameraSemanticAction;
use super::HeldCameraAction;
use super::ImpulseCameraAction;
use super::OrbitCamOrbitAction;
use super::OrbitCamPanAction;
use super::OrbitCamZoomCoarseAction;
use super::OrbitCamZoomSmoothAction;
use super::actions::OrbitCamOrbitEngagedAction;
use super::actions::OrbitCamPanEngagedAction;
use super::actions::OrbitCamZoomEngagedAction;
use super::constants::ORBIT_ACTION_NAME;
use super::constants::PAN_ACTION_NAME;
use super::constants::ZOOM_COARSE_ACTION_NAME;
use super::constants::ZOOM_SMOOTH_ACTION_NAME;

/// Built-in orbit-camera input presets.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[non_exhaustive]
pub enum OrbitCamPreset {
    /// Mouse-oriented default controls.
    #[default]
    SimpleMouse,
    /// Editor-oriented controls modeled after Blender navigation.
    BlenderLike,
}

impl OrbitCamPreset {
    /// Converts this preset into validated custom bindings.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] if the preset construction violates the
    /// shared binding validator.
    pub fn to_bindings(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        match self {
            Self::SimpleMouse => OrbitCamBindings::builder()
                .orbit(OrbitCamMouseDrag::new(MouseButton::Left))
                .pan(OrbitCamMouseDrag::new(MouseButton::Right))
                .zoom(OrbitCamMouseWheelZoom::default())
                .zoom(OrbitCamTrackpadScroll::default())
                .zoom(OrbitCamPinchZoom)
                .build(),
            Self::BlenderLike => OrbitCamBindings::builder()
                .orbit(OrbitCamMouseDrag::new(MouseButton::Middle))
                .orbit(OrbitCamTrackpadScroll::default())
                .pan(OrbitCamMouseDrag::new(MouseButton::Middle).with_mod_keys(ModKeys::SHIFT))
                .pan(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::SHIFT))
                .zoom(OrbitCamMouseWheelZoom::default())
                .zoom(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::CONTROL))
                .zoom(OrbitCamPinchZoom)
                .build(),
        }
    }
}

/// Validated runtime binding specification for an `OrbitCam`.
#[derive(Component, Clone, Debug, PartialEq, Reflect)]
#[reflect(Component)]
#[reflect(opaque)]
pub struct OrbitCamBindings {
    orbit:            OrbitCamOrbitActionBindings,
    pan:              OrbitCamPanActionBindings,
    zoom_smooth:      OrbitCamZoomSmoothActionBindings,
    zoom_coarse:      OrbitCamZoomCoarseActionBindings,
    trackpad_orbit:   Vec<OrbitCamTrackpadScroll>,
    trackpad_pan:     Vec<OrbitCamTrackpadScroll>,
    trackpad_zoom:    Vec<OrbitCamTrackpadScroll>,
    mouse_wheel_zoom: Option<OrbitCamMouseWheelZoom>,
    pinch_zoom:       PinchGestureZoom,
    touch:            Option<OrbitCamTouchBinding>,
    gamepad:          CameraInputGamepadSelectionPolicy,
    zoom_direction:   ZoomDirection,
    button_drag_zoom: Option<OrbitCamButtonDragZoom>,
}

/// Whether the pinch gesture is wired up as a zoom input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum PinchGestureZoom {
    /// Pinch gestures contribute to zoom.
    Enabled,
    /// Pinch gestures are ignored.
    #[default]
    Disabled,
}

impl OrbitCamBindings {
    /// Creates an `OrbitCamBindings` builder.
    #[must_use]
    pub fn builder() -> OrbitCamBindingsBuilder { OrbitCamBindingsBuilder::default() }

    /// Returns orbit action bindings.
    #[must_use]
    pub const fn orbit(&self) -> &OrbitCamOrbitActionBindings { &self.orbit }

    /// Returns pan action bindings.
    #[must_use]
    pub const fn pan(&self) -> &OrbitCamPanActionBindings { &self.pan }

    /// Returns smooth zoom action bindings.
    #[must_use]
    pub const fn zoom_smooth(&self) -> &OrbitCamZoomSmoothActionBindings { &self.zoom_smooth }

    /// Returns coarse zoom action bindings.
    #[must_use]
    pub const fn zoom_coarse(&self) -> &OrbitCamZoomCoarseActionBindings { &self.zoom_coarse }

    /// Returns trackpad orbit bindings.
    #[must_use]
    pub fn trackpad_orbit(&self) -> &[OrbitCamTrackpadScroll] { &self.trackpad_orbit }

    /// Returns trackpad pan bindings.
    #[must_use]
    pub fn trackpad_pan(&self) -> &[OrbitCamTrackpadScroll] { &self.trackpad_pan }

    /// Returns trackpad zoom bindings.
    #[must_use]
    pub fn trackpad_zoom(&self) -> &[OrbitCamTrackpadScroll] { &self.trackpad_zoom }

    /// Returns mouse wheel zoom binding.
    #[must_use]
    pub const fn mouse_wheel_zoom(&self) -> Option<OrbitCamMouseWheelZoom> { self.mouse_wheel_zoom }

    /// Returns whether pinch zoom is enabled.
    #[must_use]
    pub const fn pinch_zoom(&self) -> PinchGestureZoom { self.pinch_zoom }

    /// Returns touch policy.
    #[must_use]
    pub const fn touch(&self) -> Option<OrbitCamTouchBinding> { self.touch }

    /// Returns gamepad selection policy.
    #[must_use]
    pub const fn gamepad(&self) -> CameraInputGamepadSelectionPolicy { self.gamepad }

    /// Returns zoom direction policy.
    #[must_use]
    pub const fn zoom_direction(&self) -> ZoomDirection { self.zoom_direction }

    /// Returns button-drag zoom policy.
    #[must_use]
    pub const fn button_drag_zoom(&self) -> Option<OrbitCamButtonDragZoom> { self.button_drag_zoom }
}

/// Reflectable draft binding specification for editor and keymap tooling.
#[derive(Clone, Debug, Default, PartialEq, Reflect)]
pub struct OrbitCamBindingsDescriptor {
    orbit:            Vec<HeldBindingDescriptor>,
    pan:              Vec<HeldBindingDescriptor>,
    zoom_smooth:      Vec<HeldBindingDescriptor>,
    zoom_coarse:      Vec<ActionBindingDescriptor>,
    trackpad_orbit:   Vec<OrbitCamTrackpadScroll>,
    trackpad_pan:     Vec<OrbitCamTrackpadScroll>,
    trackpad_zoom:    Vec<OrbitCamTrackpadScroll>,
    mouse_wheel_zoom: Option<OrbitCamMouseWheelZoom>,
    pinch_zoom:       PinchGestureZoom,
    touch:            Option<OrbitCamTouchBinding>,
    gamepad:          CameraInputGamepadSelectionPolicy,
    zoom_direction:   ZoomDirection,
    button_drag_zoom: Option<OrbitCamButtonDragZoom>,
}

impl TryFrom<OrbitCamBindingsDescriptor> for OrbitCamBindings {
    type Error = OrbitCamBindingsError;

    fn try_from(descriptor: OrbitCamBindingsDescriptor) -> Result<Self, Self::Error> {
        validate_bindings(&descriptor)
    }
}

#[cfg(test)]
pub(crate) fn invalid_bindings_descriptor_for_tests() -> OrbitCamBindingsDescriptor {
    let mut descriptor = OrbitCamBindingsDescriptor::default();
    descriptor.orbit.push(HeldBindingDescriptor {
        motion:             OrbitCamInputBinding::from(Binding::mouse_motion()).descriptor(),
        engagement:         None,
        sources:            CameraInteractionSources::MOUSE,
        engagement_sources: CameraInteractionSources::MOUSE,
        route:              BindingRoutePolicy::CursorPosition,
    });
    descriptor
}

/// Builder for `OrbitCamBindings`.
#[derive(Clone, Debug, Default)]
pub struct OrbitCamBindingsBuilder {
    descriptor: OrbitCamBindingsDescriptor,
}

impl OrbitCamBindingsBuilder {
    /// Adds a binding that produces orbit intent.
    #[must_use]
    pub fn orbit(mut self, binding: impl Into<OrbitCamOrbitBinding>) -> Self {
        match binding.into() {
            OrbitCamOrbitBinding::Held(binding) => self.descriptor.orbit.push(binding.into()),
            OrbitCamOrbitBinding::Trackpad(binding) => self.descriptor.trackpad_orbit.push(binding),
        }
        self
    }

    /// Adds a binding that produces pan intent.
    #[must_use]
    pub fn pan(mut self, binding: impl Into<OrbitCamPanBinding>) -> Self {
        match binding.into() {
            OrbitCamPanBinding::Held(binding) => self.descriptor.pan.push(binding.into()),
            OrbitCamPanBinding::Trackpad(binding) => self.descriptor.trackpad_pan.push(binding),
        }
        self
    }

    /// Adds a binding that produces zoom intent.
    #[must_use]
    pub fn zoom(mut self, binding: impl Into<OrbitCamZoomBinding>) -> Self {
        match binding.into() {
            OrbitCamZoomBinding::Held(binding) => {
                self.descriptor.zoom_smooth.push(binding.into());
            },
            OrbitCamZoomBinding::Trackpad(binding) => self.descriptor.trackpad_zoom.push(binding),
            OrbitCamZoomBinding::MouseWheel(binding) => {
                self.descriptor.mouse_wheel_zoom = Some(binding);
            },
            OrbitCamZoomBinding::Pinch(_) => {
                self.descriptor.pinch_zoom = PinchGestureZoom::Enabled;
            },
            OrbitCamZoomBinding::ButtonDrag(binding) => {
                self.descriptor.button_drag_zoom = Some(binding);
            },
        }
        self
    }

    /// Sets the touch policy.
    #[must_use]
    pub const fn touch(mut self, touch: Option<OrbitCamTouchBinding>) -> Self {
        self.descriptor.touch = touch;
        self
    }

    /// Sets the gamepad selection policy.
    #[must_use]
    pub const fn gamepad(mut self, gamepad: CameraInputGamepadSelectionPolicy) -> Self {
        self.descriptor.gamepad = gamepad;
        self
    }

    /// Sets the zoom direction policy.
    #[must_use]
    pub const fn zoom_direction(mut self, zoom_direction: ZoomDirection) -> Self {
        self.descriptor.zoom_direction = zoom_direction;
        self
    }

    /// Builds validated `OrbitCamBindings`.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when the descriptor violates a binding
    /// invariant.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        validate_bindings(&self.descriptor)
    }
}

/// Binding that can produce orbit intent.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamOrbitBinding {
    /// Held enhanced-input binding.
    Held(OrbitCamHeldBinding),
    /// Trackpad smooth-scroll binding.
    Trackpad(OrbitCamTrackpadScroll),
}

impl From<OrbitCamHeldBinding> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamHeldBinding) -> Self { Self::Held(value) }
}

impl From<OrbitCamMouseDrag> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamMouseDrag) -> Self { Self::Held(value.into()) }
}

impl From<OrbitCamInputBinding> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamInputBinding) -> Self { Self::Held(OrbitCamHeldBinding::same(value)) }
}

impl From<OrbitCamTrackpadScroll> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamTrackpadScroll) -> Self { Self::Trackpad(value) }
}

/// Binding that can produce pan intent.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamPanBinding {
    /// Held enhanced-input binding.
    Held(OrbitCamHeldBinding),
    /// Trackpad smooth-scroll binding.
    Trackpad(OrbitCamTrackpadScroll),
}

impl From<OrbitCamHeldBinding> for OrbitCamPanBinding {
    fn from(value: OrbitCamHeldBinding) -> Self { Self::Held(value) }
}

impl From<OrbitCamMouseDrag> for OrbitCamPanBinding {
    fn from(value: OrbitCamMouseDrag) -> Self { Self::Held(value.into()) }
}

impl From<OrbitCamInputBinding> for OrbitCamPanBinding {
    fn from(value: OrbitCamInputBinding) -> Self { Self::Held(OrbitCamHeldBinding::same(value)) }
}

impl From<OrbitCamTrackpadScroll> for OrbitCamPanBinding {
    fn from(value: OrbitCamTrackpadScroll) -> Self { Self::Trackpad(value) }
}

/// Binding that can produce zoom intent.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamZoomBinding {
    /// Held enhanced-input binding.
    Held(OrbitCamHeldBinding),
    /// Trackpad smooth-scroll binding.
    Trackpad(OrbitCamTrackpadScroll),
    /// Mouse wheel zoom binding.
    MouseWheel(OrbitCamMouseWheelZoom),
    /// Pinch gesture zoom binding.
    Pinch(OrbitCamPinchZoom),
    /// Button-drag zoom binding.
    ButtonDrag(OrbitCamButtonDragZoom),
}

impl From<OrbitCamHeldBinding> for OrbitCamZoomBinding {
    fn from(value: OrbitCamHeldBinding) -> Self { Self::Held(value) }
}

impl From<OrbitCamInputBinding> for OrbitCamZoomBinding {
    fn from(value: OrbitCamInputBinding) -> Self { Self::Held(OrbitCamHeldBinding::same(value)) }
}

impl From<OrbitCamTrackpadScroll> for OrbitCamZoomBinding {
    fn from(value: OrbitCamTrackpadScroll) -> Self { Self::Trackpad(value) }
}

impl From<OrbitCamMouseWheelZoom> for OrbitCamZoomBinding {
    fn from(value: OrbitCamMouseWheelZoom) -> Self { Self::MouseWheel(value) }
}

impl From<OrbitCamPinchZoom> for OrbitCamZoomBinding {
    fn from(value: OrbitCamPinchZoom) -> Self { Self::Pinch(value) }
}

impl From<OrbitCamButtonDragZoom> for OrbitCamZoomBinding {
    fn from(value: OrbitCamButtonDragZoom) -> Self { Self::ButtonDrag(value) }
}

/// A held enhanced-input binding made from a value binding and an engagement binding.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct OrbitCamHeldBinding {
    motion:     OrbitCamInputBinding,
    engagement: OrbitCamInputBinding,
    sources:    CameraInteractionSources,
    route:      BindingRoutePolicy,
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

    fn descriptor(&self) -> InputBindingDescriptor {
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

/// Mouse-drag binding for orbit or pan behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct OrbitCamMouseDrag {
    /// Mouse button that engages the drag.
    pub button:   MouseButton,
    /// Keyboard modifiers required by both motion and button engagement.
    pub mod_keys: ModKeys,
}

impl OrbitCamMouseDrag {
    /// Creates a mouse-drag binding without keyboard modifiers.
    #[must_use]
    pub const fn new(button: MouseButton) -> Self {
        Self {
            button,
            mod_keys: ModKeys::empty(),
        }
    }

    /// Requires keyboard modifiers on both mouse motion and button engagement.
    #[must_use]
    pub const fn with_mod_keys(mut self, mod_keys: ModKeys) -> Self {
        self.mod_keys = mod_keys;
        self
    }
}

impl From<OrbitCamMouseDrag> for OrbitCamHeldBinding {
    fn from(value: OrbitCamMouseDrag) -> Self {
        Self::new(
            Binding::MouseMotion {
                mod_keys: value.mod_keys,
            },
            Binding::MouseButton {
                button:   value.button,
                mod_keys: value.mod_keys,
            },
        )
        .with_sources(CameraInteractionSources::MOUSE)
        .with_route(BindingRoutePolicy::CursorPosition)
    }
}

/// Trackpad smooth-scroll binding for orbit, pan, or zoom behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamTrackpadScroll {
    /// Keyboard modifiers required by the smooth-scroll binding.
    pub mod_keys: ModKeys,
}

impl OrbitCamTrackpadScroll {
    /// Requires keyboard modifiers on smooth-scroll input.
    #[must_use]
    pub const fn with_mod_keys(mut self, mod_keys: ModKeys) -> Self {
        self.mod_keys = mod_keys;
        self
    }
}

/// Mouse-wheel zoom binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamMouseWheelZoom {
    /// Wheel polarity applied before zoom direction.
    pub polarity: WheelZoomPolarity,
}

/// Wheel polarity applied before zoom direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum WheelZoomPolarity {
    /// Wheel value passes through unchanged.
    #[default]
    Normal,
    /// Wheel value is negated before zoom direction is applied.
    Inverted,
}

/// Pinch gesture zoom binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamPinchZoom;

/// Button-drag zoom binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct OrbitCamButtonDragZoom {
    /// Mouse button that engages button-drag zoom.
    pub button: MouseButton,
    /// Axis used for button-drag zoom.
    pub axis:   OrbitCamButtonDragZoomAxis,
}

/// Touch gesture policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamTouchBinding {
    /// One finger orbits and two fingers pan and zoom.
    OneFingerOrbit,
    /// One finger pans and two fingers orbit and zoom.
    TwoFingerOrbit,
}

/// Direction of scroll/zoom input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum ZoomDirection {
    /// Scrolling zooms in the default direction.
    #[default]
    Normal,
    /// Scrolling zooms in the opposite direction.
    Reversed,
}

/// Axis used for button-drag zoom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamButtonDragZoomAxis {
    /// Horizontal motion controls zoom.
    X,
    /// Vertical motion controls zoom.
    #[default]
    Y,
    /// Horizontal plus vertical motion controls zoom.
    XY,
}

/// Gamepad routing policy for camera input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum CameraInputGamepadSelectionPolicy {
    /// Ignore gamepad input.
    #[default]
    Disabled,
    /// Route a single gamepad through active camera routing.
    Active,
}

/// Orbit action binding set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrbitCamOrbitActionBindings(
    HeldActionBindingSet<OrbitCamOrbitAction, OrbitCamOrbitEngagedAction>,
);

/// Pan action binding set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrbitCamPanActionBindings(
    HeldActionBindingSet<OrbitCamPanAction, OrbitCamPanEngagedAction>,
);

/// Smooth zoom action binding set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrbitCamZoomSmoothActionBindings(
    HeldActionBindingSet<OrbitCamZoomSmoothAction, OrbitCamZoomEngagedAction>,
);

/// Coarse zoom action binding set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrbitCamZoomCoarseActionBindings(ActionBindingSet<OrbitCamZoomCoarseAction>);

impl OrbitCamOrbitActionBindings {
    /// Returns the number of orbit bindings.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Returns `true` when there are no orbit bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns orbit binding entries.
    #[must_use]
    pub fn entries(&self) -> &[HeldActionBindingEntry<OrbitCamOrbitAction>] { self.0.entries() }
}

impl OrbitCamPanActionBindings {
    /// Returns the number of pan bindings.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Returns `true` when there are no pan bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns pan binding entries.
    #[must_use]
    pub fn entries(&self) -> &[HeldActionBindingEntry<OrbitCamPanAction>] { self.0.entries() }
}

impl OrbitCamZoomSmoothActionBindings {
    /// Returns the number of smooth-zoom bindings.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Returns `true` when there are no smooth-zoom bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns smooth-zoom binding entries.
    #[must_use]
    pub fn entries(&self) -> &[HeldActionBindingEntry<OrbitCamZoomSmoothAction>] {
        self.0.entries()
    }
}

impl OrbitCamZoomCoarseActionBindings {
    /// Returns the number of coarse-zoom bindings.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Returns `true` when there are no coarse-zoom bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns coarse-zoom binding entries.
    #[must_use]
    pub fn entries(&self) -> &[ActionBindingEntry<OrbitCamZoomCoarseAction>] { self.0.entries() }
}

/// Binding set for one semantic action.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionBindingSet<A: CameraSemanticAction> {
    entries: Vec<ActionBindingEntry<A>>,
    action:  PhantomData<A>,
}

impl<A: CameraSemanticAction> ActionBindingSet<A> {
    /// Returns the number of bindings in the set.
    #[must_use]
    pub const fn len(&self) -> usize { self.entries.len() }

    /// Returns `true` when the set has no bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// Returns the binding entries.
    #[must_use]
    pub fn entries(&self) -> &[ActionBindingEntry<A>] { &self.entries }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct HeldActionBindingSet<A: HeldCameraAction, E: CameraSemanticAction> {
    entries: Vec<HeldActionBindingEntry<A>>,
    action:  PhantomData<(A, E)>,
}

impl<A: HeldCameraAction, E: CameraSemanticAction> HeldActionBindingSet<A, E> {
    const fn len(&self) -> usize { self.entries.len() }

    const fn is_empty(&self) -> bool { self.entries.is_empty() }

    fn entries(&self) -> &[HeldActionBindingEntry<A>] { &self.entries }
}

/// Binding entry for an impulse camera action.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionBindingEntry<A: CameraSemanticAction> {
    binding:    InputBindingDescriptor,
    sources:    CameraInteractionSources,
    route:      BindingRoutePolicy,
    engagement: BindingEngagement,
    action:     PhantomData<A>,
}

impl<A: CameraSemanticAction> ActionBindingEntry<A> {
    pub(crate) const fn binding_descriptor(&self) -> &InputBindingDescriptor { &self.binding }

    /// Returns source metadata for this binding.
    #[must_use]
    pub const fn sources(&self) -> CameraInteractionSources { self.sources }

    /// Returns route policy for this binding.
    #[must_use]
    pub const fn route(&self) -> BindingRoutePolicy { self.route }

    /// Returns engagement kind for this binding.
    #[must_use]
    pub const fn engagement(&self) -> BindingEngagement { self.engagement }
}

/// Paired movement and engagement entry for held camera actions.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldActionBindingEntry<A: HeldCameraAction> {
    motion:     InputBindingDescriptor,
    engagement: InputBindingDescriptor,
    sources:    CameraInteractionSources,
    route:      BindingRoutePolicy,
    action:     PhantomData<A>,
}

impl<A: HeldCameraAction> HeldActionBindingEntry<A> {
    /// Creates a held binding from BEI-style input bindings.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when the source metadata is empty.
    pub fn new(binding: OrbitCamHeldBinding) -> Result<Self, OrbitCamBindingsError> {
        if binding.sources.is_empty() {
            return Err(OrbitCamBindingsError::MissingSources);
        }
        Ok(Self {
            motion:     binding.motion.descriptor(),
            engagement: binding.engagement.descriptor(),
            sources:    binding.sources,
            route:      binding.route,
            action:     PhantomData,
        })
    }

    pub(crate) const fn motion_descriptor(&self) -> &InputBindingDescriptor { &self.motion }

    pub(crate) const fn engagement_descriptor(&self) -> &InputBindingDescriptor { &self.engagement }

    /// Returns source metadata for this binding.
    #[must_use]
    pub const fn sources(&self) -> CameraInteractionSources { self.sources }

    /// Returns route policy for this binding.
    #[must_use]
    pub const fn route(&self) -> BindingRoutePolicy { self.route }
}

/// Route policy attached to a binding entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum BindingRoutePolicy {
    /// Route by cursor or touch position.
    #[default]
    CursorPosition,
    /// Route without position when a latch or explicit owner exists.
    NoPosition,
}

/// Whether a binding is an impulse or held input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum BindingEngagement {
    /// One-frame or impulse binding.
    Impulse,
    /// Held binding with a separate engagement binding.
    Held,
}

/// Structured binding validation error.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OrbitCamBindingsError {
    /// A binding entry did not provide source metadata.
    MissingSources,
    /// A held action has motion without engagement.
    HeldMotionMissingEngagement {
        /// Semantic action name.
        action: &'static str,
    },
    /// An impulse action was configured with held engagement.
    ImpulseEngagement {
        /// Semantic action name.
        action: &'static str,
    },
    /// Held motion and engagement metadata did not match.
    HeldSourceMismatch {
        /// Semantic action name.
        action: &'static str,
    },
}

impl OrbitCamBindingsError {
    /// Returns the semantic action name attached to the error, when available.
    #[must_use]
    pub const fn action_name(&self) -> Option<&'static str> {
        match self {
            Self::HeldMotionMissingEngagement { action }
            | Self::ImpulseEngagement { action }
            | Self::HeldSourceMismatch { action } => Some(*action),
            Self::MissingSources => None,
        }
    }
}

impl Display for OrbitCamBindingsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSources => formatter.write_str("binding source metadata is missing"),
            Self::HeldMotionMissingEngagement { action } => {
                write!(
                    formatter,
                    "{action} is a held binding but has no engagement binding"
                )
            },
            Self::ImpulseEngagement { action } => {
                write!(
                    formatter,
                    "{action} is an impulse binding and cannot have an engagement action"
                )
            },
            Self::HeldSourceMismatch { action } => {
                write!(
                    formatter,
                    "{action} motion and engagement bindings do not share source metadata"
                )
            },
        }
    }
}

impl Error for OrbitCamBindingsError {}

#[derive(Clone, Debug, PartialEq, Reflect)]
struct HeldBindingDescriptor {
    motion:             InputBindingDescriptor,
    engagement:         Option<InputBindingDescriptor>,
    sources:            CameraInteractionSources,
    engagement_sources: CameraInteractionSources,
    route:              BindingRoutePolicy,
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

/// Reflectable descriptor for an impulse action binding.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct ActionBindingDescriptor {
    binding:    InputBindingDescriptor,
    sources:    CameraInteractionSources,
    route:      BindingRoutePolicy,
    engagement: BindingEngagement,
}

#[derive(Clone, Debug, Default, PartialEq, Reflect)]
pub(crate) struct InputBindingDescriptor {
    entries: Vec<InputBindingEntry>,
}

impl InputBindingDescriptor {
    fn single(binding: Binding) -> Self {
        Self {
            entries: vec![InputBindingEntry::new(binding, InputBindingTransform::None)],
        }
    }

    fn entries<const N: usize>(entries: [InputBindingEntry; N]) -> Self {
        Self {
            entries: entries.into(),
        }
    }

    pub(crate) fn entries_slice(&self) -> &[InputBindingEntry] { &self.entries }

    const fn is_empty(&self) -> bool { self.entries.is_empty() }

    pub(crate) fn is_active(
        &self,
        keyboard: Option<&ButtonInput<KeyCode>>,
        mouse_buttons: Option<&ButtonInput<MouseButton>>,
    ) -> bool {
        self.entries
            .iter()
            .any(|entry| binding_active(entry.binding, keyboard, mouse_buttons))
    }

    pub(crate) fn mouse_button_engagement(&self) -> Option<(MouseButton, ModKeys)> {
        self.entries.iter().find_map(|entry| match entry.binding {
            Binding::MouseButton { button, mod_keys } => Some((button, mod_keys)),
            Binding::Keyboard { .. }
            | Binding::MouseMotion { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadButton(_)
            | Binding::GamepadAxis(_)
            | Binding::AnyKey
            | Binding::None => None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(crate) struct InputBindingEntry {
    pub(crate) binding:   Binding,
    pub(crate) transform: InputBindingTransform,
}

impl InputBindingEntry {
    const fn new(binding: Binding, transform: InputBindingTransform) -> Self {
        Self { binding, transform }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub(crate) enum InputBindingTransform {
    None,
    Negate,
    Swizzle,
    SwizzleNegate,
}

/// Validates and builds `OrbitCamBindings` from a descriptor.
///
/// # Errors
///
/// Returns [`OrbitCamBindingsError`] when any binding invariant fails.
pub fn validate_bindings(
    descriptor: &OrbitCamBindingsDescriptor,
) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
    validate_held_entries(ORBIT_ACTION_NAME, &descriptor.orbit)?;
    validate_held_entries(PAN_ACTION_NAME, &descriptor.pan)?;
    validate_held_entries(ZOOM_SMOOTH_ACTION_NAME, &descriptor.zoom_smooth)?;
    validate_impulse_entries(ZOOM_COARSE_ACTION_NAME, &descriptor.zoom_coarse)?;

    Ok(OrbitCamBindings {
        orbit:            OrbitCamOrbitActionBindings(held_descriptors_to_set(
            ORBIT_ACTION_NAME,
            &descriptor.orbit,
        )?),
        pan:              OrbitCamPanActionBindings(held_descriptors_to_set(
            PAN_ACTION_NAME,
            &descriptor.pan,
        )?),
        zoom_smooth:      OrbitCamZoomSmoothActionBindings(held_descriptors_to_set(
            ZOOM_SMOOTH_ACTION_NAME,
            &descriptor.zoom_smooth,
        )?),
        zoom_coarse:      OrbitCamZoomCoarseActionBindings(ActionBindingSet {
            entries: descriptor
                .zoom_coarse
                .iter()
                .map(action_descriptor_to_entry)
                .collect(),
            action:  PhantomData,
        }),
        trackpad_orbit:   descriptor.trackpad_orbit.clone(),
        trackpad_pan:     descriptor.trackpad_pan.clone(),
        trackpad_zoom:    descriptor.trackpad_zoom.clone(),
        mouse_wheel_zoom: descriptor.mouse_wheel_zoom,
        pinch_zoom:       descriptor.pinch_zoom,
        touch:            descriptor.touch,
        gamepad:          descriptor.gamepad,
        zoom_direction:   descriptor.zoom_direction,
        button_drag_zoom: descriptor.button_drag_zoom,
    })
}

fn validate_held_entries(
    action: &'static str,
    entries: &[HeldBindingDescriptor],
) -> Result<(), OrbitCamBindingsError> {
    for entry in entries {
        if entry.sources.is_empty() {
            return Err(OrbitCamBindingsError::MissingSources);
        }
        if entry.engagement.is_none() {
            return Err(OrbitCamBindingsError::HeldMotionMissingEngagement { action });
        }
        if entry.sources != entry.engagement_sources {
            return Err(OrbitCamBindingsError::HeldSourceMismatch { action });
        }
        if entry.motion.is_empty()
            || entry
                .engagement
                .as_ref()
                .is_some_and(InputBindingDescriptor::is_empty)
        {
            return Err(OrbitCamBindingsError::MissingSources);
        }
    }
    Ok(())
}

fn validate_impulse_entries(
    action: &'static str,
    entries: &[ActionBindingDescriptor],
) -> Result<(), OrbitCamBindingsError> {
    for entry in entries {
        if entry.sources.is_empty() || entry.binding.is_empty() {
            return Err(OrbitCamBindingsError::MissingSources);
        }
        if entry.engagement == BindingEngagement::Held {
            return Err(OrbitCamBindingsError::ImpulseEngagement { action });
        }
    }
    Ok(())
}

fn held_descriptors_to_set<A: HeldCameraAction, E: CameraSemanticAction>(
    action: &'static str,
    descriptors: &[HeldBindingDescriptor],
) -> Result<HeldActionBindingSet<A, E>, OrbitCamBindingsError> {
    Ok(HeldActionBindingSet {
        entries: descriptors
            .iter()
            .map(|descriptor| held_descriptor_to_entry(action, descriptor))
            .collect::<Result<Vec<_>, _>>()?,
        action:  PhantomData,
    })
}

fn held_descriptor_to_entry<A: HeldCameraAction>(
    action: &'static str,
    descriptor: &HeldBindingDescriptor,
) -> Result<HeldActionBindingEntry<A>, OrbitCamBindingsError> {
    let engagement = descriptor
        .engagement
        .clone()
        .ok_or(OrbitCamBindingsError::HeldMotionMissingEngagement { action })?;

    Ok(HeldActionBindingEntry {
        motion: descriptor.motion.clone(),
        engagement,
        sources: descriptor.sources,
        route: descriptor.route,
        action: PhantomData,
    })
}

fn action_descriptor_to_entry<A: ImpulseCameraAction>(
    descriptor: &ActionBindingDescriptor,
) -> ActionBindingEntry<A> {
    ActionBindingEntry {
        binding:    descriptor.binding.clone(),
        sources:    descriptor.sources,
        route:      descriptor.route,
        engagement: BindingEngagement::Impulse,
        action:     PhantomData,
    }
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

fn binding_active(
    binding: Binding,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
) -> bool {
    match binding {
        Binding::Keyboard { key, mod_keys } => keyboard
            .is_some_and(|keyboard| keyboard.pressed(key) && mod_keys_pressed(keyboard, mod_keys)),
        Binding::MouseButton { button, mod_keys } => {
            mouse_buttons.is_some_and(|buttons| buttons.pressed(button))
                && keyboard.is_some_and(|keyboard| mod_keys_pressed(keyboard, mod_keys))
        },
        Binding::AnyKey => {
            keyboard.is_some_and(|keyboard| keyboard.get_pressed().next().is_some())
                || mouse_buttons
                    .is_some_and(|mouse_buttons| mouse_buttons.get_pressed().next().is_some())
        },
        Binding::MouseMotion { .. }
        | Binding::MouseWheel { .. }
        | Binding::GamepadButton(_)
        | Binding::GamepadAxis(_)
        | Binding::None => false,
    }
}

pub(crate) fn mod_keys_pressed(keyboard: &ButtonInput<KeyCode>, mod_keys: ModKeys) -> bool {
    mod_keys.iter_keys().all(|keys| keyboard.any_pressed(keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor_with_no_bindings() -> OrbitCamBindingsDescriptor {
        OrbitCamBindingsDescriptor::default()
    }

    #[test]
    fn presets_validate_through_shared_path() -> Result<(), OrbitCamBindingsError> {
        let simple = OrbitCamPreset::SimpleMouse.to_bindings()?;
        assert!(simple.mouse_wheel_zoom().is_some());
        assert_eq!(simple.trackpad_zoom().len(), 1);
        assert_eq!(simple.pinch_zoom(), PinchGestureZoom::Enabled);
        assert!(simple.touch().is_none());

        let blender = OrbitCamPreset::BlenderLike.to_bindings()?;
        assert_eq!(blender.orbit().len(), 1);
        assert_eq!(blender.pan().len(), 1);
        assert_eq!(blender.trackpad_orbit().len(), 1);
        assert_eq!(blender.trackpad_pan().len(), 1);
        assert_eq!(blender.trackpad_zoom().len(), 1);
        assert!(blender.mouse_wheel_zoom().is_some());
        assert_eq!(blender.pinch_zoom(), PinchGestureZoom::Enabled);

        let [pan] = blender.pan().entries() else {
            assert_eq!(blender.pan().entries().len(), 1);
            return Ok(());
        };
        assert_eq!(
            pan.engagement_descriptor().mouse_button_engagement(),
            Some((MouseButton::Middle, ModKeys::SHIFT))
        );

        Ok(())
    }

    #[test]
    fn empty_bindings_are_valid() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder().build()?;

        assert!(bindings.orbit().is_empty());
        assert!(bindings.pan().is_empty());
        assert!(bindings.zoom_smooth().is_empty());
        assert!(bindings.zoom_coarse().is_empty());
        assert!(bindings.trackpad_orbit().is_empty());
        assert!(bindings.trackpad_pan().is_empty());
        assert!(bindings.trackpad_zoom().is_empty());
        assert_eq!(bindings.pinch_zoom(), PinchGestureZoom::Disabled);
        assert!(bindings.mouse_wheel_zoom().is_none());

        Ok(())
    }

    #[test]
    fn held_motion_without_engagement_is_rejected() {
        let mut descriptor = descriptor_with_no_bindings();
        descriptor.orbit.push(HeldBindingDescriptor {
            motion:             OrbitCamInputBinding::from(Binding::mouse_motion()).descriptor(),
            engagement:         None,
            sources:            CameraInteractionSources::MOUSE,
            engagement_sources: CameraInteractionSources::MOUSE,
            route:              BindingRoutePolicy::CursorPosition,
        });

        assert_eq!(
            validate_bindings(&descriptor),
            Err(OrbitCamBindingsError::HeldMotionMissingEngagement {
                action: ORBIT_ACTION_NAME,
            })
        );
    }

    #[test]
    fn held_source_mismatch_is_rejected() {
        let mut descriptor = descriptor_with_no_bindings();
        descriptor.pan.push(HeldBindingDescriptor {
            motion:             OrbitCamInputBinding::from(Binding::mouse_motion()).descriptor(),
            engagement:         Some(OrbitCamInputBinding::from(KeyCode::ShiftLeft).descriptor()),
            sources:            CameraInteractionSources::MOUSE,
            engagement_sources: CameraInteractionSources::KEYBOARD,
            route:              BindingRoutePolicy::CursorPosition,
        });

        assert_eq!(
            validate_bindings(&descriptor),
            Err(OrbitCamBindingsError::HeldSourceMismatch {
                action: PAN_ACTION_NAME,
            })
        );
    }

    #[test]
    fn impulse_engagement_is_rejected() {
        let mut descriptor = descriptor_with_no_bindings();
        descriptor.zoom_coarse.push(ActionBindingDescriptor {
            binding:    OrbitCamInputBinding::from(KeyCode::Space).descriptor(),
            sources:    CameraInteractionSources::KEYBOARD,
            route:      BindingRoutePolicy::NoPosition,
            engagement: BindingEngagement::Held,
        });

        assert_eq!(
            validate_bindings(&descriptor),
            Err(OrbitCamBindingsError::ImpulseEngagement {
                action: ZOOM_COARSE_ACTION_NAME,
            })
        );
    }

    #[test]
    fn held_binding_preserves_bei_bindings() -> Result<(), OrbitCamBindingsError> {
        let binding = OrbitCamHeldBinding::new(KeyCode::KeyA, KeyCode::ShiftLeft);

        let bindings = OrbitCamBindings::builder().orbit(binding).build()?;
        let [entry] = bindings.orbit().entries() else {
            assert_eq!(bindings.orbit().entries().len(), 1);
            return Ok(());
        };

        assert!(entry.sources().contains(CameraInteractionSources::KEYBOARD));
        assert_eq!(entry.route(), BindingRoutePolicy::NoPosition);

        Ok(())
    }
}
