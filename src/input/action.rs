/// Generates a `bevy_enhanced_input` `InputAction` struct.
///
/// # Examples
///
/// ```ignore
/// use bevy_kana::action;
///
/// action!(CameraHome);
/// ```
///
/// Expands to:
///
/// ```ignore
/// #[derive(InputAction)]
/// #[action_output(bool)]
/// pub struct CameraHome;
/// ```
#[macro_export]
macro_rules! action {
    ($(#[$meta:meta])* $action:ident) => {
        $(#[$meta])*
        #[derive(InputAction)]
        #[action_output(bool)]
        pub struct $action;
    };
}
