//! Compact top-left title bar for example-level controls.

use bevy::prelude::*;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Sizing;

use super::constants::CONTROL_ACTIVE_COLOR;
use super::constants::CONTROL_INACTIVE_COLOR;
use super::constants::DIVIDER_COLOR;
use super::constants::SEPARATOR_HEIGHT;
use super::constants::SEPARATOR_WIDTH;
use super::constants::TITLE_BAR_CHILD_GAP;
use super::constants::TITLE_BAR_DEFAULT_TITLE;
use super::screen_panel_frame;
use super::screen_panel_material;
use crate::camera_home::CameraHomeConfig;
use crate::camera_home::HomeTitleBarControl;
use crate::constants::HOME_CONTROL;
use crate::constants::LABEL_SIZE;
use crate::constants::TITLE_COLOR;
use crate::constants::TITLE_SIZE;

const HELP_CONTROL: &str = "?";

/// Stable identity plus visible label for a title-bar chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TitleChip {
    id:    &'static str,
    label: &'static str,
}

impl TitleChip {
    /// Creates a title chip with separate stable identity and visible label.
    #[must_use]
    pub const fn new(id: &'static str, label: &'static str) -> Self { Self { id, label } }

    /// Creates a title chip whose identity and visible label are the same.
    #[must_use]
    pub const fn label(label: &'static str) -> Self { Self { id: label, label } }

    /// Returns the stable chip identity.
    #[must_use]
    pub const fn id(self) -> &'static str { self.id }

    /// Returns the visible chip label.
    #[must_use]
    pub const fn label_text(self) -> &'static str { self.label }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Stored title-bar control with stable identity and visible label.
pub struct TitleBarControl {
    id:    String,
    label: String,
}

impl TitleBarControl {
    fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id:    id.into(),
            label: label.into(),
        }
    }

    fn from_label(label: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            id: label.clone(),
            label,
        }
    }
}

impl From<TitleChip> for TitleBarControl {
    fn from(value: TitleChip) -> Self { Self::new(value.id, value.label) }
}

impl From<TitleChip> for String {
    fn from(value: TitleChip) -> Self { value.id.to_string() }
}

impl From<&'static str> for TitleBarControl {
    fn from(value: &'static str) -> Self { Self::from_label(value) }
}

impl From<String> for TitleBarControl {
    fn from(value: String) -> Self { Self::from_label(value) }
}

/// Resource-level sugar for one-resource / one-chip title-bar activation.
pub trait TitleChipActivation {
    /// Returns whether the chip should be highlighted.
    fn activation(&self) -> ControlActivation;
}

#[derive(Resource, Default)]
pub(crate) struct TitleBarControlRegistry {
    controls: Vec<TitleBarControl>,
}

impl TitleBarControlRegistry {
    pub(crate) fn push(&mut self, control: impl Into<TitleBarControl>) {
        let control = control.into();
        if self
            .controls
            .iter()
            .any(|existing| existing.id == control.id)
        {
            return;
        }
        self.controls.push(control);
    }
}

/// Whether title-bar chips lay out in a single row or stack in a column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TitleBarOrientation {
    /// Chips share one horizontal row separated by vertical dividers.
    #[default]
    Horizontal,
    /// Each chip occupies its own row, separated by horizontal dividers.
    Vertical,
}

/// A compact top-left title bar for example-level controls.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TitleBar {
    anchor:           Anchor,
    orientation:      TitleBarOrientation,
    title:            String,
    controls:         Vec<TitleBarControl>,
    active_controls:  Vec<String>,
    background_color: Option<Color>,
}

impl Default for TitleBar {
    fn default() -> Self { Self::new() }
}

impl TitleBar {
    /// Creates a title bar with the visible example title.
    #[must_use]
    pub fn new() -> Self {
        Self {
            anchor:           Anchor::TopLeft,
            orientation:      TitleBarOrientation::Horizontal,
            title:            TITLE_BAR_DEFAULT_TITLE.to_string(),
            controls:         Vec::new(),
            active_controls:  Vec::new(),
            background_color: None,
        }
    }

    /// Overrides the rendered title. Strings render literally — pass the
    /// case you want displayed.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the title bar screen anchor.
    #[must_use]
    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Sets whether chips lay out in a row (default) or stack in a column.
    #[must_use]
    pub const fn with_orientation(mut self, orientation: TitleBarOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Overrides the inner background color (including alpha) for this
    /// title bar. Defaults to the crate's `INNER_BACKGROUND` constant.
    #[must_use]
    pub const fn with_background_color(mut self, color: Color) -> Self {
        self.background_color = Some(color);
        self
    }

    /// Adds a compact control label such as `H Home`.
    #[must_use]
    pub fn control(mut self, control: impl Into<TitleBarControl>) -> Self {
        self.controls.push(control.into());
        self
    }

    /// Adds a compact control label that starts highlighted.
    #[must_use]
    pub fn active_control(mut self, control: impl Into<TitleBarControl>) -> Self {
        let control = control.into();
        self.active_controls.push(control.id.clone());
        self.controls.push(control);
        self
    }

    /// Adds multiple compact control labels.
    #[must_use]
    pub fn controls(
        mut self,
        controls: impl IntoIterator<Item = impl Into<TitleBarControl>>,
    ) -> Self {
        self.controls.extend(controls.into_iter().map(Into::into));
        self
    }
}

/// Whether a control label should be highlighted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlActivation {
    /// Highlight the control.
    Active,
    /// Clear the control's highlight.
    Inactive,
}

/// Mutable highlight state for a spawned [`TitleBar`].
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TitleBarControlState {
    active_controls: Vec<String>,
}

impl TitleBarControlState {
    fn from_title_bar(title_bar: &TitleBar) -> Self {
        Self {
            active_controls: title_bar.active_controls.clone(),
        }
    }

    /// Sets whether a control label is highlighted. Repeated calls in the same
    /// state are no-ops.
    pub fn set_active(&mut self, control: &str, activation: ControlActivation) {
        let position = self
            .active_controls
            .iter()
            .position(|active_control| active_control == control);
        match (position, activation) {
            (None, ControlActivation::Active) => self.active_controls.push(control.to_string()),
            (Some(index), ControlActivation::Inactive) => {
                self.active_controls.remove(index);
            },
            (None, ControlActivation::Inactive) | (Some(_), ControlActivation::Active) => {},
        }
    }

    /// Returns whether a control label is highlighted.
    #[must_use]
    pub fn is_active(&self, control: &str) -> bool {
        self.active_controls
            .iter()
            .any(|active_control| active_control == control)
    }
}

#[derive(Component)]
struct TitleBarMarker;

pub(super) fn spawn_title_bar_with_home_chip(
    commands: &mut Commands,
    title_bar: &TitleBar,
    home: Option<&CameraHomeConfig>,
    registry: Option<&TitleBarControlRegistry>,
) {
    let mut title_bar = title_bar.clone();
    if home.is_some_and(|home| matches!(home.title_bar_control, HomeTitleBarControl::Shown))
        && !title_bar
            .controls
            .iter()
            .any(|control| control.id == HOME_CONTROL)
    {
        title_bar
            .controls
            .insert(0, TitleBarControl::from(HOME_CONTROL));
    }
    if let Some(registry) = registry {
        for control in &registry.controls {
            if !title_bar
                .controls
                .iter()
                .any(|existing| existing.id == control.id)
            {
                title_bar.controls.push(control.clone());
            }
        }
    }
    spawn_title_bar(commands, &title_bar);
}

fn spawn_title_bar(commands: &mut Commands, title_bar: &TitleBar) {
    let state = TitleBarControlState::from_title_bar(title_bar);
    let unlit = screen_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(title_bar.anchor)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_title_bar_tree(title_bar, &state))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((
                TitleBarMarker,
                title_bar.clone(),
                state,
                panel,
                Transform::default(),
            ));
        },
        Err(error) => {
            error!("fairy_dust: failed to build title bar: {error}");
        },
    }
}

pub(super) fn refresh_changed_title_bar(
    mut commands: Commands,
    title_bars: Query<
        (Entity, &TitleBar, &TitleBarControlState),
        Or<(Changed<TitleBar>, Changed<TitleBarControlState>)>,
    >,
) {
    for (entity, title_bar, state) in &title_bars {
        commands.set_tree(entity, build_title_bar_tree(title_bar, state));
    }
}

fn build_title_bar_tree(title_bar: &TitleBar, state: &TitleBarControlState) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_title_bar_layout(&mut builder, title_bar, state);
    builder.build()
}

fn build_title_bar_layout(
    builder: &mut LayoutBuilder,
    title_bar: &TitleBar,
    state: &TitleBarControlState,
) {
    let title = LayoutTextStyle::new(TITLE_SIZE).with_color(TITLE_COLOR);
    let inactive_control = LayoutTextStyle::new(LABEL_SIZE).with_color(CONTROL_INACTIVE_COLOR);
    let active_control = LayoutTextStyle::new(LABEL_SIZE).with_color(CONTROL_ACTIVE_COLOR);

    let background = title_bar
        .background_color
        .unwrap_or_else(super::default_inner_background);
    let orientation = title_bar.orientation;
    let row = match orientation {
        TitleBarOrientation::Horizontal => El::new()
            .width(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_gap(TITLE_BAR_CHILD_GAP)
            .child_align_y(AlignY::Center),
        TitleBarOrientation::Vertical => El::new()
            .width(Sizing::GROW)
            .direction(Direction::TopToBottom)
            .child_gap(TITLE_BAR_CHILD_GAP),
    };
    screen_panel_frame(builder, Sizing::FIT, Sizing::FIT, background, |builder| {
        builder.with(row, |builder| {
            builder.text(&title_bar.title, title);
            for control in &title_bar.controls {
                title_separator(builder, orientation);
                let style = if state.is_active(&control.id) {
                    active_control.clone()
                } else {
                    inactive_control.clone()
                };
                builder.text(&control.label, style);
            }
            title_separator(builder, orientation);
            builder.text(HELP_CONTROL, inactive_control);
        });
    });
}

fn title_separator(builder: &mut LayoutBuilder, orientation: TitleBarOrientation) {
    let (width, height) = match orientation {
        TitleBarOrientation::Horizontal => (
            Sizing::fixed(SEPARATOR_WIDTH),
            Sizing::fixed(SEPARATOR_HEIGHT),
        ),
        TitleBarOrientation::Vertical => (Sizing::GROW, Sizing::fixed(SEPARATOR_WIDTH)),
    };
    builder.with(
        El::new()
            .width(width)
            .height(height)
            .background(DIVIDER_COLOR),
        |_| {},
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_bar_control_state_tracks_active_labels() {
        let mut state = TitleBarControlState::default();

        state.set_active("H Home", ControlActivation::Active);
        assert!(state.is_active("H Home"));

        // Repeat Active must be a no-op — otherwise the duplicate would survive
        // the single Inactive below and `is_active` would still return true.
        state.set_active("H Home", ControlActivation::Active);
        state.set_active("H Home", ControlActivation::Inactive);
        assert!(!state.is_active("H Home"));

        // Repeat Inactive on an empty list must be a no-op.
        state.set_active("H Home", ControlActivation::Inactive);
        assert!(!state.is_active("H Home"));
    }

    #[test]
    fn title_bar_can_seed_active_controls() {
        let title_bar = TitleBar::new()
            .with_title("Demo")
            .control("A Action")
            .active_control("T Toggle");
        let state = TitleBarControlState::from_title_bar(&title_bar);

        assert!(!state.is_active("A Action"));
        assert!(state.is_active("T Toggle"));
    }
}
