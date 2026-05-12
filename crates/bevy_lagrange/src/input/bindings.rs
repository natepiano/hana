use std::fmt;
use std::marker::PhantomData;

use bevy::prelude::*;

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

const ORBIT_ACTION_NAME: &str = "OrbitCamOrbitAction";
const PAN_ACTION_NAME: &str = "OrbitCamPanAction";
const ZOOM_COARSE_ACTION_NAME: &str = "OrbitCamZoomCoarseAction";
const ZOOM_SMOOTH_ACTION_NAME: &str = "OrbitCamZoomSmoothAction";

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
                .held_mouse_orbit(MouseButton::Left)
                .held_mouse_pan(MouseButton::Right)
                .wheel_from_preset(Self::SimpleMouse)
                .pinch(OrbitCamPinchBinding::Zoom)
                .build(),
            Self::BlenderLike => OrbitCamBindings::builder()
                .held_mouse_orbit(MouseButton::Middle)
                .held_mouse_pan(MouseButton::Middle)
                .wheel_from_preset(Self::BlenderLike)
                .pinch(OrbitCamPinchBinding::Zoom)
                .build(),
        }
    }
}

/// Validated runtime binding specification for an `OrbitCam`.
#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[reflect(opaque)]
pub struct OrbitCamBindings {
    orbit:            OrbitCamOrbitActionBindings,
    pan:              OrbitCamPanActionBindings,
    zoom_smooth:      OrbitCamZoomSmoothActionBindings,
    zoom_coarse:      OrbitCamZoomCoarseActionBindings,
    wheel:            OrbitCamWheelBinding,
    pinch:            OrbitCamPinchBinding,
    touch:            Option<OrbitCamTouchBinding>,
    gamepad:          CameraInputGamepadSelectionPolicy,
    zoom_direction:   ZoomDirection,
    button_drag_zoom: Option<OrbitCamButtonDragZoomBinding>,
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

    /// Returns wheel policy.
    #[must_use]
    pub const fn wheel(&self) -> OrbitCamWheelBinding { self.wheel }

    /// Returns pinch policy.
    #[must_use]
    pub const fn pinch(&self) -> OrbitCamPinchBinding { self.pinch }

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
    pub const fn button_drag_zoom(&self) -> Option<OrbitCamButtonDragZoomBinding> {
        self.button_drag_zoom
    }
}

/// Reflectable draft binding specification for editor and keymap tooling.
#[derive(Clone, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamBindingsDescriptor {
    orbit:            Vec<HeldBindingDescriptor>,
    pan:              Vec<HeldBindingDescriptor>,
    zoom_smooth:      Vec<HeldBindingDescriptor>,
    zoom_coarse:      Vec<ActionBindingDescriptor>,
    wheel:            Option<OrbitCamWheelBinding>,
    pinch:            OrbitCamPinchBinding,
    touch:            Option<OrbitCamTouchBinding>,
    gamepad:          CameraInputGamepadSelectionPolicy,
    zoom_direction:   ZoomDirection,
    button_drag_zoom: Option<OrbitCamButtonDragZoomBinding>,
}

impl TryFrom<OrbitCamBindingsDescriptor> for OrbitCamBindings {
    type Error = OrbitCamBindingsError;

    fn try_from(descriptor: OrbitCamBindingsDescriptor) -> Result<Self, Self::Error> {
        validate_bindings(&descriptor)
    }
}

/// Typestate marker for bindings builders that have not selected a wheel policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OrbitCamBindingsWheelUnset;

/// Typestate marker for bindings builders that selected a wheel policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OrbitCamBindingsWheelSet;

/// Builder for `OrbitCamBindings`.
#[derive(Clone, Debug)]
pub struct OrbitCamBindingsBuilder<WheelState = OrbitCamBindingsWheelUnset> {
    descriptor: OrbitCamBindingsDescriptor,
    wheel:      PhantomData<WheelState>,
}

impl Default for OrbitCamBindingsBuilder<OrbitCamBindingsWheelUnset> {
    fn default() -> Self {
        Self {
            descriptor: OrbitCamBindingsDescriptor::default(),
            wheel:      PhantomData,
        }
    }
}

impl<WheelState> OrbitCamBindingsBuilder<WheelState> {
    /// Adds a held mouse orbit binding.
    #[must_use]
    pub fn held_mouse_orbit(mut self, button: MouseButton) -> Self {
        self.descriptor
            .orbit
            .push(HeldBindingDescriptor::mouse_motion_with_button(
                button,
                CameraInteractionSources::MOUSE,
                BindingRoutePolicy::CursorPosition,
            ));
        self
    }

    /// Adds a held mouse pan binding.
    #[must_use]
    pub fn held_mouse_pan(mut self, button: MouseButton) -> Self {
        self.descriptor
            .pan
            .push(HeldBindingDescriptor::mouse_motion_with_button(
                button,
                CameraInteractionSources::MOUSE,
                BindingRoutePolicy::CursorPosition,
            ));
        self
    }

    /// Adds a held orbit binding from explicit enhanced-input recipes.
    #[must_use]
    pub fn held_orbit_binding(
        mut self,
        binding: HeldActionBindingEntry<OrbitCamOrbitAction>,
    ) -> Self {
        self.descriptor
            .orbit
            .push(HeldBindingDescriptor::from_held_entry(binding));
        self
    }

    /// Adds a held pan binding from explicit enhanced-input recipes.
    #[must_use]
    pub fn held_pan_binding(mut self, binding: HeldActionBindingEntry<OrbitCamPanAction>) -> Self {
        self.descriptor
            .pan
            .push(HeldBindingDescriptor::from_held_entry(binding));
        self
    }

    /// Adds a held smooth-zoom binding from explicit enhanced-input recipes.
    #[must_use]
    pub fn held_smooth_zoom_binding(
        mut self,
        binding: HeldActionBindingEntry<OrbitCamZoomSmoothAction>,
    ) -> Self {
        self.descriptor
            .zoom_smooth
            .push(HeldBindingDescriptor::from_held_entry(binding));
        self
    }

    /// Sets the pinch policy.
    #[must_use]
    pub const fn pinch(mut self, pinch: OrbitCamPinchBinding) -> Self {
        self.descriptor.pinch = pinch;
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

    /// Sets the button-drag zoom policy.
    #[must_use]
    pub const fn button_drag_zoom(
        mut self,
        button_drag_zoom: Option<OrbitCamButtonDragZoomBinding>,
    ) -> Self {
        self.descriptor.button_drag_zoom = button_drag_zoom;
        self
    }

    /// Adds a low-level enhanced-input impulse zoom binding.
    #[must_use]
    pub fn zoom_coarse_binding(mut self, binding: ActionBindingDescriptor) -> Self {
        self.descriptor.zoom_coarse.push(binding);
        self
    }
}

impl OrbitCamBindingsBuilder<OrbitCamBindingsWheelUnset> {
    /// Sets the wheel policy.
    #[must_use]
    pub fn wheel(
        mut self,
        wheel: OrbitCamWheelBinding,
    ) -> OrbitCamBindingsBuilder<OrbitCamBindingsWheelSet> {
        self.descriptor.wheel = Some(wheel);
        OrbitCamBindingsBuilder {
            descriptor: self.descriptor,
            wheel:      PhantomData,
        }
    }

    /// Copies only the wheel policy from a preset.
    #[must_use]
    pub fn wheel_from_preset(
        mut self,
        preset: OrbitCamPreset,
    ) -> OrbitCamBindingsBuilder<OrbitCamBindingsWheelSet> {
        self.descriptor.wheel = Some(wheel_binding_from_preset(preset));
        OrbitCamBindingsBuilder {
            descriptor: self.descriptor,
            wheel:      PhantomData,
        }
    }
}

impl OrbitCamBindingsBuilder<OrbitCamBindingsWheelSet> {
    /// Sets the wheel policy.
    #[must_use]
    pub const fn wheel(mut self, wheel: OrbitCamWheelBinding) -> Self {
        self.descriptor.wheel = Some(wheel);
        self
    }

    /// Copies only the wheel policy from a preset.
    #[must_use]
    pub fn wheel_from_preset(mut self, preset: OrbitCamPreset) -> Self {
        self.descriptor.wheel = Some(wheel_binding_from_preset(preset));
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

fn wheel_binding_from_preset(preset: OrbitCamPreset) -> OrbitCamWheelBinding {
    match preset {
        OrbitCamPreset::SimpleMouse => {
            OrbitCamWheelBinding::LineZoom(OrbitCamWheelModifier::default())
        },
        OrbitCamPreset::BlenderLike => {
            OrbitCamWheelBinding::BlenderLike(OrbitCamBlenderLikeWheelBinding::default())
        },
    }
}

/// Orbit action binding set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrbitCamOrbitActionBindings(
    HeldActionBindingSet<OrbitCamOrbitAction, OrbitCamOrbitEngagedAction>,
);

/// Pan action binding set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrbitCamPanActionBindings(
    HeldActionBindingSet<OrbitCamPanAction, OrbitCamPanEngagedAction>,
);

/// Smooth zoom action binding set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrbitCamZoomSmoothActionBindings(
    HeldActionBindingSet<OrbitCamZoomSmoothAction, OrbitCamZoomEngagedAction>,
);

/// Coarse zoom action binding set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionBindingEntry<A: CameraSemanticAction> {
    binding:    BindingRecipe,
    sources:    CameraInteractionSources,
    route:      BindingRoutePolicy,
    engagement: BindingEngagement,
    action:     PhantomData<A>,
}

impl<A: ImpulseCameraAction> ActionBindingEntry<A> {
    /// Creates an impulse binding from an enhanced-input recipe and explicit metadata.
    #[must_use]
    pub const fn from_enhanced_input_impulse(
        binding: BindingRecipe,
        sources: CameraInteractionSources,
        route: BindingRoutePolicy,
    ) -> Self {
        Self {
            binding,
            sources,
            route,
            engagement: BindingEngagement::Impulse,
            action: PhantomData,
        }
    }
}

impl<A: CameraSemanticAction> ActionBindingEntry<A> {
    /// Returns the binding recipe.
    #[must_use]
    pub const fn binding(&self) -> BindingRecipe { self.binding }

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldActionBindingEntry<A: HeldCameraAction> {
    motion:     BindingRecipe,
    engagement: BindingRecipe,
    sources:    CameraInteractionSources,
    route:      BindingRoutePolicy,
    action:     PhantomData<A>,
}

impl<A: HeldCameraAction> HeldActionBindingEntry<A> {
    /// Creates a held binding from paired enhanced-input recipes and explicit metadata.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when the source metadata is empty.
    pub const fn from_enhanced_input_pair(
        motion: BindingRecipe,
        engagement: BindingRecipe,
        sources: CameraInteractionSources,
        route: BindingRoutePolicy,
    ) -> Result<Self, OrbitCamBindingsError> {
        if sources.is_empty() {
            return Err(OrbitCamBindingsError::MissingSources);
        }
        Ok(Self {
            motion,
            engagement,
            sources,
            route,
            action: PhantomData,
        })
    }

    /// Returns the motion binding recipe.
    #[must_use]
    pub const fn motion(&self) -> BindingRecipe { self.motion }

    /// Returns the engagement binding recipe.
    #[must_use]
    pub const fn engagement(&self) -> BindingRecipe { self.engagement }

    /// Returns source metadata for this binding.
    #[must_use]
    pub const fn sources(&self) -> CameraInteractionSources { self.sources }

    /// Returns route policy for this binding.
    #[must_use]
    pub const fn route(&self) -> BindingRoutePolicy { self.route }
}

/// Public recipe for a single enhanced-input binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum BindingRecipe {
    /// Keyboard key binding.
    Key(KeyCode),
    /// Mouse button binding.
    MouseButton(MouseButton),
    /// Mouse motion binding.
    MouseMotion,
    /// Mouse wheel binding.
    MouseWheel,
    /// Gamepad button binding.
    GamepadButton(GamepadButton),
    /// Gamepad axis binding.
    GamepadAxis(GamepadAxis),
    /// Empty binding.
    None,
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
    /// Held binding with a separate engagement recipe.
    Held,
}

/// Mouse wheel policy for `OrbitCamBindings`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamWheelBinding {
    /// Disable wheel input.
    Disabled,
    /// Treat line-wheel input as coarse zoom with the provided modifier.
    LineZoom(OrbitCamWheelModifier),
    /// Use Blender-like pixel scroll policy.
    BlenderLike(OrbitCamBlenderLikeWheelBinding),
}

/// Blender-like wheel and smooth-scroll policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct OrbitCamBlenderLikeWheelBinding {
    /// Modifier that turns smooth scroll into pan.
    pub pan_modifier:  Option<KeyCode>,
    /// Modifier that turns smooth scroll into zoom.
    pub zoom_modifier: Option<KeyCode>,
    /// Wheel modifier applied to zoom values.
    pub wheel:         OrbitCamWheelModifier,
}

impl Default for OrbitCamBlenderLikeWheelBinding {
    fn default() -> Self {
        Self {
            pan_modifier:  Some(KeyCode::ShiftLeft),
            zoom_modifier: Some(KeyCode::ControlLeft),
            wheel:         OrbitCamWheelModifier::default(),
        }
    }
}

/// Modifier applied to wheel zoom values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamWheelModifier {
    /// Whether to invert the wheel value before applying zoom direction.
    pub inverted: bool,
}

/// Pinch gesture policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamPinchBinding {
    /// Disable pinch input.
    #[default]
    Disabled,
    /// Treat pinch input as smooth zoom.
    Zoom,
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

/// Button-drag zoom policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct OrbitCamButtonDragZoomBinding {
    /// Mouse button that engages button-drag zoom.
    pub button: MouseButton,
    /// Axis used for button-drag zoom.
    pub axis:   OrbitCamButtonDragZoomAxis,
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

/// Structured binding validation error.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OrbitCamBindingsError {
    /// No wheel policy was selected.
    MissingWheelPolicy,
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
    /// An adapter-owned source was also bound as a public enhanced-input binding.
    AdapterConflict {
        /// Conflicting source name.
        source: &'static str,
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
            Self::MissingWheelPolicy | Self::MissingSources | Self::AdapterConflict { .. } => None,
        }
    }
}

impl fmt::Display for OrbitCamBindingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingWheelPolicy => {
                formatter.write_str("custom bindings must choose a wheel policy")
            },
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
            Self::AdapterConflict { source } => {
                write!(
                    formatter,
                    "binding conflicts with Lagrange's {source} adapter"
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

impl std::error::Error for OrbitCamBindingsError {}

#[derive(Clone, Debug, PartialEq, Eq, Reflect)]
struct HeldBindingDescriptor {
    motion:             BindingRecipe,
    engagement:         Option<BindingRecipe>,
    sources:            CameraInteractionSources,
    engagement_sources: CameraInteractionSources,
    route:              BindingRoutePolicy,
}

impl HeldBindingDescriptor {
    const fn mouse_motion_with_button(
        button: MouseButton,
        sources: CameraInteractionSources,
        route: BindingRoutePolicy,
    ) -> Self {
        Self {
            motion: BindingRecipe::MouseMotion,
            engagement: Some(BindingRecipe::MouseButton(button)),
            sources,
            engagement_sources: sources,
            route,
        }
    }

    const fn from_held_entry<A: HeldCameraAction>(entry: HeldActionBindingEntry<A>) -> Self {
        Self {
            motion:             entry.motion,
            engagement:         Some(entry.engagement),
            sources:            entry.sources,
            engagement_sources: entry.sources,
            route:              entry.route,
        }
    }
}

/// Reflectable descriptor for an impulse action binding.
#[derive(Clone, Debug, PartialEq, Eq, Reflect)]
pub struct ActionBindingDescriptor {
    binding:    BindingRecipe,
    sources:    CameraInteractionSources,
    route:      BindingRoutePolicy,
    engagement: BindingEngagement,
}

impl ActionBindingDescriptor {
    /// Creates an impulse action binding descriptor.
    ///
    /// Source metadata is explicit so the resolver can later report only the
    /// sources that actually contributed to camera intent.
    #[must_use]
    pub const fn impulse(
        binding: BindingRecipe,
        sources: CameraInteractionSources,
        route: BindingRoutePolicy,
    ) -> Self {
        Self {
            binding,
            sources,
            route,
            engagement: BindingEngagement::Impulse,
        }
    }
}

/// Validates and builds `OrbitCamBindings` from a descriptor.
///
/// # Errors
///
/// Returns [`OrbitCamBindingsError`] when any binding invariant fails.
pub fn validate_bindings(
    descriptor: &OrbitCamBindingsDescriptor,
) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
    let wheel = descriptor
        .wheel
        .ok_or(OrbitCamBindingsError::MissingWheelPolicy)?;

    validate_held_entries(ORBIT_ACTION_NAME, &descriptor.orbit)?;
    validate_held_entries(PAN_ACTION_NAME, &descriptor.pan)?;
    validate_held_entries(ZOOM_SMOOTH_ACTION_NAME, &descriptor.zoom_smooth)?;
    validate_impulse_entries(ZOOM_COARSE_ACTION_NAME, &descriptor.zoom_coarse)?;
    validate_adapter_conflicts(wheel, descriptor)?;

    Ok(OrbitCamBindings {
        orbit: OrbitCamOrbitActionBindings(held_descriptors_to_set(
            ORBIT_ACTION_NAME,
            &descriptor.orbit,
        )?),
        pan: OrbitCamPanActionBindings(held_descriptors_to_set(PAN_ACTION_NAME, &descriptor.pan)?),
        zoom_smooth: OrbitCamZoomSmoothActionBindings(held_descriptors_to_set(
            ZOOM_SMOOTH_ACTION_NAME,
            &descriptor.zoom_smooth,
        )?),
        zoom_coarse: OrbitCamZoomCoarseActionBindings(ActionBindingSet {
            entries: descriptor
                .zoom_coarse
                .iter()
                .map(action_descriptor_to_entry)
                .collect(),
            action:  PhantomData,
        }),
        wheel,
        pinch: descriptor.pinch,
        touch: descriptor.touch,
        gamepad: descriptor.gamepad,
        zoom_direction: descriptor.zoom_direction,
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
    }
    Ok(())
}

fn validate_impulse_entries(
    action: &'static str,
    entries: &[ActionBindingDescriptor],
) -> Result<(), OrbitCamBindingsError> {
    for entry in entries {
        if entry.sources.is_empty() {
            return Err(OrbitCamBindingsError::MissingSources);
        }
        if entry.engagement == BindingEngagement::Held {
            return Err(OrbitCamBindingsError::ImpulseEngagement { action });
        }
    }
    Ok(())
}

fn validate_adapter_conflicts(
    wheel: OrbitCamWheelBinding,
    descriptor: &OrbitCamBindingsDescriptor,
) -> Result<(), OrbitCamBindingsError> {
    if wheel != OrbitCamWheelBinding::Disabled {
        for entry in &descriptor.zoom_coarse {
            if entry.binding == BindingRecipe::MouseWheel {
                return Err(OrbitCamBindingsError::AdapterConflict {
                    source: "mouse wheel",
                });
            }
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
        .ok_or(OrbitCamBindingsError::HeldMotionMissingEngagement { action })?;

    HeldActionBindingEntry::from_enhanced_input_pair(
        descriptor.motion,
        engagement,
        descriptor.sources,
        descriptor.route,
    )
}

const fn action_descriptor_to_entry<A: ImpulseCameraAction>(
    descriptor: &ActionBindingDescriptor,
) -> ActionBindingEntry<A> {
    ActionBindingEntry::from_enhanced_input_impulse(
        descriptor.binding,
        descriptor.sources,
        descriptor.route,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor_with_disabled_wheel() -> OrbitCamBindingsDescriptor {
        OrbitCamBindingsDescriptor {
            wheel: Some(OrbitCamWheelBinding::Disabled),
            ..OrbitCamBindingsDescriptor::default()
        }
    }

    #[test]
    fn presets_validate_through_shared_path() -> Result<(), OrbitCamBindingsError> {
        let simple = OrbitCamPreset::SimpleMouse.to_bindings()?;
        assert_eq!(
            simple.wheel(),
            OrbitCamWheelBinding::LineZoom(OrbitCamWheelModifier::default())
        );
        assert_eq!(simple.pinch(), OrbitCamPinchBinding::Zoom);

        let blender = OrbitCamPreset::BlenderLike.to_bindings()?;
        assert_eq!(
            blender.wheel(),
            OrbitCamWheelBinding::BlenderLike(OrbitCamBlenderLikeWheelBinding::default())
        );
        assert_eq!(blender.pinch(), OrbitCamPinchBinding::Zoom);

        Ok(())
    }

    #[test]
    fn wheel_from_preset_copies_only_wheel_policy() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder()
            .wheel_from_preset(OrbitCamPreset::BlenderLike)
            .build()?;

        assert!(bindings.orbit().is_empty());
        assert!(bindings.pan().is_empty());
        assert!(bindings.zoom_smooth().is_empty());
        assert!(bindings.zoom_coarse().is_empty());
        assert_eq!(bindings.pinch(), OrbitCamPinchBinding::Disabled);
        assert_eq!(
            bindings.wheel(),
            OrbitCamWheelBinding::BlenderLike(OrbitCamBlenderLikeWheelBinding::default())
        );

        Ok(())
    }

    #[test]
    fn missing_wheel_policy_is_rejected() {
        assert_eq!(
            validate_bindings(&OrbitCamBindingsDescriptor::default()),
            Err(OrbitCamBindingsError::MissingWheelPolicy)
        );
    }

    #[test]
    fn held_motion_without_engagement_is_rejected() {
        let mut descriptor = descriptor_with_disabled_wheel();
        descriptor.orbit.push(HeldBindingDescriptor {
            motion:             BindingRecipe::MouseMotion,
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
        let mut descriptor = descriptor_with_disabled_wheel();
        descriptor.pan.push(HeldBindingDescriptor {
            motion:             BindingRecipe::MouseMotion,
            engagement:         Some(BindingRecipe::Key(KeyCode::ShiftLeft)),
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
        let mut descriptor = descriptor_with_disabled_wheel();
        descriptor.zoom_coarse.push(ActionBindingDescriptor {
            binding:    BindingRecipe::Key(KeyCode::Space),
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
    fn adapter_conflict_is_rejected() {
        let mut descriptor = OrbitCamBindingsDescriptor {
            wheel: Some(OrbitCamWheelBinding::LineZoom(
                OrbitCamWheelModifier::default(),
            )),
            ..OrbitCamBindingsDescriptor::default()
        };
        descriptor
            .zoom_coarse
            .push(ActionBindingDescriptor::impulse(
                BindingRecipe::MouseWheel,
                CameraInteractionSources::WHEEL,
                BindingRoutePolicy::NoPosition,
            ));

        assert_eq!(
            validate_bindings(&descriptor),
            Err(OrbitCamBindingsError::AdapterConflict {
                source: "mouse wheel",
            })
        );
    }

    #[test]
    fn held_entry_builder_preserves_motion_and_engagement() -> Result<(), OrbitCamBindingsError> {
        let entry = HeldActionBindingEntry::<OrbitCamOrbitAction>::from_enhanced_input_pair(
            BindingRecipe::Key(KeyCode::KeyA),
            BindingRecipe::Key(KeyCode::ShiftLeft),
            CameraInteractionSources::KEYBOARD,
            BindingRoutePolicy::NoPosition,
        )?;

        let bindings = OrbitCamBindings::builder()
            .held_orbit_binding(entry)
            .wheel(OrbitCamWheelBinding::Disabled)
            .build()?;
        let [entry] = bindings.orbit().entries() else {
            assert_eq!(bindings.orbit().entries().len(), 1);
            return Ok(());
        };

        assert_eq!(entry.motion(), BindingRecipe::Key(KeyCode::KeyA));
        assert_eq!(entry.engagement(), BindingRecipe::Key(KeyCode::ShiftLeft));
        assert_eq!(entry.sources(), CameraInteractionSources::KEYBOARD);
        assert_eq!(entry.route(), BindingRoutePolicy::NoPosition);

        Ok(())
    }
}
