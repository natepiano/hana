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
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;

use super::CONTROL_ACTIVE_COLOR;
use super::CONTROL_INACTIVE_COLOR;
use super::CONTROL_SIZE;
use super::DIVIDER_COLOR;
use super::panel_frame;
use super::unlit_panel_material;
use crate::camera_home;
use crate::camera_home::CameraHomeConfig;
use crate::ui::theme::TITLE_COLOR;
use crate::ui::theme::TITLE_SIZE;

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
struct TitleBarMarker;

pub(super) fn spawn_title_bar_with_home_chip(
    commands: &mut Commands,
    title_bar: &TitleBar,
    home: Option<&CameraHomeConfig>,
) {
    let mut bar = title_bar.clone();
    if home.is_some()
        && !bar
            .controls
            .iter()
            .any(|control| control == camera_home::HOME_CONTROL)
    {
        bar.controls
            .insert(0, camera_home::HOME_CONTROL.to_string());
    }
    spawn_title_bar(commands, &bar);
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
