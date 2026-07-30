use std::ops::Range;

use bevy::prelude::Reflect;
use bevy::prelude::ReflectResource;
use bevy::prelude::Resource;

/// Categorizes the keymap failure described by a [`Diagnostic`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Reflect)]
pub enum DiagnosticKind {
    /// The JSONC document does not follow the keymap syntax.
    Syntax,
    /// A binding contains invalid keystroke text.
    Keystroke,
    /// A binding refers to an unavailable command.
    Command,
    /// A binding refers to an unavailable input context.
    Context,
    /// The keymap file could not be read or watched.
    Disk,
    /// A companion file could not be published.
    Companion,
    /// Multiple command events declare the same command ID.
    DuplicateCommandId,
    /// A command event has no palette title text.
    MissingCommandTitle,
    /// A command event has no palette description text.
    MissingCommandDescription,
    /// A command event declares malformed command ID text.
    InvalidCommandId,
    /// A command event has no reflected event trigger handle.
    CommandEventNotReflected,
    /// A hold-to-act command was bound to a multi-keystroke sequence.
    HeldCommandInSequence,
    /// A permanently registered recovery command appeared in a keymap file.
    UnremappableCommand,
    /// A keymap sequence begins with a protected recovery keystroke.
    ReservedKeystroke,
}

/// Describes whether a keymap diagnostic prevents its binding from loading.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Reflect)]
pub enum DiagnosticSeverity {
    /// The affected binding or document cannot be loaded.
    Failure,
    /// The document loaded, but the authored source should be corrected.
    Advisory,
}

/// One keymap problem retained for application diagnostics and BRP inspection.
#[derive(Clone, Debug, Eq, PartialEq, Reflect)]
pub struct Diagnostic {
    /// Source file that produced this diagnostic.
    pub source_path:        String,
    /// Byte range within the contents of [`Self::source_path`] that identifies
    /// the problem.
    pub byte_range:         Range<usize>,
    /// One-based source line containing [`Self::byte_range`].
    pub line:               usize,
    /// One-based source column containing [`Self::byte_range`].
    pub column:             usize,
    /// Zero-based keymap block that produced this diagnostic.
    pub block_index:        usize,
    /// Input context text associated with the binding.
    pub context:            String,
    /// Original keystroke text from the binding.
    pub original_keystroke: String,
    /// Command ID text from the binding.
    pub command_id:         String,
    /// Diagnostic category reported for this source location.
    pub kind:               DiagnosticKind,
    /// Whether this diagnostic prevents the affected binding from loading.
    pub severity:           DiagnosticSeverity,
    /// Human-readable sentence describing this diagnostic.
    ///
    /// Unlike [`Self::suggestions`], this is not machine-applicable replacement text. This
    /// required field uses an empty string when no message applies.
    pub message:            String,
    /// Text replacements that can repair the keymap source.
    pub suggestions:        Vec<String>,
}

/// Problems reported by the most recent keymap load.
///
/// A later reload transaction retains advisory diagnostics after a successful
/// load so keymap editors can continue to present source corrections.
#[derive(Clone, Debug, Default, Reflect, Resource)]
#[reflect(Resource)]
pub struct KeymapLoadFailures {
    /// All diagnostics retained from the most recent keymap load.
    pub diagnostics: Vec<Diagnostic>,
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::Diagnostic;
    use super::DiagnosticKind;
    use super::DiagnosticSeverity;
    use super::KeymapLoadFailures;

    #[test]
    fn diagnostic_fields_use_only_stable_data_types() {
        let diagnostic = Diagnostic {
            source_path:        String::from("keymap.jsonc"),
            byte_range:         0..1,
            line:               1,
            column:             1,
            block_index:        0,
            context:            String::from("editor"),
            original_keystroke: String::from("cmd-k"),
            command_id:         String::from("camera::home"),
            kind:               DiagnosticKind::Command,
            severity:           DiagnosticSeverity::Failure,
            message:            String::from("The command is unavailable."),
            suggestions:        vec![String::from("camera::home")],
        };
        let keymap_load_failures = KeymapLoadFailures {
            diagnostics: vec![diagnostic],
        };

        let diagnostic = &keymap_load_failures.diagnostics[0];
        let _: &Range<usize> = &diagnostic.byte_range;
        let _: &String = &diagnostic.source_path;
        let _: &String = &diagnostic.context;
        let _: &String = &diagnostic.original_keystroke;
        let _: &String = &diagnostic.command_id;
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Failure);
        let _: &String = &diagnostic.message;
        assert_eq!(diagnostic.message, "The command is unavailable.");
        let _: &Vec<String> = &diagnostic.suggestions;
        let _: &Vec<Diagnostic> = &keymap_load_failures.diagnostics;
    }

    #[test]
    fn diagnostic_kind_includes_command_registry_failures() {
        let diagnostic_kinds = [
            DiagnosticKind::DuplicateCommandId,
            DiagnosticKind::MissingCommandTitle,
            DiagnosticKind::MissingCommandDescription,
            DiagnosticKind::InvalidCommandId,
            DiagnosticKind::CommandEventNotReflected,
        ];

        assert_eq!(diagnostic_kinds.len(), 5);
    }
}
