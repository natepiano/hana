use bevy::prelude::*;
use bevy_egui::EguiContext;

/// A resource that tracks whether egui wants focus on the current and previous frames,
/// in order to determine whether `OrbitCam` should react to input events.
///
/// The reason the previous frame's value is saved is because when you click inside an
/// egui window, `Context::wants_pointer_input()` still returns false once before returning
/// true. If the camera stops taking input only when it returns false, there's one frame
/// where both egui and the camera are using the input events, which is not desirable.
///
/// This is re-exported in case it's useful. I recommend only using input events if both
/// `prev` and `curr` are false.
#[derive(Resource, PartialEq, Eq, Default)]
pub struct EguiWantsFocus {
    /// Whether egui wanted focus on the previous frame
    pub prev: bool,
    /// Whether egui wants focus on the current frame
    pub curr: bool,
}

/// Controls whether merely hovering over an egui panel/window prevents `OrbitCam`
/// from reacting to input events.
#[derive(Resource, PartialEq, Eq, Default, Clone, Copy, Debug, Reflect)]
pub enum EguiFocusIncludesHover {
    /// Only clicks inside egui panels prevent camera input.
    #[default]
    ClickOnly,
    /// Hovering over an egui panel also prevents camera input.
    IncludeHover,
}

/// Blocks an `OrbitCam` from receiving input when egui has focus.
///
/// Add this component to a camera entity to prevent it from responding
/// to orbit/pan/zoom input while the user interacts with egui.
/// Cameras without this component are unaffected by egui focus.
#[derive(Component, Reflect, Debug, Default)]
#[reflect(Component)]
pub struct BlockOnEguiFocus;

pub fn check_egui_wants_focus(
    mut contexts: Query<&mut EguiContext>,
    mut wants_focus: ResMut<EguiWantsFocus>,
    include_hover: Res<EguiFocusIncludesHover>,
) {
    // Check all egui contexts to see if any of them want focus. If any context wants focus,
    // we assume that's the one the user is interacting with and prevent camera input.
    let mut new_wants_focus = false;
    for mut context in &mut contexts {
        let context = context.get_mut();
        let mut context_wants_focus =
            context.wants_pointer_input() || context.wants_keyboard_input();
        if *include_hover == EguiFocusIncludesHover::IncludeHover {
            context_wants_focus |= context.is_pointer_over_area();
        }
        new_wants_focus |= context_wants_focus;
    }

    let new_res = EguiWantsFocus {
        prev: wants_focus.curr,
        curr: new_wants_focus,
    };
    wants_focus.set_if_neq(new_res);
}
