//! Which side of the keyboard owns the keystrokes routing sees.

use bevy::prelude::Resource;

use crate::CommandId;

/// Whether the keys the reader presses are commands or the text of a field.
///
/// An application that opens a text field inserts [`Self::text_entry`] so the
/// characters typed into the field are not also matched against the compiled
/// keymap. Routing keeps running while the field owns the keyboard — it still
/// consumes every release edge — but dispatches nothing except the commands the
/// text-entry state exempts, which is how the command that closes the field
/// keeps working from its own keystroke.
///
/// Every keyboard handover releases the held physical sources and re-inhibits
/// the keys still down, the same reset a keymap reload performs, so a key held
/// across the transition cannot stay logically pressed after it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Resource)]
pub enum KeystrokeRouting {
    /// Every compiled binding routes.
    #[default]
    EveryBinding,
    /// A text field owns the keyboard, and only `exempt_commands` still reach
    /// their observers.
    TextEntry {
        /// The commands a keystroke still runs while the field owns the
        /// keyboard, normally the one that closes the field.
        exempt_commands: Vec<CommandId>,
    },
}

impl KeystrokeRouting {
    /// Suspends command routing for a text field, leaving `exempt_commands`
    /// runnable from the keyboard.
    ///
    /// Build the ids with [`CommandId::declared`], which takes them from the
    /// command declaration rather than from re-parsed text.
    #[must_use]
    pub fn text_entry(exempt_commands: impl IntoIterator<Item = CommandId>) -> Self {
        Self::TextEntry {
            exempt_commands: exempt_commands.into_iter().collect(),
        }
    }

    /// Whether a keystroke matching `command_id` still runs it.
    pub(crate) fn routes(&self, command_id: &CommandId) -> bool {
        match self {
            Self::EveryBinding => true,
            Self::TextEntry { exempt_commands } => exempt_commands.contains(command_id),
        }
    }
}

#[cfg(test)]
#[allow(
    dead_code,
    reason = "the command declarations generate action marker types the registry reaches by \
              reflection"
)]
mod tests {
    use bevy::prelude::Event;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy_enhanced_input::prelude::InputAction;

    use super::KeystrokeRouting;
    use crate::CommandId;
    use crate::ReflectKeymapCommand;

    crate::command! {
        action:      RoutingPaletteOpenAction,
        event:       PaletteOpen,
        id:          "routing::palette_open",
        title:       "Routing Palette Open",
        description: "Stays runnable while a text field owns the keyboard.",
    }

    crate::command! {
        action:      RoutingPaletteRunAction,
        event:       PaletteRun,
        id:          "routing::palette_run",
        title:       "Routing Palette Run",
        description: "Stands down while a text field owns the keyboard.",
    }

    #[test]
    fn every_binding_routes_each_declared_command() {
        let keystroke_routing = KeystrokeRouting::EveryBinding;

        assert!(keystroke_routing.routes(&CommandId::declared::<PaletteOpen>()));
        assert!(keystroke_routing.routes(&CommandId::declared::<PaletteRun>()));
    }

    #[test]
    fn text_entry_routes_only_the_commands_it_exempts() {
        let keystroke_routing =
            KeystrokeRouting::text_entry([CommandId::declared::<PaletteOpen>()]);

        assert!(keystroke_routing.routes(&CommandId::declared::<PaletteOpen>()));
        assert!(!keystroke_routing.routes(&CommandId::declared::<PaletteRun>()));
    }
}
