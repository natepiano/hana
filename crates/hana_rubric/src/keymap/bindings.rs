//! The live keymap's command-to-keystroke table, read by user interfaces that
//! present a command alongside the keys that run it.

use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::CommandId;
use crate::KeystrokeSequence;

/// What the live keymap resolves one declared command's keystroke to.
///
/// A command that is declared but bound to nothing is a different state from a
/// command that is bound, and the two are rendered differently: a bound command
/// shows its keystroke, an unbound one shows nothing at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandKeystroke<'keymap> {
    /// The live keymap runs the command from this keystroke sequence, which
    /// renders per platform through its [`Display`](std::fmt::Display).
    BoundTo(&'keymap KeystrokeSequence),
    /// The live keymap binds no keystroke to the command, so it runs from the
    /// command palette or a direct trigger rather than from the keyboard.
    Unbound,
}

/// The keystroke every bound command is run from in the live keymap.
///
/// The resource is replaced whenever a keymap generation is committed, so it
/// always reports the bindings currently routing input. A command bound in more
/// than one place reports its global binding; a command bound only inside
/// conditions reports the binding from the condition the registry registered
/// first, because `MergedKeymap::bindings`
/// sorts the condition handles before walking them. Two keymaps built from the
/// same sources therefore report the same keystroke, rather than whichever
/// condition the `conditions` map happened to hash ahead of the other.
#[derive(Default, Resource)]
pub struct KeymapBindings {
    keystrokes: HashMap<CommandId, KeystrokeSequence>,
}

impl KeymapBindings {
    /// Reports the keystroke `command_id` is run from.
    #[must_use]
    pub fn keystroke(&self, command_id: &CommandId) -> CommandKeystroke<'_> {
        self.keystrokes
            .get(command_id)
            .map_or(CommandKeystroke::Unbound, CommandKeystroke::BoundTo)
    }

    /// Collects one table from a keymap generation's resolved bindings.
    ///
    /// The first binding reaching a command wins, so a later condition-scoped
    /// binding never displaces the global keystroke a reader expects. The
    /// caller supplies the order, and the table is only as stable as that
    /// order is; the only producer is
    /// [`MergedKeymap::bindings`](super::MergedKeymap::bindings), which yields
    /// global bindings first and then each condition in sorted handle order,
    /// so the same sources always resolve to the same keystroke.
    pub(crate) fn from_bindings<'binding>(
        bindings: impl Iterator<Item = (&'binding KeystrokeSequence, &'binding CommandId)>,
    ) -> Self {
        let mut keystrokes = HashMap::new();
        for (keystroke_sequence, command_id) in bindings {
            keystrokes
                .entry(command_id.clone())
                .or_insert_with(|| keystroke_sequence.clone());
        }

        Self { keystrokes }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::str::FromStr;

    use super::CommandKeystroke;
    use super::KeymapBindings;
    use crate::CommandId;
    use crate::KeystrokeSequence;

    fn command_id(text: &str) -> CommandId {
        CommandId::from_str(text).expect("the test command id is well formed")
    }

    fn keystroke_sequence(text: &str) -> KeystrokeSequence {
        KeystrokeSequence::from_str(text).expect("the test keystroke sequence is well formed")
    }

    #[test]
    fn a_bound_command_reports_the_keystroke_it_runs_from() {
        let bound = keystroke_sequence("secondary-p");
        let bound_command = command_id("palette::open");
        let keymap_bindings = KeymapBindings::from_bindings([(&bound, &bound_command)].into_iter());

        assert_eq!(
            keymap_bindings.keystroke(&bound_command),
            CommandKeystroke::BoundTo(&bound)
        );
    }

    #[test]
    fn a_declared_command_with_no_binding_reports_unbound() {
        let bound = keystroke_sequence("secondary-p");
        let bound_command = command_id("palette::open");
        let keymap_bindings = KeymapBindings::from_bindings([(&bound, &bound_command)].into_iter());

        assert_eq!(
            keymap_bindings.keystroke(&command_id("palette::never_bound")),
            CommandKeystroke::Unbound
        );
    }

    #[test]
    fn the_first_binding_reaching_a_command_is_the_one_reported() {
        let global = keystroke_sequence("secondary-p");
        let conditional = keystroke_sequence("secondary-shift-p");
        let bound_command = command_id("palette::open");
        let keymap_bindings = KeymapBindings::from_bindings(
            [(&global, &bound_command), (&conditional, &bound_command)].into_iter(),
        );

        assert_eq!(
            keymap_bindings.keystroke(&bound_command),
            CommandKeystroke::BoundTo(&global)
        );
    }
}
