//! The palette box: a screen-space panel holding the keymap failure rows, the
//! query field, and the matching command rows.

use bevy::prelude::Vec2;
use bevy::prelude::Window;
use hana_diegetic::AlignX;
use hana_diegetic::AlignY;
use hana_diegetic::Border;
use hana_diegetic::CornerRadius;
use hana_diegetic::EditorStateColors;
use hana_diegetic::El;
use hana_diegetic::ImeAppOwnedFieldSpec;
use hana_diegetic::ImeEditableFieldSpec;
use hana_diegetic::LayoutBuilder;
use hana_diegetic::LayoutTree;
use hana_diegetic::Padding;
use hana_diegetic::Sizing;
use hana_diegetic::Text;
use hana_diegetic::TextStyle;
use hana_diegetic::TextWrap;
use hana_rubric::DiagnosticSeverity;

use super::constants::COMMAND_KEYSTROKE_COLOR;
use super::constants::COMMAND_KEYSTROKE_COLUMN_WIDTH;
use super::constants::COMMAND_KEYSTROKE_MAX_CHARS;
use super::constants::COMMAND_ROW_HEIGHT;
use super::constants::COMMAND_TEXT_SIZE;
use super::constants::COMMAND_TITLE_COLOR;
use super::constants::COMMAND_TITLE_MAX_CHARS;
use super::constants::FAILURE_ACTION_COLUMN_WIDTH;
use super::constants::FAILURE_ACTION_ID_PREFIX;
use super::constants::FAILURE_ADVISORY_COLOR;
use super::constants::FAILURE_COLOR;
use super::constants::FAILURE_LINE_MAX_CHARS;
use super::constants::FAILURE_ROW_HEIGHT;
use super::constants::FAILURE_TEXT_SIZE;
use super::constants::FIELD_BACKGROUND;
use super::constants::FIELD_BORDER;
use super::constants::FIELD_BORDER_WIDTH;
use super::constants::FIELD_CORNER_RADIUS;
use super::constants::FIELD_HEIGHT;
use super::constants::FIELD_ID;
use super::constants::FIELD_PADDING_X;
use super::constants::FIELD_SELECTION_COLOR;
use super::constants::FIELD_TEXT_COLOR;
use super::constants::FIELD_TEXT_SIZE;
use super::constants::HIDDEN_ROW_COLOR;
use super::constants::MAX_VISIBLE_COMMAND_ROWS;
use super::constants::PANEL_BACKGROUND;
use super::constants::PANEL_BORDER;
use super::constants::PANEL_BORDER_WIDTH;
use super::constants::PANEL_COLUMN_GAP;
use super::constants::PANEL_CORNER_RADIUS;
use super::constants::PANEL_EDGE_MARGIN;
use super::constants::PANEL_MAX_WIDTH;
use super::constants::PANEL_MIN_WIDTH;
use super::constants::PANEL_PADDING;
use super::constants::PANEL_ROW_GAP;
use super::constants::PANEL_TOP_RATIO;
use super::constants::PLACEHOLDER_COLOR;
use super::constants::PLACEHOLDER_TEXT;
use super::constants::SELECTED_COMMAND_BACKGROUND;
use super::constants::SELECTED_COMMAND_TITLE_COLOR;
use super::constants::SEPARATOR_COLOR;
use super::constants::SEPARATOR_HEIGHT;
use super::failure_row::KeymapFailureAction;
use super::failure_row::KeymapFailureActionLabel;
use super::failure_row::KeymapFailureRow;
use super::query::CommandRow;
use super::query::PaletteSelection;
use super::query::RejectionStatusLine;
use super::query::RowKeystroke;

/// Everything one rebuild of the palette tree renders.
pub(super) struct PaletteView<'view> {
    /// Query text as the IME session currently holds it.
    pub(super) query:           &'view str,
    /// What that query resolved to, which decides the highlighted row and the
    /// status line under the field.
    pub(super) selection:       &'view PaletteSelection,
    /// The command rows matching the query, in registry order.
    pub(super) commands:        &'view [CommandRow],
    /// Keymap failures rendered above the field.
    pub(super) keymap_failures: &'view [KeymapFailureRow],
    /// Width the box occupies, which the window decides.
    pub(super) panel_width:     f32,
}

/// Width the palette box occupies in this window.
///
/// The box holds [`PANEL_MAX_WIDTH`] while the window is wide enough for it and
/// its edge margins, and shrinks with the window below that rather than
/// overhanging the left edge.
pub(super) fn palette_panel_width(window: &Window) -> f32 {
    let available = PANEL_EDGE_MARGIN.mul_add(-2.0, window.width());
    available.clamp(PANEL_MIN_WIDTH, PANEL_MAX_WIDTH)
}

/// Top-left corner the palette box is anchored at, in window pixels.
///
/// The horizontal origin is clamped so a window narrower than
/// [`PANEL_MIN_WIDTH`] still shows the box's left edge.
pub(super) fn palette_panel_origin(window: &Window) -> Vec2 {
    let centered = (window.width() - palette_panel_width(window)) * 0.5;
    Vec2::new(centered.max(0.0), window.height() * PANEL_TOP_RATIO)
}

/// The panel-local id the failure row at `row_index` gives its action button.
pub(super) fn failure_action_id(row_index: usize) -> String {
    format!("{FAILURE_ACTION_ID_PREFIX}{row_index}")
}

/// Reads back the failure-row index an action button's panel-local id carries.
pub(super) fn failure_action_row_index(element_id: &str) -> FailureActionRow {
    element_id
        .strip_prefix(FAILURE_ACTION_ID_PREFIX)
        .and_then(|index| index.parse().ok())
        .map_or(FailureActionRow::NotAFailureAction, |row_index| {
            FailureActionRow::Row(row_index)
        })
}

/// Whether a clicked panel element is one of the palette's failure-row actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FailureActionRow {
    /// The click landed on the action of the failure row at this index.
    Row(usize),
    /// The click landed on some other element, which the palette leaves alone.
    NotAFailureAction,
}

/// Builds the palette's layout tree.
pub(super) fn palette_tree(view: &PaletteView<'_>) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(view.panel_width))
            .height(Sizing::FIT),
    );
    build_palette(&mut builder, view);
    builder.build()
}

fn build_palette(builder: &mut LayoutBuilder, view: &PaletteView<'_>) {
    builder.with(
        El::column()
            .width(Sizing::fixed(view.panel_width))
            .height(Sizing::FIT)
            .padding(Padding::all(PANEL_PADDING))
            .gap(PANEL_ROW_GAP)
            .background(PANEL_BACKGROUND)
            .corner_radius(CornerRadius::all(PANEL_CORNER_RADIUS))
            .border(Border::all(PANEL_BORDER_WIDTH, PANEL_BORDER)),
        |builder| {
            for (row_index, keymap_failure) in view.keymap_failures.iter().enumerate() {
                build_failure_row(builder, row_index, keymap_failure);
            }
            build_query_field(builder, view);
            build_separator(builder);
            build_command_rows(builder, view);
        },
    );
}

fn build_failure_row(
    builder: &mut LayoutBuilder,
    row_index: usize,
    keymap_failure: &KeymapFailureRow,
) {
    let color = match keymap_failure.severity {
        DiagnosticSeverity::Failure => FAILURE_COLOR,
        DiagnosticSeverity::Advisory => FAILURE_ADVISORY_COLOR,
    };
    let text = TextStyle::new(FAILURE_TEXT_SIZE).with_color(color);
    let line = clip(
        &format!("{} — {}", keymap_failure.location, keymap_failure.message),
        FAILURE_LINE_MAX_CHARS,
    );
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::fixed(FAILURE_ROW_HEIGHT))
            .gap(PANEL_COLUMN_GAP)
            .align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new().width(Sizing::GROW).height(Sizing::FIT),
                |builder| {
                    builder.text(Text::new(line, text.clone()).wrap(TextWrap::None));
                },
            );
            build_failure_action(builder, row_index, &keymap_failure.action, &text);
        },
    );
}

/// Renders the row's repair action as a button, so the word the row prints is
/// the thing the reader clicks.
fn build_failure_action(
    builder: &mut LayoutBuilder,
    row_index: usize,
    action: &KeymapFailureAction,
    text: &TextStyle,
) {
    let column = El::new()
        .width(Sizing::fixed(FAILURE_ACTION_COLUMN_WIDTH))
        .height(Sizing::FIT)
        .align_x(AlignX::Right);
    match action.label() {
        KeymapFailureActionLabel::NoAction => {
            builder.with(column, |_| {});
        },
        KeymapFailureActionLabel::Verb(verb) => {
            builder.with(column.button(failure_action_id(row_index)), |builder| {
                builder.text(Text::new(verb, text.clone()).wrap(TextWrap::None));
            });
        },
    }
}

/// Shortens `text` to `max_chars`, marking the cut so a clipped row never reads
/// as the whole message.
fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let kept = max_chars.saturating_sub(1);
    text.chars().take(kept).chain(['…']).collect()
}

/// Authors the query field as an editable panel field.
///
/// The IME session edits inside this element, so the palette draws one box and
/// `hana_diegetic` renders the live buffer into it.
fn build_query_field(builder: &mut LayoutBuilder, view: &PaletteView<'_>) {
    let (query_text, style) = if view.query.is_empty() {
        (
            PLACEHOLDER_TEXT,
            TextStyle::new(FIELD_TEXT_SIZE).with_color(PLACEHOLDER_COLOR),
        )
    } else {
        (
            view.query,
            TextStyle::new(FIELD_TEXT_SIZE).with_color(FIELD_TEXT_COLOR),
        )
    };
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::fixed(FIELD_HEIGHT))
            .padding(Padding::xy(FIELD_PADDING_X, 0.0))
            .align_y(AlignY::Center)
            .background(FIELD_BACKGROUND)
            .corner_radius(CornerRadius::all(FIELD_CORNER_RADIUS))
            .border(Border::all(FIELD_BORDER_WIDTH, FIELD_BORDER))
            .editable_field(
                FIELD_ID,
                ImeEditableFieldSpec::AppOwned(ImeAppOwnedFieldSpec::new(FIELD_ID)),
            )
            .editor_text(EditorStateColors::new().focused(FIELD_TEXT_COLOR))
            .editor_selection(EditorStateColors::new().focused(FIELD_SELECTION_COLOR))
            .editor_caret(EditorStateColors::new().focused(FIELD_TEXT_COLOR)),
        |builder| {
            builder.text(Text::new(query_text, style.clone()).wrap(TextWrap::None));
        },
    );
}

fn build_separator(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(SEPARATOR_HEIGHT))
            .background(SEPARATOR_COLOR),
        |_| {},
    );
}

fn build_command_rows(builder: &mut LayoutBuilder, view: &PaletteView<'_>) {
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(PANEL_ROW_GAP),
        |builder| {
            if let RejectionStatusLine::Line(status) = status_line(view.selection) {
                build_note_row(builder, status.to_owned(), FAILURE_ADVISORY_COLOR);
            }
            for command_row in view.commands.iter().take(MAX_VISIBLE_COMMAND_ROWS) {
                build_command_row(builder, command_row, view.selection);
            }
            let hidden = view.commands.len().saturating_sub(MAX_VISIBLE_COMMAND_ROWS);
            if hidden > 0 {
                build_note_row(
                    builder,
                    format!("{hidden} more — keep typing to narrow the list"),
                    HIDDEN_ROW_COLOR,
                );
            }
        },
    );
}

fn build_note_row(builder: &mut LayoutBuilder, note: String, color: bevy::prelude::Color) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(COMMAND_ROW_HEIGHT)),
        |builder| {
            builder.text(
                Text::new(note, TextStyle::new(COMMAND_TEXT_SIZE).with_color(color))
                    .wrap(TextWrap::None),
            );
        },
    );
}

/// One command row: its authored title on the left, the keystroke it runs from
/// right-aligned on the right. A command with no binding shows nothing there.
fn build_command_row(
    builder: &mut LayoutBuilder,
    command_row: &CommandRow,
    selection: &PaletteSelection,
) {
    let is_selected = matches!(selection, PaletteSelection::Selected(selected)
        if *selected == command_row.id);
    let mut row = El::row()
        .width(Sizing::GROW)
        .height(Sizing::fixed(COMMAND_ROW_HEIGHT))
        .gap(PANEL_COLUMN_GAP)
        .align_y(AlignY::Center);
    let title_color = if is_selected {
        row = row
            .background(SELECTED_COMMAND_BACKGROUND)
            .corner_radius(CornerRadius::all(FIELD_CORNER_RADIUS));
        SELECTED_COMMAND_TITLE_COLOR
    } else {
        COMMAND_TITLE_COLOR
    };

    builder.with(row, |builder| {
        builder.with(
            El::new().width(Sizing::GROW).height(Sizing::FIT),
            |builder| {
                builder.text(
                    Text::new(
                        clip(&command_row.title, COMMAND_TITLE_MAX_CHARS),
                        TextStyle::new(COMMAND_TEXT_SIZE).with_color(title_color),
                    )
                    .wrap(TextWrap::None),
                );
            },
        );
        builder.with(
            El::new()
                .width(Sizing::fixed(COMMAND_KEYSTROKE_COLUMN_WIDTH))
                .height(Sizing::FIT)
                .align_x(AlignX::Right),
            |builder| {
                if let RowKeystroke::Bound(keystroke) = &command_row.keystroke {
                    builder.text(
                        Text::new(
                            clip(keystroke, COMMAND_KEYSTROKE_MAX_CHARS),
                            TextStyle::new(COMMAND_TEXT_SIZE).with_color(COMMAND_KEYSTROKE_COLOR),
                        )
                        .wrap(TextWrap::None),
                    );
                }
            },
        );
    });
}

/// Returns the line rendered above the command rows when the query resolved to
/// something the palette cannot dispatch.
const fn status_line(selection: &PaletteSelection) -> RejectionStatusLine {
    match selection {
        PaletteSelection::Selected(_) => RejectionStatusLine::NoLine,
        PaletteSelection::Rejected(rejection) => rejection.status_line(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bevy::prelude::Window;
    use hana_diegetic::LayoutTree;
    use hana_diegetic::LayoutTreeChange;
    use hana_rubric::DiagnosticSeverity;

    use super::FailureActionRow;
    use super::KeymapFailureAction;
    use super::KeymapFailureActionLabel;
    use super::KeymapFailureRow;
    use super::PANEL_EDGE_MARGIN;
    use super::PANEL_MAX_WIDTH;
    use super::PANEL_MIN_WIDTH;
    use super::PaletteView;
    use super::RejectionStatusLine;
    use super::clip;
    use super::failure_action_id;
    use super::failure_action_row_index;
    use super::palette_panel_origin;
    use super::palette_panel_width;
    use super::palette_tree;
    use super::status_line;
    use crate::command_palette::query::CommandRow;
    use crate::command_palette::query::PaletteRejection;
    use crate::command_palette::query::PaletteSelection;
    use crate::command_palette::query::RowKeystroke;
    use crate::command_palette::test_support::command_id;

    /// Pixel counts compare to a tenth of a pixel, which is finer than anything
    /// the layout can render and coarser than f32 rounding.
    const PIXEL_TOLERANCE: f32 = 0.1;
    /// Window dimensions for the axis a geometry test is not varying.
    const WINDOW_HEIGHT: f32 = 1080.0;
    const WINDOW_WIDTH: f32 = 1920.0;

    /// A window of exactly this logical size, which is all
    /// [`palette_panel_width`] and [`palette_panel_origin`] read.
    fn window(width: f32, height: f32) -> Window {
        let mut window = Window::default();
        window.resolution.set(width, height);
        window
    }

    fn panel_width(window_width: f32) -> f32 {
        palette_panel_width(&window(window_width, WINDOW_HEIGHT))
    }

    fn panel_origin_x(window_width: f32) -> f32 {
        palette_panel_origin(&window(window_width, WINDOW_HEIGHT)).x
    }

    fn panel_origin_y(window_height: f32) -> f32 {
        palette_panel_origin(&window(WINDOW_WIDTH, window_height)).y
    }

    fn assert_pixels_eq(measured: f32, expected: f32) {
        assert!(
            (measured - expected).abs() < PIXEL_TOLERANCE,
            "expected {expected} pixels, measured {measured}"
        );
    }

    #[test]
    fn a_wide_window_holds_the_box_at_its_full_width_and_centers_it() {
        assert_pixels_eq(panel_width(1920.0), PANEL_MAX_WIDTH);
        assert_pixels_eq(panel_origin_x(1920.0), (1920.0 - PANEL_MAX_WIDTH) * 0.5);
    }

    #[test]
    fn a_narrow_window_shrinks_the_box_instead_of_pushing_it_off_the_left_edge() {
        let window_width = 700.0;

        assert_pixels_eq(
            panel_width(window_width),
            PANEL_EDGE_MARGIN.mul_add(-2.0, window_width),
        );
        assert_pixels_eq(panel_origin_x(window_width), PANEL_EDGE_MARGIN);
    }

    #[test]
    fn a_window_narrower_than_the_minimum_still_shows_the_left_edge() {
        assert_pixels_eq(panel_width(200.0), PANEL_MIN_WIDTH);
        assert_pixels_eq(panel_origin_x(200.0), 0.0);
    }

    #[test]
    fn the_box_sits_below_the_top_of_the_window_without_reaching_its_middle() {
        let window_height = 1080.0;

        let origin_y = panel_origin_y(window_height);

        assert!(origin_y > 0.0);
        assert!(origin_y < window_height * 0.5);
    }

    #[test]
    fn an_action_id_round_trips_to_its_row_index() {
        assert_eq!(
            failure_action_row_index(&failure_action_id(3)),
            FailureActionRow::Row(3)
        );
        assert_eq!(
            failure_action_row_index("query"),
            FailureActionRow::NotAFailureAction
        );
        assert_eq!(
            failure_action_row_index("keymap-failure-action-not-a-number"),
            FailureActionRow::NotAFailureAction
        );
    }

    #[test]
    fn only_an_actionable_failure_prints_a_clickable_verb() {
        assert_eq!(
            KeymapFailureAction::OpenFile(PathBuf::from("/tmp/keymap.jsonc")).label(),
            KeymapFailureActionLabel::Verb("Open")
        );
        assert_eq!(
            KeymapFailureAction::RevealDirectory(PathBuf::from("/tmp")).label(),
            KeymapFailureActionLabel::Verb("Reveal")
        );
        assert_eq!(
            KeymapFailureAction::NoAction.label(),
            KeymapFailureActionLabel::NoAction
        );
    }

    #[test]
    fn a_clipped_line_is_marked_as_cut() {
        assert_eq!(clip("abcdef", 6), "abcdef");
        assert_eq!(clip("abcdef", 4), "abc…");
    }

    #[test]
    fn only_a_rejection_the_reader_can_act_on_prints_a_status_line() {
        assert_eq!(
            status_line(&PaletteSelection::Selected(command_id("palette::open"))),
            RejectionStatusLine::NoLine
        );
        assert_eq!(
            status_line(&PaletteSelection::Rejected(PaletteRejection::EmptyQuery)),
            RejectionStatusLine::NoLine
        );
        assert!(matches!(
            status_line(&PaletteSelection::Rejected(PaletteRejection::NoMatch)),
            RejectionStatusLine::Line(_)
        ));
        assert!(matches!(
            status_line(&PaletteSelection::Rejected(
                PaletteRejection::NotPaletteInvocable
            )),
            RejectionStatusLine::Line(_)
        ));
    }

    fn keymap_failure(severity: DiagnosticSeverity) -> KeymapFailureRow {
        KeymapFailureRow {
            severity,
            location: String::from("embedded defaults"),
            message: String::from("Unrecognized keymap block member."),
            action: KeymapFailureAction::NoAction,
        }
    }

    fn command_row(keystroke: RowKeystroke) -> CommandRow {
        CommandRow {
            id: command_id("palette::open"),
            title: String::from("Open Command Palette"),
            keystroke,
        }
    }

    /// The tree the palette renders for `commands` and `keymap_failures`, with
    /// the selection and query a reader sees before typing anything.
    fn tree(commands: &[CommandRow], keymap_failures: &[KeymapFailureRow]) -> LayoutTree {
        palette_tree(&PaletteView {
            query: "",
            selection: &PaletteSelection::Rejected(PaletteRejection::EmptyQuery),
            commands,
            keymap_failures,
            panel_width: PANEL_MAX_WIDTH,
        })
    }

    #[test]
    fn a_failure_row_colors_itself_by_its_severity_without_moving_anything() {
        let advisory = tree(&[], &[keymap_failure(DiagnosticSeverity::Advisory)]);
        let failure = tree(&[], &[keymap_failure(DiagnosticSeverity::Failure)]);

        assert_eq!(
            advisory.classify_change(&failure),
            LayoutTreeChange::VisualOnly
        );
        assert_eq!(
            advisory.classify_change(&tree(&[], &[keymap_failure(DiagnosticSeverity::Advisory)])),
            LayoutTreeChange::Identical
        );
    }

    #[test]
    fn each_keymap_failure_adds_its_own_row_above_the_field() {
        let none = tree(&[], &[]);
        let one = tree(&[], &[keymap_failure(DiagnosticSeverity::Advisory)]);
        let two = tree(
            &[],
            &[
                keymap_failure(DiagnosticSeverity::Advisory),
                keymap_failure(DiagnosticSeverity::Advisory),
            ],
        );

        assert!(one.len() > none.len());
        assert_eq!(two.len() - one.len(), one.len() - none.len());
    }

    #[test]
    fn an_unbound_command_row_renders_no_keystroke() {
        let bound = tree(
            &[command_row(RowKeystroke::Bound(String::from("⌘⇧P")))],
            &[],
        );
        let unbound = tree(&[command_row(RowKeystroke::Unbound)], &[]);

        assert_eq!(bound.len() - unbound.len(), 1);
    }

    #[test]
    fn the_selected_command_row_is_highlighted_where_the_others_are_not() {
        let rows = [command_row(RowKeystroke::Unbound)];
        let unselected = tree(&rows, &[]);
        let selected = palette_tree(&PaletteView {
            query:           "",
            selection:       &PaletteSelection::Selected(command_id("palette::open")),
            commands:        &rows,
            keymap_failures: &[],
            panel_width:     PANEL_MAX_WIDTH,
        });

        assert_eq!(
            unselected.classify_change(&selected),
            LayoutTreeChange::VisualOnly
        );
    }
}
