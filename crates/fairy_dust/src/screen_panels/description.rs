//! Static side-panel that explains what an example demonstrates.

use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Sizing;

use super::constants::BODY_COLOR;
use super::constants::BODY_SIZE;
use super::constants::DESCRIPTION_CHILD_GAP;
use super::constants::DESCRIPTION_WIDTH;
use super::panel_frame;
use super::unlit_panel_material;
use crate::constants::TITLE_COLOR;
use crate::constants::TITLE_SIZE;

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

#[derive(Component)]
struct DescriptionPanelMarker;

pub(super) fn spawn_description_panel(commands: &mut Commands, panel: &DescriptionPanel) {
    let unlit = unlit_panel_material();
    let built = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(panel.anchor)
        .material(unlit.clone())
        .text_material(unlit)
        .layout(|builder| build_description_layout(builder, panel))
        .build();

    match built {
        Ok(built) => {
            commands.spawn((DescriptionPanelMarker, built, Transform::default()));
        },
        Err(error) => {
            error!("fairy_dust: failed to build description panel: {error}");
        },
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
                .child_gap(DESCRIPTION_CHILD_GAP),
            |builder| {
                builder.text(panel.title.to_uppercase(), title);
                for line in &panel.lines {
                    builder.text(line, body.clone());
                }
            },
        );
    });
}
