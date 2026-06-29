//! Bottom-left Fairy Dust panel listing the nine sections. The current row is
//! highlighted; the `\u{2190}`/`\u{2192}` header arrows highlight the last-used
//! direction and grey out at the first/last section. Arrow keys step between
//! sections, number keys 1-9 jump.

use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_diegetic::AlignY;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use bevy_lagrange::ZoomToFit;
use fairy_dust::Anchor;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TITLE_COLOR;

use super::constants::CAP_STYLES_SECTION_INDEX;
use super::constants::CATENARY_SECTION_INDEX;
use super::constants::CONNECTOR_SECTION_INDEX;
use super::constants::DETACH_DEMO_SECTION_INDEX;
use super::constants::ENTITY_ATTACHMENT_SECTION_INDEX;
use super::constants::INSIDE_VIEW_SECTION_INDEX;
use super::constants::NAV_PANEL_ACTIVE_COLOR;
use super::constants::NAV_PANEL_DISABLED_COLOR;
use super::constants::NAV_PANEL_HEADER_GAP;
use super::constants::NAV_PANEL_HEADER_SIZE;
use super::constants::NAV_PANEL_LEFT_ARROW;
use super::constants::NAV_PANEL_NORMAL_COLOR;
use super::constants::NAV_PANEL_RIGHT_ARROW;
use super::constants::NAV_PANEL_ROW_GAP;
use super::constants::NAVIGATION_DURATION_MS;
use super::constants::ORTHOGONAL_ROUTING_SECTION_INDEX;
use super::constants::SECTION_COUNT;
use super::constants::SECTION_TITLES;
use super::constants::SHARED_HUB_SECTION_INDEX;
use super::constants::SOLVER_COMPARISON_SECTION_INDEX;
use super::constants::ZOOM_MARGIN_NAVIGATION;
use super::sections::CurrentSection;
use super::sections::SectionBounds;

/// Which neighbor an arrow step moves toward; drives the header arrow highlight.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigationDirection {
    Left,
    Right,
}

#[derive(Clone, Copy)]
enum NavigationArrowState {
    Disabled,
    Highlighted,
    Normal,
}

#[derive(Clone, Copy)]
enum NavigationRequest {
    Section(usize),
    Step(NavigationDirection),
}

/// The last navigation action: `Some` after an arrow step (highlights that
/// arrow), `None` after a number jump (no arrow highlighted).
#[derive(Resource, Default)]
pub(crate) struct NavSelection {
    pub(crate) direction: Option<NavigationDirection>,
}

/// A Fairy Dust shortcut request consumed by `apply_navigation_request`.
#[derive(Resource, Default)]
pub(crate) struct RequestedNavigation(Option<NavigationRequest>);

#[derive(Component)]
pub(crate) struct NavPanel;

pub(crate) fn spawn_nav_panel(
    mut commands: Commands,
    current_section: Res<CurrentSection>,
    selection: Res<NavSelection>,
) {
    let unlit = fairy_dust::screen_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_nav_tree(current_section.0, selection.direction))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((NavPanel, panel, Transform::default()));
        },
        Err(error) => {
            error!("playground: failed to build nav panel: {error}");
        },
    }
}

pub(crate) fn refresh_nav_panel(
    mut commands: Commands,
    current_section: Res<CurrentSection>,
    selection: Res<NavSelection>,
    panel: Single<Entity, With<NavPanel>>,
) {
    if !current_section.is_changed() && !selection.is_changed() {
        return;
    }
    commands.set_tree(
        *panel,
        build_nav_tree(current_section.0, selection.direction),
    );
}

pub(crate) fn request_previous_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Step(NavigationDirection::Left));
}

pub(crate) fn request_next_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Step(NavigationDirection::Right));
}

pub(crate) fn request_catenary_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Section(CATENARY_SECTION_INDEX));
}

pub(crate) fn request_cap_styles_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Section(CAP_STYLES_SECTION_INDEX));
}

pub(crate) fn request_solver_comparison_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Section(SOLVER_COMPARISON_SECTION_INDEX));
}

pub(crate) fn request_entity_attachment_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Section(ENTITY_ATTACHMENT_SECTION_INDEX));
}

pub(crate) fn request_shared_hub_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Section(SHARED_HUB_SECTION_INDEX));
}

pub(crate) fn request_orthogonal_routing_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Section(ORTHOGONAL_ROUTING_SECTION_INDEX));
}

pub(crate) fn request_detach_demo_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Section(DETACH_DEMO_SECTION_INDEX));
}

pub(crate) fn request_inside_view_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Section(INSIDE_VIEW_SECTION_INDEX));
}

pub(crate) fn request_connector_section(mut requested: ResMut<RequestedNavigation>) {
    requested.0 = Some(NavigationRequest::Section(CONNECTOR_SECTION_INDEX));
}

pub(crate) fn apply_navigation_request(
    mut commands: Commands,
    mut requested: ResMut<RequestedNavigation>,
    mut current_section: ResMut<CurrentSection>,
    mut selection: ResMut<NavSelection>,
    camera: Single<Entity, With<FairyDustOrbitCam>>,
    section_bounds: Res<SectionBounds>,
) {
    let Some(request) = requested.0.take() else {
        return;
    };
    let Some((section, direction)) = resolve_navigation_request(request, current_section.0) else {
        return;
    };

    selection.direction = direction;
    current_section.0 = section;
    commands.trigger(
        ZoomToFit::new(*camera, section_bounds.0[section])
            .margin(ZOOM_MARGIN_NAVIGATION)
            .duration(Duration::from_millis(NAVIGATION_DURATION_MS))
            .easing(EaseFunction::CubicInOut),
    );
}

const fn resolve_navigation_request(
    request: NavigationRequest,
    current_section: usize,
) -> Option<(usize, Option<NavigationDirection>)> {
    match request {
        NavigationRequest::Section(section) if section < SECTION_COUNT => Some((section, None)),
        NavigationRequest::Step(NavigationDirection::Right)
            if current_section < SECTION_COUNT - 1 =>
        {
            Some((current_section + 1, Some(NavigationDirection::Right)))
        },
        NavigationRequest::Step(NavigationDirection::Left) if current_section > 0 => {
            Some((current_section - 1, Some(NavigationDirection::Left)))
        },
        NavigationRequest::Section(_) | NavigationRequest::Step(_) => None,
    }
}

fn build_nav_tree(current: usize, direction: Option<NavigationDirection>) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    fairy_dust::screen_panel_frame(
        &mut builder,
        Sizing::FIT,
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(El::column().gap(NAV_PANEL_ROW_GAP), |builder| {
                build_header(builder, current, direction);
                for (section, title) in SECTION_TITLES.iter().enumerate() {
                    builder.text(
                        format!("{} {title}", section + 1),
                        text_style(if section == current {
                            NAV_PANEL_ACTIVE_COLOR
                        } else {
                            NAV_PANEL_NORMAL_COLOR
                        }),
                    );
                }
            });
        },
    );
    builder.build()
}

fn build_header(
    builder: &mut LayoutBuilder,
    current: usize,
    direction: Option<NavigationDirection>,
) {
    let left = arrow_color(left_arrow_state(current, direction));
    let right = arrow_color(right_arrow_state(current, direction));
    builder.with(
        El::row().gap(NAV_PANEL_HEADER_GAP).align_y(AlignY::Center),
        |builder| {
            builder.text(NAV_PANEL_LEFT_ARROW, text_style(left));
            builder.text("Sections", header_text_style(TITLE_COLOR));
            builder.text(NAV_PANEL_RIGHT_ARROW, text_style(right));
        },
    );
}

const fn left_arrow_state(
    current: usize,
    direction: Option<NavigationDirection>,
) -> NavigationArrowState {
    if current == 0 {
        NavigationArrowState::Disabled
    } else if matches!(direction, Some(NavigationDirection::Left)) {
        NavigationArrowState::Highlighted
    } else {
        NavigationArrowState::Normal
    }
}

const fn right_arrow_state(
    current: usize,
    direction: Option<NavigationDirection>,
) -> NavigationArrowState {
    if current >= SECTION_COUNT - 1 {
        NavigationArrowState::Disabled
    } else if matches!(direction, Some(NavigationDirection::Right)) {
        NavigationArrowState::Highlighted
    } else {
        NavigationArrowState::Normal
    }
}

const fn arrow_color(state: NavigationArrowState) -> Color {
    match state {
        NavigationArrowState::Disabled => NAV_PANEL_DISABLED_COLOR,
        NavigationArrowState::Highlighted => NAV_PANEL_ACTIVE_COLOR,
        NavigationArrowState::Normal => NAV_PANEL_NORMAL_COLOR,
    }
}

fn text_style(color: Color) -> TextStyle {
    TextStyle::new(LABEL_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn header_text_style(color: Color) -> TextStyle {
    TextStyle::new(NAV_PANEL_HEADER_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}
