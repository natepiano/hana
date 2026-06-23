//! Per-section info overlays for the cable playground.
//!
//! The title bar, camera-control panel, and bottom-left section nav panel come
//! from Fairy Dust panels elsewhere; this module spawns only the top-center
//! per-section hint text.

use bevy::picking::Pickable;
use bevy::prelude::*;

use super::constants::SECTION_INFO_BACKGROUND;
use super::constants::SECTION_INFO_CENTER_X_PERCENT;
use super::constants::SECTION_INFO_LEFT_OFFSET;
use super::constants::SECTION_INFO_TEXTS;
use super::constants::SECTION_INFO_TOP;
use super::constants::SECTION_INFO_WIDTH;
use super::constants::UI_FONT_SIZE;
use super::sections::SectionInfo;

pub(crate) fn setup_ui(mut commands: Commands) { spawn_section_infos(&mut commands); }

fn spawn_section_infos(commands: &mut Commands) {
    for (section, text) in SECTION_INFO_TEXTS {
        commands.spawn((
            Text::new(text),
            TextFont {
                font_size: FontSize::Px(UI_FONT_SIZE),
                ..default()
            },
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(SECTION_INFO_TOP),
                left: Val::Percent(SECTION_INFO_CENTER_X_PERCENT),
                margin: UiRect::left(Val::Px(SECTION_INFO_LEFT_OFFSET)),
                width: Val::Px(SECTION_INFO_WIDTH),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(SECTION_INFO_BACKGROUND),
            Pickable::IGNORE,
            SectionInfo(section),
            Visibility::Hidden,
        ));
    }
}
