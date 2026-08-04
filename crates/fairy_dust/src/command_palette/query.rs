//! Search over the command registry: query normalization, the rows the palette
//! lists, and what a query resolved to.

use hana_rubric::CommandId;
use hana_rubric::CommandInfo;
use hana_rubric::CommandKeystroke;
use hana_rubric::CommandRegistry;
use hana_rubric::KeymapBindings;

/// What a palette query resolved to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PaletteSelection {
    /// The query selected this command, which Enter dispatches.
    Selected(CommandId),
    /// The query dispatches nothing, for this reason.
    Rejected(PaletteRejection),
}

/// Why a query dispatches no command.
///
/// The three states are distinct because the palette renders each differently:
/// an empty query lists everything without explaining itself, an unmatched query
/// says so, and a query that only matched a hold-to-act command explains why
/// pressing Enter does nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaletteRejection {
    /// The query is blank, so the palette lists every invocable command and
    /// selects none of them.
    EmptyQuery,
    /// No declared command matched the query text.
    NoMatch,
    /// The query matched only commands whose
    /// [`Capability`](hana_rubric::Capability) keeps them out of the palette.
    NotPaletteInvocable,
}

/// Whether a rejection is worth a line above the command rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RejectionStatusLine {
    /// Render this sentence above the command rows.
    Line(&'static str),
    /// Render nothing: the listed commands are already the whole answer.
    NoLine,
}

impl PaletteRejection {
    /// The sentence the IME rejection carries when Enter cannot dispatch.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::EmptyQuery => "type a command title or id",
            Self::NoMatch => "no command matches this text",
            Self::NotPaletteInvocable => "that command runs from its binding, not here",
        }
    }

    /// The line rendered above the command rows.
    pub(crate) const fn status_line(self) -> RejectionStatusLine {
        match self {
            Self::EmptyQuery => RejectionStatusLine::NoLine,
            Self::NoMatch => RejectionStatusLine::Line("No command matches this text."),
            Self::NotPaletteInvocable => RejectionStatusLine::Line(
                "That command is hold-to-act, so it runs from its binding rather than here.",
            ),
        }
    }
}

/// One rendered command row: what the reader reads, with no command id and no
/// declaration detail in it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommandRow {
    /// The command this row dispatches.
    pub(crate) id:        CommandId,
    /// Authored title, which is the only command text the row shows.
    pub(crate) title:     String,
    /// The keystroke the row prints on its right.
    pub(crate) keystroke: RowKeystroke,
}

/// The keystroke text one command row prints, already rendered for this
/// platform.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RowKeystroke {
    /// The live keymap runs this command from this keystroke text.
    Bound(String),
    /// Nothing runs this command from the keyboard, so the column stays empty.
    Unbound,
}

/// Builds the rows for the commands matching `query`, each carrying the
/// keystroke the live keymap runs it from.
pub(crate) fn command_rows(
    command_registry: &CommandRegistry,
    keymap_bindings: &KeymapBindings,
    query: &str,
) -> Vec<CommandRow> {
    matching_commands(command_registry, query)
        .into_iter()
        .map(|command_info| CommandRow {
            id:        command_info.id.clone(),
            title:     command_info.title.to_owned(),
            keystroke: match keymap_bindings.keystroke(command_info.id) {
                CommandKeystroke::BoundTo(keystroke_sequence) => {
                    RowKeystroke::Bound(keystroke_sequence.to_string())
                },
                CommandKeystroke::Unbound => RowKeystroke::Unbound,
            },
        })
        .collect()
}

/// Lists the commands the palette offers, in registry order.
#[must_use]
pub(crate) fn palette_commands(command_registry: &CommandRegistry) -> Vec<CommandInfo<'_>> {
    command_registry
        .iter()
        .filter(|command_info| command_info.capability.is_palette_invocable())
        .collect()
}

/// Lists the palette-invocable commands whose title or id matches `query`.
///
/// A blank query matches every row, so the palette opens showing the whole
/// registry.
#[must_use]
pub(crate) fn matching_commands<'registry>(
    command_registry: &'registry CommandRegistry,
    query: &str,
) -> Vec<CommandInfo<'registry>> {
    let normalized_query = normalize(query);
    palette_commands(command_registry)
        .into_iter()
        .filter(|command_info| matches_query(*command_info, &normalized_query))
        .collect()
}

/// Resolves `query` against every declared command, including the ones the
/// palette does not list, so a query naming a hold-to-act command reports that
/// rather than reading as a typo.
#[must_use]
pub(crate) fn resolve_selection(
    command_registry: &CommandRegistry,
    query: &str,
) -> PaletteSelection {
    let normalized_query = normalize(query);
    if normalized_query.is_empty() {
        return PaletteSelection::Rejected(PaletteRejection::EmptyQuery);
    }

    let mut matched_any = false;
    for command_info in command_registry.iter() {
        if !matches_query(command_info, &normalized_query) {
            continue;
        }
        matched_any = true;
        if command_info.capability.is_palette_invocable() {
            return PaletteSelection::Selected(command_info.id.clone());
        }
    }

    if matched_any {
        PaletteSelection::Rejected(PaletteRejection::NotPaletteInvocable)
    } else {
        PaletteSelection::Rejected(PaletteRejection::NoMatch)
    }
}

fn matches_query(command_info: CommandInfo<'_>, normalized_query: &str) -> bool {
    normalize(command_info.title).contains(normalized_query)
        || normalize(command_info.id.as_str()).contains(normalized_query)
}

/// Folds case and reduces `::`, `_`, `-`, and runs of whitespace to one space,
/// so `camera::reset_home`, `camera reset home`, and `Reset Home` normalize to
/// text that matches the same rows.
fn normalize(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut pending_separator = false;
    for character in text.chars() {
        if character == ':' || character == '_' || character == '-' || character.is_whitespace() {
            pending_separator = !normalized.is_empty();
            continue;
        }
        if pending_separator {
            normalized.push(' ');
            pending_separator = false;
        }
        normalized.extend(character.to_lowercase());
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::PaletteRejection;
    use super::PaletteSelection;
    use super::matching_commands;
    use super::normalize;
    use super::palette_commands;
    use super::resolve_selection;
    use crate::command_palette::test_support::HELD_COMMAND_ID;
    use crate::command_palette::test_support::OPEN_COMMAND_ID;
    use crate::command_palette::test_support::UNREMAPPABLE_COMMAND_ID;
    use crate::command_palette::test_support::command_id;
    use crate::command_palette::test_support::palette_test_app;

    #[test]
    fn normalization_collapses_separators_and_case() {
        assert_eq!(normalize("camera::reset_home"), "camera reset home");
        assert_eq!(normalize("  Reset   Camera  "), "reset camera");
        assert_eq!(normalize("Open-Command-Palette"), "open command palette");
        assert_eq!(normalize("::"), "");
    }

    #[test]
    fn palette_lists_invocable_commands_and_omits_held_ones() {
        let app = palette_test_app();
        let command_registry = crate::command_palette::test_support::registry(&app);

        let listed = palette_commands(command_registry)
            .into_iter()
            .map(|command_info| command_info.id.as_str())
            .collect::<Vec<_>>();

        assert!(listed.contains(&OPEN_COMMAND_ID));
        assert!(listed.contains(&UNREMAPPABLE_COMMAND_ID));
        assert!(!listed.contains(&HELD_COMMAND_ID));
    }

    #[test]
    fn search_matches_authored_title_and_raw_id() {
        let app = palette_test_app();
        let command_registry = crate::command_palette::test_support::registry(&app);

        let matched_by_title = matching_commands(command_registry, "Open Command Palette")
            .into_iter()
            .any(|command_info| command_info.id.as_str() == OPEN_COMMAND_ID);
        let matched_by_id = matching_commands(command_registry, OPEN_COMMAND_ID)
            .into_iter()
            .any(|command_info| command_info.id.as_str() == OPEN_COMMAND_ID);

        assert!(matched_by_title);
        assert!(matched_by_id);
    }

    #[test]
    fn blank_query_lists_every_invocable_command() {
        let app = palette_test_app();
        let command_registry = crate::command_palette::test_support::registry(&app);

        assert_eq!(
            matching_commands(command_registry, "   ").len(),
            palette_commands(command_registry).len()
        );
    }

    #[test]
    fn selection_distinguishes_blank_unmatched_and_held_queries() {
        let app = palette_test_app();
        let command_registry = crate::command_palette::test_support::registry(&app);

        assert_eq!(
            resolve_selection(command_registry, "  "),
            PaletteSelection::Rejected(PaletteRejection::EmptyQuery)
        );
        assert_eq!(
            resolve_selection(command_registry, "no such command anywhere"),
            PaletteSelection::Rejected(PaletteRejection::NoMatch)
        );
        assert_eq!(
            resolve_selection(command_registry, HELD_COMMAND_ID),
            PaletteSelection::Rejected(PaletteRejection::NotPaletteInvocable)
        );
        assert_eq!(
            resolve_selection(command_registry, OPEN_COMMAND_ID),
            PaletteSelection::Selected(command_id(OPEN_COMMAND_ID))
        );
    }
}
