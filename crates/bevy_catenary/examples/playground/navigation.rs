//! Section navigation UI: navigation bar, left/right arrows, and keyboard shortcuts.

use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_lagrange::ZoomToFit;

use super::constants::CATENARY_SECTION_INDEX;
use super::constants::NAVIGATION_BAR_BACKGROUND;
use super::constants::NAVIGATION_BAR_BORDER_RADIUS;
use super::constants::NAVIGATION_BAR_BOTTOM;
use super::constants::NAVIGATION_BAR_CENTER_X_PERCENT;
use super::constants::NAVIGATION_BAR_COLUMN_GAP;
use super::constants::NAVIGATION_BAR_HORIZONTAL_OFFSET;
use super::constants::NAVIGATION_BAR_HORIZONTAL_PADDING;
use super::constants::NAVIGATION_BAR_VERTICAL_PADDING;
use super::constants::NAVIGATION_BUTTON_BACKGROUND;
use super::constants::NAVIGATION_BUTTON_BORDER_RADIUS;
use super::constants::NAVIGATION_BUTTON_HORIZONTAL_PADDING;
use super::constants::NAVIGATION_BUTTON_VERTICAL_PADDING;
use super::constants::NAVIGATION_DURATION_MS;
use super::constants::NAVIGATION_FONT_SIZE;
use super::constants::NAVIGATION_LABEL_WIDTH;
use super::constants::NAVIGATION_NEXT_LABEL;
use super::constants::NAVIGATION_PREVIOUS_LABEL;
use super::constants::SECTION_COUNT;
use super::constants::SECTION_TITLES;
use super::constants::ZOOM_MARGIN_NAVIGATION;
use super::scene::SceneEntities;
use super::sections::CurrentSection;
use super::sections::SectionBounds;

#[derive(Component)]
pub(crate) struct NavigationLabel;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigationDirection {
    Left,
    Right,
}

#[derive(Component)]
pub(crate) struct NavigationButton(pub(crate) NavigationDirection);

pub(crate) fn navigate_to_section(
    commands: &mut Commands,
    section: usize,
    current_section: &mut ResMut<CurrentSection>,
    scene_entities: &Res<SceneEntities>,
    section_bounds: &Res<SectionBounds>,
    label_query: &mut Query<&mut Text, With<NavigationLabel>>,
) {
    current_section.0 = section;
    commands.trigger(
        ZoomToFit::new(scene_entities.camera, section_bounds.0[section])
            .margin(ZOOM_MARGIN_NAVIGATION)
            .duration(Duration::from_millis(NAVIGATION_DURATION_MS))
            .easing(EaseFunction::CubicInOut),
    );
    update_navigation_label(label_query, section);
}

pub(crate) fn update_navigation_label(
    label_query: &mut Query<&mut Text, With<NavigationLabel>>,
    section: usize,
) {
    let section_number = section + 1;
    for mut text in label_query.iter_mut() {
        **text = format!(
            "{section_number} / {SECTION_COUNT} - {}",
            SECTION_TITLES[section]
        );
    }
}

pub(crate) fn handle_navigation_buttons(
    interactions: Query<(&Interaction, &NavigationButton), Changed<Interaction>>,
    mut commands: Commands,
    mut current_section: ResMut<CurrentSection>,
    scene_entities: Res<SceneEntities>,
    section_bounds: Res<SectionBounds>,
    mut label_query: Query<&mut Text, With<NavigationLabel>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let mut new_section = None;

    for (interaction, navigation_button) in &interactions {
        if *interaction == Interaction::Pressed {
            match navigation_button.0 {
                NavigationDirection::Left if current_section.0 > 0 => {
                    new_section = Some(current_section.0 - 1);
                },
                NavigationDirection::Right if current_section.0 < SECTION_COUNT - 1 => {
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

pub(crate) fn spawn_navigation_bar(commands: &mut Commands, camera: Entity) {
    let initial_section_number = CATENARY_SECTION_INDEX + 1;
    let initial_section_title = SECTION_TITLES[CATENARY_SECTION_INDEX];
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(NAVIGATION_BAR_BOTTOM),
                left: Val::Percent(NAVIGATION_BAR_CENTER_X_PERCENT),
                margin: UiRect::left(Val::Px(NAVIGATION_BAR_HORIZONTAL_OFFSET)),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(NAVIGATION_BAR_COLUMN_GAP),
                padding: UiRect::axes(
                    Val::Px(NAVIGATION_BAR_HORIZONTAL_PADDING),
                    Val::Px(NAVIGATION_BAR_VERTICAL_PADDING),
                ),
                border_radius: BorderRadius::all(Val::Px(NAVIGATION_BAR_BORDER_RADIUS)),
                ..default()
            },
            BackgroundColor(NAVIGATION_BAR_BACKGROUND),
            Pickable::IGNORE,
            UiTargetCamera(camera),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(
                            Val::Px(NAVIGATION_BUTTON_HORIZONTAL_PADDING),
                            Val::Px(NAVIGATION_BUTTON_VERTICAL_PADDING),
                        ),
                        border_radius: BorderRadius::all(Val::Px(NAVIGATION_BUTTON_BORDER_RADIUS)),
                        ..default()
                    },
                    BackgroundColor(NAVIGATION_BUTTON_BACKGROUND),
                    NavigationButton(NavigationDirection::Left),
                ))
                .with_child((
                    Text::new(NAVIGATION_PREVIOUS_LABEL),
                    TextFont {
                        font_size: FontSize::Px(NAVIGATION_FONT_SIZE),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));

            parent
                .spawn(Node {
                    width: Val::Px(NAVIGATION_LABEL_WIDTH),
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_child((
                    Text::new(format!(
                        "{initial_section_number} / {SECTION_COUNT} - {initial_section_title}"
                    )),
                    TextFont {
                        font_size: FontSize::Px(NAVIGATION_FONT_SIZE),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    NavigationLabel,
                ));

            parent
                .spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(
                            Val::Px(NAVIGATION_BUTTON_HORIZONTAL_PADDING),
                            Val::Px(NAVIGATION_BUTTON_VERTICAL_PADDING),
                        ),
                        border_radius: BorderRadius::all(Val::Px(NAVIGATION_BUTTON_BORDER_RADIUS)),
                        ..default()
                    },
                    BackgroundColor(NAVIGATION_BUTTON_BACKGROUND),
                    NavigationButton(NavigationDirection::Right),
                ))
                .with_child((
                    Text::new(NAVIGATION_NEXT_LABEL),
                    TextFont {
                        font_size: FontSize::Px(NAVIGATION_FONT_SIZE),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
        });
}
