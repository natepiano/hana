//! Capability: screen-space `bevy_diegetic` camera guidance panels for
//! `bevy_lagrange::OrbitCam` examples.

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
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
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::default_panel_material;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::OrbitCamInteractionEnded;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionSourcesChanged;
use bevy_lagrange::OrbitCamInteractionStarted;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::OrbitCamPreset;

use crate::ensure_plugin;

/// Data-driven camera guidance shown by [`SprinkleBuilder`](crate::SprinkleBuilder)
/// examples.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct CameraGuidance {
    title:        String,
    rows:         Vec<CameraGuidanceRow>,
    show_sources: bool,
}

impl Default for CameraGuidance {
    fn default() -> Self { Self::for_preset(OrbitCamPreset::SimpleMouse) }
}

impl CameraGuidance {
    /// Builds guidance rows for a built-in orbit-camera preset.
    #[must_use]
    pub fn for_preset(preset: OrbitCamPreset) -> Self {
        match preset {
            OrbitCamPreset::SimpleMouse => Self::custom([
                CameraGuidanceRow::new(OrbitCamInteractionKind::Orbit, "Left drag -> Orbit")
                    .when_sources(CameraInteractionSources::MOUSE),
                CameraGuidanceRow::new(OrbitCamInteractionKind::Pan, "Right drag -> Pan")
                    .when_sources(CameraInteractionSources::MOUSE),
                CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "Wheel -> Zoom")
                    .when_sources(CameraInteractionSources::WHEEL),
                CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "Pinch -> Zoom")
                    .when_sources(CameraInteractionSources::PINCH),
            ])
            .with_title("Simple Mouse"),
            OrbitCamPreset::BlenderLike => Self::custom([
                CameraGuidanceRow::new(OrbitCamInteractionKind::Orbit, "MMB drag -> Orbit")
                    .when_sources(CameraInteractionSources::MOUSE),
                CameraGuidanceRow::new(OrbitCamInteractionKind::Pan, "Shift+MMB -> Pan")
                    .when_sources(CameraInteractionSources::MOUSE),
                CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "Wheel -> Zoom")
                    .when_sources(CameraInteractionSources::WHEEL),
                CameraGuidanceRow::new(OrbitCamInteractionKind::Orbit, "Trackpad -> Orbit")
                    .when_sources(CameraInteractionSources::SMOOTH_SCROLL),
                CameraGuidanceRow::new(OrbitCamInteractionKind::Pan, "Shift+trackpad -> Pan")
                    .when_sources(CameraInteractionSources::SMOOTH_SCROLL),
                CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "Ctrl+trackpad -> Zoom")
                    .when_sources(CameraInteractionSources::SMOOTH_SCROLL),
                CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "Pinch -> Zoom")
                    .when_sources(CameraInteractionSources::PINCH),
            ])
            .with_title("Blender Like"),
            _ => Self::for_preset(OrbitCamPreset::SimpleMouse),
        }
    }

    /// Builds custom camera guidance rows.
    #[must_use]
    pub fn custom(rows: impl IntoIterator<Item = CameraGuidanceRow>) -> Self {
        Self {
            title:        "Camera".to_string(),
            rows:         rows.into_iter().collect(),
            show_sources: true,
        }
    }

    /// Replaces the panel title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Controls whether active source labels are rendered.
    #[must_use]
    pub const fn with_source_flags(mut self, show_sources: bool) -> Self {
        self.show_sources = show_sources;
        self
    }

    /// Returns the configured rows.
    #[must_use]
    pub fn rows(&self) -> &[CameraGuidanceRow] { &self.rows }
}

/// A single camera guidance row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraGuidanceRow {
    kind:    OrbitCamInteractionKind,
    label:   String,
    sources: Option<CameraInteractionSources>,
}

impl CameraGuidanceRow {
    /// Creates a row for an interaction kind.
    #[must_use]
    pub fn new(kind: OrbitCamInteractionKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            sources: None,
        }
    }

    /// Highlights this row only when the active sources intersect `sources`.
    #[must_use]
    pub const fn when_sources(mut self, sources: CameraInteractionSources) -> Self {
        self.sources = Some(sources);
        self
    }

    /// Returns the interaction kind matched by this row.
    #[must_use]
    pub const fn kind(&self) -> OrbitCamInteractionKind { self.kind }

    /// Returns this row's source predicate.
    #[must_use]
    pub const fn sources(&self) -> Option<CameraInteractionSources> { self.sources }

    /// Returns the display label.
    #[must_use]
    pub fn label(&self) -> &str { &self.label }
}

#[derive(Component)]
struct StaticCameraControlPanel;

#[derive(Component)]
struct CameraGuidancePanel {
    camera: Entity,
}

const RADIUS: Px = Px(8.0);
const FRAME_PAD: Px = Px(2.0);
const BORDER: Px = Px(2.0);
const INSET: Px = Px(FRAME_PAD.0 + BORDER.0);
const INNER_RADIUS: Px = Px(RADIUS.0 - INSET.0);

const TITLE_SIZE: Pt = Pt(16.0);
const HEADER_SIZE: Pt = Pt(12.0);
const LABEL_SIZE: Pt = Pt(11.0);

const FRAME_BG: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const INNER_BG: Color = Color::srgba(0.02, 0.03, 0.07, 0.80);
const BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HEADER_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
const LABEL_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);
const ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
const SOURCE_COLOR: Color = Color::srgba(0.35, 0.8, 1.0, 0.95);

pub(crate) fn install(app: &mut App) {
    ensure_panel_plugins(app);
    app.add_systems(Startup, spawn_static_panel);
}

pub(crate) fn install_guidance(app: &mut App) {
    ensure_panel_plugins(app);
    app.add_systems(
        PostUpdate,
        (spawn_guidance_panels, refresh_changed_guidance),
    );
    app.add_observer(refresh_on_interaction_started)
        .add_observer(refresh_on_interaction_ended)
        .add_observer(refresh_on_sources_changed);
}

fn ensure_panel_plugins(app: &mut App) {
    ensure_plugin(app, DiegeticUiPlugin);
    ensure_plugin(app, MeshPickingPlugin);
}

fn spawn_static_panel(mut commands: Commands) {
    let unlit = unlit_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomRight)
        .material(unlit.clone())
        .text_material(unlit)
        .layout(build_static_layout)
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((StaticCameraControlPanel, panel, Transform::default()));
        },
        Err(error) => {
            error!("fairy_dust: failed to build camera control panel: {error}");
        },
    }
}

fn spawn_guidance_panels(
    mut commands: Commands,
    cameras: Query<
        (Entity, &CameraGuidance, Option<&OrbitCamInteractionState>),
        Added<CameraGuidance>,
    >,
) {
    for (camera, guidance, state) in &cameras {
        let unlit = unlit_panel_material();
        let panel = DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(Anchor::BottomRight)
            .material(unlit.clone())
            .text_material(unlit)
            .with_tree(build_guidance_tree(
                guidance,
                state.copied().unwrap_or_default(),
            ))
            .build();

        match panel {
            Ok(panel) => {
                commands.spawn((CameraGuidancePanel { camera }, panel, Transform::default()));
            },
            Err(error) => {
                error!("fairy_dust: failed to build camera guidance panel: {error}");
            },
        }
    }
}

fn refresh_changed_guidance(
    mut commands: Commands,
    cameras: Query<
        (Entity, &CameraGuidance, &OrbitCamInteractionState),
        Or<(Changed<CameraGuidance>, Changed<OrbitCamInteractionState>)>,
    >,
    panels: Query<(Entity, &CameraGuidancePanel)>,
) {
    for (camera, guidance, state) in &cameras {
        refresh_camera_guidance(camera, guidance, *state, &mut commands, &panels);
    }
}

fn refresh_on_interaction_started(
    event: On<OrbitCamInteractionStarted>,
    mut commands: Commands,
    cameras: Query<(&CameraGuidance, &OrbitCamInteractionState)>,
    panels: Query<(Entity, &CameraGuidancePanel)>,
) {
    refresh_camera_guidance_from_event(event.camera, &mut commands, &cameras, &panels);
}

fn refresh_on_interaction_ended(
    event: On<OrbitCamInteractionEnded>,
    mut commands: Commands,
    cameras: Query<(&CameraGuidance, &OrbitCamInteractionState)>,
    panels: Query<(Entity, &CameraGuidancePanel)>,
) {
    refresh_camera_guidance_from_event(event.camera, &mut commands, &cameras, &panels);
}

fn refresh_on_sources_changed(
    event: On<OrbitCamInteractionSourcesChanged>,
    mut commands: Commands,
    cameras: Query<(&CameraGuidance, &OrbitCamInteractionState)>,
    panels: Query<(Entity, &CameraGuidancePanel)>,
) {
    refresh_camera_guidance_from_event(event.camera, &mut commands, &cameras, &panels);
}

fn refresh_camera_guidance_from_event(
    camera: Entity,
    commands: &mut Commands,
    cameras: &Query<(&CameraGuidance, &OrbitCamInteractionState)>,
    panels: &Query<(Entity, &CameraGuidancePanel)>,
) {
    let Ok((guidance, state)) = cameras.get(camera) else {
        return;
    };
    refresh_camera_guidance(camera, guidance, *state, commands, panels);
}

fn refresh_camera_guidance(
    camera: Entity,
    guidance: &CameraGuidance,
    state: OrbitCamInteractionState,
    commands: &mut Commands,
    panels: &Query<(Entity, &CameraGuidancePanel)>,
) {
    for (panel, panel_camera) in panels {
        if panel_camera.camera == camera {
            commands.set_tree(panel, build_guidance_tree(guidance, state));
        }
    }
}

fn unlit_panel_material() -> StandardMaterial {
    StandardMaterial {
        unlit: true,
        ..default_panel_material()
    }
}

fn build_static_layout(builder: &mut LayoutBuilder) {
    let guidance = CameraGuidance::for_preset(OrbitCamPreset::SimpleMouse);
    build_guidance_layout(builder, &guidance, OrbitCamInteractionState::default());
}

fn build_guidance_tree(guidance: &CameraGuidance, state: OrbitCamInteractionState) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_guidance_layout(&mut builder, guidance, state);
    builder.build()
}

fn build_guidance_layout(
    builder: &mut LayoutBuilder,
    guidance: &CameraGuidance,
    state: OrbitCamInteractionState,
) {
    let title = LayoutTextStyle::new(TITLE_SIZE).with_color(TITLE_COLOR);
    let header = LayoutTextStyle::new(HEADER_SIZE).with_color(HEADER_COLOR);
    let label = LayoutTextStyle::new(LABEL_SIZE).with_color(LABEL_COLOR);
    let active = LayoutTextStyle::new(LABEL_SIZE).with_color(ACTIVE_COLOR);
    let source = LayoutTextStyle::new(LABEL_SIZE).with_color(SOURCE_COLOR);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(FRAME_PAD))
            .corner_radius(CornerRadius::all(RADIUS))
            .background(FRAME_BG)
            .border(Border::all(BORDER, BORDER_ACCENT)),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(Px(10.0)))
                    .child_gap(Px(5.0))
                    .corner_radius(CornerRadius::all(INNER_RADIUS))
                    .background(INNER_BG)
                    .border(Border::all(Px(1.0), BORDER_DIM)),
                |builder| {
                    builder.text(guidance.title.to_uppercase(), title.clone());
                    builder.text("Orbit / Pan / Zoom", header.clone());
                    for row in guidance.rows() {
                        build_guidance_row(
                            builder,
                            row,
                            state,
                            guidance.show_sources,
                            &label,
                            &active,
                        );
                    }
                    let sources = state
                        .orbit_sources()
                        .union(state.pan_sources())
                        .union(state.zoom_sources());
                    builder.text(format!("sources: {}", source_label(sources)), source);
                },
            );
        },
    );
}

fn build_guidance_row(
    builder: &mut LayoutBuilder,
    row: &CameraGuidanceRow,
    state: OrbitCamInteractionState,
    show_sources: bool,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
    let active_sources = state.sources(row.kind());
    let is_active = row_active(row, active_sources);
    let style = if is_active { active } else { label };
    let mut text = if is_active {
        format!("> {}", row.label())
    } else {
        format!("  {}", row.label())
    };
    if is_active && show_sources {
        text.push_str(" [");
        text.push_str(&source_label(active_sources));
        text.push(']');
    }
    builder.text(text, style.clone());
}

fn row_active(row: &CameraGuidanceRow, sources: CameraInteractionSources) -> bool {
    if sources.is_empty() {
        return false;
    }
    row.sources()
        .is_none_or(|filter| sources.intersects(filter))
}

fn source_label(sources: CameraInteractionSources) -> String {
    let mut labels = Vec::new();
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::MOUSE,
        "mouse",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::WHEEL,
        "wheel",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::SMOOTH_SCROLL,
        "smooth-scroll",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::PINCH,
        "pinch",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::TOUCH,
        "touch",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::KEYBOARD,
        "keyboard",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::GAMEPAD,
        "gamepad",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::MANUAL,
        "manual",
    );

    if labels.is_empty() {
        "idle".to_string()
    } else {
        labels.join(" + ")
    }
}

fn push_source_label(
    labels: &mut Vec<&'static str>,
    sources: CameraInteractionSources,
    source: CameraInteractionSources,
    label: &'static str,
) {
    if sources.contains(source) {
        labels.push(label);
    }
}
