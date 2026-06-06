//! UI overlays for the cable playground.

use bevy::picking::Pickable;
use bevy::prelude::*;

use super::constants::CAMERA_HELP;
use super::constants::KEYBOARD_SHORTCUTS_HELP;
use super::constants::OVERLAY_MARGIN;
use super::constants::SECTION_INFO_BACKGROUND;
use super::constants::SECTION_INFO_CENTER_X_PERCENT;
use super::constants::SECTION_INFO_LEFT_OFFSET;
use super::constants::SECTION_INFO_TEXTS;
use super::constants::SECTION_INFO_TOP;
use super::constants::SECTION_INFO_WIDTH;
use super::constants::UI_FONT_SIZE;
use super::navigation;
use super::scene::SceneEntities;
use super::sections::SectionInfo;

pub(crate) fn setup_ui(mut commands: Commands, scene_entities: Res<SceneEntities>) {
    spawn_help_text(&mut commands, scene_entities.camera);
    spawn_keyboard_shortcuts(&mut commands, scene_entities.camera);
    navigation::spawn_navigation_bar(&mut commands, scene_entities.camera);
    spawn_section_infos(&mut commands, scene_entities.camera);
}

fn spawn_section_infos(commands: &mut Commands, camera: Entity) {
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
            UiTargetCamera(camera),
            SectionInfo(section),
            Visibility::Hidden,
        ));
    }
}

fn spawn_help_text(commands: &mut Commands, camera: Entity) {
    commands.spawn((
        Text::new(CAMERA_HELP),
        TextFont {
            font_size: FontSize::Px(UI_FONT_SIZE),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(OVERLAY_MARGIN),
            left: Val::Px(OVERLAY_MARGIN),
            ..default()
        },
        Pickable::IGNORE,
        UiTargetCamera(camera),
    ));
}

fn spawn_keyboard_shortcuts(commands: &mut Commands, camera: Entity) {
    commands.spawn((
        Text::new(KEYBOARD_SHORTCUTS_HELP),
        TextFont {
            font_size: FontSize::Px(UI_FONT_SIZE),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(OVERLAY_MARGIN),
            left: Val::Px(OVERLAY_MARGIN),
            ..default()
        },
        Pickable::IGNORE,
        UiTargetCamera(camera),
    ));
}
