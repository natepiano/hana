use bevy::prelude::Reflect;

/// Controls how a [`KeymapCommand`](super::KeymapCommand) can be bound in a keymap.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
pub enum Capability {
    /// The default capability; the command binds to any keystroke sequence.
    #[default]
    OneShot,
    /// The command binds to one keystroke, including `f`, `shift-f`, or `cmd-shift-f`, but not a
    /// multi-key sequence.
    ///
    /// Keymap validation rejects a `Held` command bound to a sequence. This is a command
    /// property, not a keymap-format limitation.
    Held,
    /// The command is available only through permanently registered application bindings.
    ///
    /// Keymap validation rejects entries that name an `Unremappable` command because it is
    /// reserved for recovery.
    Unremappable,
}
