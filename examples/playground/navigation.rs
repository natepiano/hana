//! Section navigation UI: nav bar, left/right arrows, and keyboard shortcuts.

use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_lagrange::ZoomToFit;

use super::constants::CATENARY_SECTION_INDEX;
use super::constants::NAV_BAR_BACKGROUND;
use super::constants::NAV_BAR_BORDER_RADIUS;
use super::constants::NAV_BAR_BOTTOM;
use super::constants::NAV_BAR_CENTER_X_PERCENT;
use super::constants::NAV_BAR_COLUMN_GAP;
use super::constants::NAV_BAR_HORIZONTAL_OFFSET;
use super::constants::NAV_BAR_HORIZONTAL_PADDING;
use super::constants::NAV_BAR_VERTICAL_PADDING;
use super::constants::NAV_BUTTON_BACKGROUND;
use super::constants::NAV_BUTTON_BORDER_RADIUS;
use super::constants::NAV_BUTTON_HORIZONTAL_PADDING;
use super::constants::NAV_BUTTON_VERTICAL_PADDING;
use super::constants::NAV_DURATION_MS;
use super::constants::NAV_FONT_SIZE;
use super::constants::NAV_LABEL_WIDTH;
use super::constants::NAV_NEXT_LABEL;
use super::constants::NAV_PREVIOUS_LABEL;
use super::constants::SECTION_COUNT;
use super::constants::SECTION_TITLES;
use super::constants::ZOOM_MARGIN_NAV;
use super::scene::SceneEntities;
use super::sections::CurrentSection;
use super::sections::SectionBounds;

#[derive(Component)]
pub(crate) struct NavLabel;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavDirection {
    Left,
    Right,
}

#[derive(Component)]
pub(crate) struct NavButton(pub(crate) NavDirection);

pub(crate) fn navigate_to_section(
    commands: &mut Commands,
    section: usize,
    current_section: &mut ResMut<CurrentSection>,
    scene_entities: &Res<SceneEntities>,
    section_bounds: &Res<SectionBounds>,
    label_query: &mut Query<&mut Text, With<NavLabel>>,
) {
    current_section.0 = section;
    commands.trigger(
        ZoomToFit::new(scene_entities.camera, section_bounds.0[section])
            .margin(ZOOM_MARGIN_NAV)
            .duration(Duration::from_millis(NAV_DURATION_MS))
            .easing(EaseFunction::CubicInOut),
    );
    update_nav_label(label_query, section);
}

pub(crate) fn update_nav_label(label_query: &mut Query<&mut Text, With<NavLabel>>, section: usize) {
    let section_number = section + 1;
    for mut text in label_query.iter_mut() {
        **text = format!(
            "{section_number} / {SECTION_COUNT} - {}",
            SECTION_TITLES[section]
        );
    }
}

pub(crate) fn handle_nav_buttons(
    interactions: Query<(&Interaction, &NavButton), Changed<Interaction>>,
    mut commands: Commands,
    mut current_section: ResMut<CurrentSection>,
    scene_entities: Res<SceneEntities>,
    section_bounds: Res<SectionBounds>,
    mut label_query: Query<&mut Text, With<NavLabel>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let mut new_section = None;

    for (interaction, nav) in &interactions {
        if *interaction == Interaction::Pressed {
            match nav.0 {
                NavDirection::Left if current_section.0 > 0 => {
                    new_section = Some(current_section.0 - 1);
                },
                NavDirection::Right if current_section.0 < SECTION_COUNT - 1 => {
                    new_section = Some(current_section.0 + 1);
                },
                _ => {},
            }
        }
    }

    if keyboard.just_pressed(KeyCode::ArrowLeft) && current_section.0 > 0 {
        new_section = Some(current_section.0 - 1);
    }
    if keyboard.just_pressed(KeyCode::ArrowRight) && current_section.0 < SECTION_COUNT - 1 {
        new_section = Some(current_section.0 + 1);
    }

    let number_keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    for (i, key) in number_keys.iter().enumerate() {
        if keyboard.just_pressed(*key) && i < SECTION_COUNT {
            new_section = Some(i);
        }
    }

    if let Some(section) = new_section {
        navigate_to_section(
            &mut commands,
            section,
            &mut current_section,
            &scene_entities,
            &section_bounds,
            &mut label_query,
        );
    }
}

pub(crate) fn spawn_nav_bar(commands: &mut Commands, camera: Entity) {
    let initial_section_number = CATENARY_SECTION_INDEX + 1;
    let initial_section_title = SECTION_TITLES[CATENARY_SECTION_INDEX];
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(NAV_BAR_BOTTOM),
                left: Val::Percent(NAV_BAR_CENTER_X_PERCENT),
                margin: UiRect::left(Val::Px(NAV_BAR_HORIZONTAL_OFFSET)),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(NAV_BAR_COLUMN_GAP),
                padding: UiRect::axes(
                    Val::Px(NAV_BAR_HORIZONTAL_PADDING),
                    Val::Px(NAV_BAR_VERTICAL_PADDING),
                ),
                border_radius: BorderRadius::all(Val::Px(NAV_BAR_BORDER_RADIUS)),
                ..default()
            },
            BackgroundColor(NAV_BAR_BACKGROUND),
            Pickable::IGNORE,
            UiTargetCamera(camera),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(
                            Val::Px(NAV_BUTTON_HORIZONTAL_PADDING),
                            Val::Px(NAV_BUTTON_VERTICAL_PADDING),
                        ),
                        border_radius: BorderRadius::all(Val::Px(NAV_BUTTON_BORDER_RADIUS)),
                        ..default()
                    },
                    BackgroundColor(NAV_BUTTON_BACKGROUND),
                    NavButton(NavDirection::Left),
                ))
                .with_child((
                    Text::new(NAV_PREVIOUS_LABEL),
                    TextFont {
                        font_size: NAV_FONT_SIZE,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));

            parent
                .spawn(Node {
                    width: Val::Px(NAV_LABEL_WIDTH),
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_child((
                    Text::new(format!(
                        "{initial_section_number} / {SECTION_COUNT} - {initial_section_title}"
                    )),
                    TextFont {
                        font_size: NAV_FONT_SIZE,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    NavLabel,
                ));

            parent
                .spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(
                            Val::Px(NAV_BUTTON_HORIZONTAL_PADDING),
                            Val::Px(NAV_BUTTON_VERTICAL_PADDING),
                        ),
                        border_radius: BorderRadius::all(Val::Px(NAV_BUTTON_BORDER_RADIUS)),
                        ..default()
                    },
                    BackgroundColor(NAV_BUTTON_BACKGROUND),
                    NavButton(NavDirection::Right),
                ))
                .with_child((
                    Text::new(NAV_NEXT_LABEL),
                    TextFont {
                        font_size: NAV_FONT_SIZE,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
        });
}
