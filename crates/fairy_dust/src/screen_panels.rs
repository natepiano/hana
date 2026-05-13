//! Capability: small screen-space panels for examples.

use bevy::prelude::*;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::default_panel_material;

use crate::ensure_plugin;

/// A static side panel that explains what an example demonstrates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptionPanel {
    anchor: Anchor,
    title:  String,
    lines:  Vec<String>,
}

impl DescriptionPanel {
    /// Creates a description panel with a title.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            anchor: Anchor::BottomLeft,
            title:  title.into(),
            lines:  Vec::new(),
        }
    }

    /// Sets the panel screen anchor.
    #[must_use]
    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Adds one display line to the panel.
    #[must_use]
    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }

    /// Adds multiple display lines to the panel.
    #[must_use]
    pub fn lines(mut self, lines: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.lines.extend(lines.into_iter().map(Into::into));
        self
    }
}

/// A compact top-left title bar for example-level controls.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct TitleBar {
    anchor:          Anchor,
    title:           String,
    controls:        Vec<String>,
    active_controls: Vec<String>,
}

impl TitleBar {
    /// Creates a title bar with the visible example title.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            anchor:          Anchor::TopLeft,
            title:           title.into(),
            controls:        Vec::new(),
            active_controls: Vec::new(),
        }
    }

    /// Sets the title bar screen anchor.
    #[must_use]
    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Adds a compact control label such as `H Home`.
    #[must_use]
    pub fn control(mut self, control: impl Into<String>) -> Self {
        self.controls.push(control.into());
        self
    }

    /// Adds a compact control label that starts highlighted.
    #[must_use]
    pub fn active_control(mut self, control: impl Into<String>) -> Self {
        let control = control.into();
        self.controls.push(control.clone());
        self.active_controls.push(control);
        self
    }

    /// Adds multiple compact control labels.
    #[must_use]
    pub fn controls(mut self, controls: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.controls.extend(controls.into_iter().map(Into::into));
        self
    }
}

/// Mutable highlight state for a spawned [`TitleBar`].
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct TitleBarControlState {
    active_controls: Vec<String>,
}

impl TitleBarControlState {
    fn from_title_bar(title_bar: &TitleBar) -> Self {
        Self {
            active_controls: title_bar.active_controls.clone(),
        }
    }

    /// Sets whether a control label is highlighted.
    pub fn set_active(&mut self, control: &str, active: bool) -> bool {
        let Some(index) = self
            .active_controls
            .iter()
            .position(|active_control| active_control == control)
        else {
            if active {
                self.active_controls.push(control.to_string());
            }
            return active;
        };

        if active {
            return false;
        }

        self.active_controls.remove(index);
        true
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
struct DescriptionPanelMarker;

#[derive(Component)]
struct TitleBarMarker;

const RADIUS: Px = Px(12.0);
const FRAME_PAD: Px = Px(2.0);
const BORDER: Px = Px(2.0);
const INNER_PAD: Px = Px(10.0);
const INSET: Px = Px(FRAME_PAD.0 + BORDER.0);
const INNER_RADIUS: Px = Px(RADIUS.0 - INSET.0);

const TITLE_SIZE: Pt = Pt(14.0);
const BODY_SIZE: Pt = Pt(11.0);
const CONTROL_SIZE: Pt = Pt(12.0);

const DESCRIPTION_WIDTH: Px = Px(330.0);

const INNER_BG: Color = Color::srgba(0.02, 0.03, 0.07, 0.50);
const BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const BODY_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);
const CONTROL_ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
const CONTROL_INACTIVE_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);
const DIVIDER_COLOR: Color = Color::srgba(0.35, 0.8, 1.0, 0.35);

pub(crate) fn install_description(app: &mut App, panel: DescriptionPanel) {
    ensure_plugin(app, DiegeticUiPlugin);
    app.add_systems(Startup, move |mut commands: Commands| {
        spawn_description_panel(&mut commands, &panel);
    });
}

pub(crate) fn install_title_bar(app: &mut App, title_bar: TitleBar) {
    ensure_plugin(app, DiegeticUiPlugin);
    app.add_systems(PostUpdate, refresh_changed_title_bar);
    app.add_systems(Startup, move |mut commands: Commands| {
        spawn_title_bar(&mut commands, &title_bar);
    });
}

fn spawn_description_panel(commands: &mut Commands, panel: &DescriptionPanel) {
    let unlit = unlit_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(panel.anchor)
        .material(unlit.clone())
        .text_material(unlit)
        .layout(|builder| build_description_layout(builder, panel))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((DescriptionPanelMarker, panel, Transform::default()));
        },
        Err(error) => {
            error!("fairy_dust: failed to build description panel: {error}");
        },
    }
}

fn spawn_title_bar(commands: &mut Commands, title_bar: &TitleBar) {
    let state = TitleBarControlState::from_title_bar(title_bar);
    let unlit = unlit_panel_material();
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

fn refresh_changed_title_bar(
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

fn unlit_panel_material() -> StandardMaterial {
    StandardMaterial {
        unlit: true,
        ..default_panel_material()
    }
}

fn build_description_layout(builder: &mut LayoutBuilder, panel: &DescriptionPanel) {
    let title = LayoutTextStyle::new(TITLE_SIZE).with_color(TITLE_COLOR);
    let body = LayoutTextStyle::new(BODY_SIZE).with_color(BODY_COLOR);

    panel_frame(builder, Sizing::fixed(DESCRIPTION_WIDTH), |builder| {
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .direction(Direction::TopToBottom)
                .child_gap(Px(6.0)),
            |builder| {
                builder.text(panel.title.to_uppercase(), title);
                for line in &panel.lines {
                    builder.text(line, body.clone());
                }
            },
        );
    });
}

fn build_title_bar_tree(
    title_bar: &TitleBar,
    state: &TitleBarControlState,
) -> bevy_diegetic::LayoutTree {
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
    let inactive_control = LayoutTextStyle::new(CONTROL_SIZE).with_color(CONTROL_INACTIVE_COLOR);
    let active_control = LayoutTextStyle::new(CONTROL_SIZE).with_color(CONTROL_ACTIVE_COLOR);

    panel_frame(builder, Sizing::FIT, |builder| {
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .direction(Direction::LeftToRight)
                .child_gap(Px(10.0))
                .child_align_y(AlignY::Center),
            |builder| {
                builder.text(title_bar.title.to_uppercase(), title);
                for control_label in &title_bar.controls {
                    title_separator(builder);
                    let control = if state.is_active(control_label) {
                        active_control.clone()
                    } else {
                        inactive_control.clone()
                    };
                    builder.text(control_label, control);
                }
            },
        );
    });
}

fn panel_frame(
    builder: &mut LayoutBuilder,
    width: Sizing,
    content: impl FnOnce(&mut LayoutBuilder),
) {
    builder.with(
        El::new()
            .width(width)
            .height(Sizing::FIT)
            .padding(Padding::all(FRAME_PAD))
            .corner_radius(CornerRadius::all(RADIUS))
            .border(Border::all(BORDER, BORDER_ACCENT)),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .padding(Padding::all(INNER_PAD))
                    .corner_radius(CornerRadius::all(INNER_RADIUS))
                    .background(INNER_BG)
                    .border(Border::all(Px(1.0), BORDER_DIM)),
                content,
            );
        },
    );
}

fn title_separator(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::fixed(Px(1.0)))
            .height(Sizing::fixed(Px(18.0)))
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

        assert!(state.set_active("H Home", true));
        assert!(state.is_active("H Home"));
        assert!(!state.set_active("H Home", true));

        assert!(state.set_active("H Home", false));
        assert!(!state.is_active("H Home"));
        assert!(!state.set_active("H Home", false));
    }

    #[test]
    fn title_bar_can_seed_active_controls() {
        let title_bar = TitleBar::new("Demo")
            .control("A Action")
            .active_control("T Toggle");
        let state = TitleBarControlState::from_title_bar(&title_bar);

        assert!(!state.is_active("A Action"));
        assert!(state.is_active("T Toggle"));
    }
}
