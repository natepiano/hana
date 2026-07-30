/// Selects whether the platform's primary shortcut modifier is `Cmd` (macOS)
/// or `Ctrl` (everywhere else).
#[derive(Clone, Copy)]
pub(super) enum PlatformShortcutMode {
    Command,
    Control,
}

impl PlatformShortcutMode {
    pub(super) const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Command
        } else {
            Self::Control
        }
    }
}
