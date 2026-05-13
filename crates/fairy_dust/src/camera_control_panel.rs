//! Capability: screen-space `bevy_diegetic` camera control panels for
//! `bevy_lagrange::OrbitCam` examples.

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_diegetic::AlignX;
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
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::default_panel_material;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamControlRow;
use bevy_lagrange::OrbitCamControlSummary;
use bevy_lagrange::OrbitCamInteractionEnded;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionSourcesChanged;
use bevy_lagrange::OrbitCamInteractionStarted;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::OrbitCamManual;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::describe_orbit_cam_controls;

use crate::ensure_plugin;

/// Data-driven camera control metadata shown by [`SprinkleBuilder`](crate::SprinkleBuilder)
/// examples.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct CameraGuidance {
    anchor:       Anchor,
    title:        Option<String>,
    mode_label:   Option<String>,
    mode_value:   Option<String>,
    content:      CameraGuidanceContent,
    show_sources: bool,
}

impl Default for CameraGuidance {
    fn default() -> Self { Self::auto() }
}

impl CameraGuidance {
    /// Builds guidance rows from the camera's actual input-mode components.
    #[must_use]
    pub const fn auto() -> Self {
        Self {
            anchor:       Anchor::BottomRight,
            title:        None,
            mode_label:   None,
            mode_value:   None,
            content:      CameraGuidanceContent::Auto,
            show_sources: true,
        }
    }

    /// Builds guidance rows for a built-in orbit-camera preset.
    #[must_use]
    pub fn for_preset(preset: OrbitCamPreset) -> Self {
        Self::from_summary(describe_orbit_cam_controls(Some(&preset), None, None))
    }

    /// Builds custom camera guidance rows.
    #[must_use]
    pub fn custom(rows: impl IntoIterator<Item = CameraGuidanceRow>) -> Self {
        Self {
            anchor:       Anchor::BottomRight,
            title:        None,
            mode_label:   None,
            mode_value:   None,
            content:      CameraGuidanceContent::Rows(rows.into_iter().collect()),
            show_sources: true,
        }
    }

    /// Sets the panel screen anchor.
    #[must_use]
    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Replaces the panel title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Controls whether active source labels are rendered.
    #[must_use]
    pub const fn with_source_flags(mut self, show_sources: bool) -> Self {
        self.show_sources = show_sources;
        self
    }

    /// Returns explicitly configured rows.
    ///
    /// Auto guidance is resolved when the panel binds to a camera.
    #[must_use]
    pub fn rows(&self) -> &[CameraGuidanceRow] {
        match &self.content {
            CameraGuidanceContent::Auto => &[],
            CameraGuidanceContent::Rows(rows) => rows,
        }
    }

    fn from_summary(summary: OrbitCamControlSummary) -> Self {
        Self {
            anchor:       Anchor::BottomRight,
            title:        Some(summary.camera_label),
            mode_label:   Some(summary.mode_label),
            mode_value:   Some(summary.mode_value),
            content:      CameraGuidanceContent::Rows(
                summary.rows.into_iter().map(Into::into).collect(),
            ),
            show_sources: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CameraGuidanceContent {
    Auto,
    Rows(Vec<CameraGuidanceRow>),
}

/// A single camera guidance row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraGuidanceRow {
    kind:                       OrbitCamInteractionKind,
    label:                      String,
    camera_interaction_sources: CameraInteractionSources,
}

impl CameraGuidanceRow {
    /// Creates a row for an interaction kind.
    #[must_use]
    pub fn new(kind: OrbitCamInteractionKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            camera_interaction_sources: CameraInteractionSources::NONE,
        }
    }

    /// Highlights this row only when the active sources intersect `sources`.
    #[must_use]
    pub const fn with_camera_interaction_sources(
        mut self,
        camera_interaction_sources: CameraInteractionSources,
    ) -> Self {
        self.camera_interaction_sources = camera_interaction_sources;
        self
    }

    /// Returns the interaction kind matched by this row.
    #[must_use]
    pub const fn kind(&self) -> OrbitCamInteractionKind { self.kind }

    /// Returns this row's camera-interaction source metadata.
    #[must_use]
    pub const fn camera_interaction_sources(&self) -> CameraInteractionSources {
        self.camera_interaction_sources
    }

    /// Returns the display label.
    #[must_use]
    pub fn label(&self) -> &str { &self.label }
}

impl From<OrbitCamControlRow> for CameraGuidanceRow {
    fn from(row: OrbitCamControlRow) -> Self {
        Self::new(row.kind, row.label)
            .with_camera_interaction_sources(row.camera_interaction_sources)
    }
}

#[derive(Component)]
struct CameraGuidancePanel {
    camera: Entity,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct CameraGuidanceSnapshot {
    camera_label: String,
    mode_label:   String,
    mode_value:   String,
    rows:         Vec<CameraGuidanceRow>,
    show_sources: bool,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct CameraGuidanceDisplayState {
    orbit:        CameraGuidanceDisplaySlot,
    pan:          CameraGuidanceDisplaySlot,
    zoom:         CameraGuidanceDisplaySlot,
    needs_render: bool,
}

impl Default for CameraGuidanceDisplayState {
    fn default() -> Self { Self::from_display(CameraGuidanceDisplay::default()) }
}

impl CameraGuidanceDisplayState {
    const fn from_display(display: CameraGuidanceDisplay) -> Self {
        Self {
            orbit:        CameraGuidanceDisplaySlot::active(display.orbit),
            pan:          CameraGuidanceDisplaySlot::active(display.pan),
            zoom:         CameraGuidanceDisplaySlot::active(display.zoom),
            needs_render: false,
        }
    }

    const fn display(self) -> CameraGuidanceDisplay {
        CameraGuidanceDisplay {
            orbit: self.orbit.sources(),
            pan:   self.pan.sources(),
            zoom:  self.zoom.sources(),
        }
    }

    fn activate(
        &mut self,
        kind: OrbitCamInteractionKind,
        sources: CameraInteractionSources,
        now: f32,
    ) {
        let Some(slot) = self.slot_mut(kind) else {
            return;
        };
        let changed = slot.activate(sources, now);
        if changed {
            self.needs_render = true;
        }
    }

    fn hold(&mut self, kind: OrbitCamInteractionKind, sources: CameraInteractionSources, now: f32) {
        let Some(slot) = self.slot_mut(kind) else {
            return;
        };
        let changed = slot.hold(sources, now);
        if changed {
            self.needs_render = true;
        }
    }

    fn expire_held_sources(&mut self, now: f32) {
        let expired = self.orbit.expire(now) | self.pan.expire(now) | self.zoom.expire(now);
        if expired {
            self.needs_render = true;
        }
    }

    const fn slot_mut(
        &mut self,
        kind: OrbitCamInteractionKind,
    ) -> Option<&mut CameraGuidanceDisplaySlot> {
        match kind {
            OrbitCamInteractionKind::Orbit => Some(&mut self.orbit),
            OrbitCamInteractionKind::Pan => Some(&mut self.pan),
            OrbitCamInteractionKind::Zoom => Some(&mut self.zoom),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraGuidanceDisplaySlot {
    active_sources: CameraInteractionSources,
    held_sources:   CameraInteractionSources,
    held_until:     Option<f32>,
}

impl CameraGuidanceDisplaySlot {
    const fn active(sources: CameraInteractionSources) -> Self {
        Self {
            active_sources: sources,
            held_sources:   CameraInteractionSources::NONE,
            held_until:     None,
        }
    }

    const fn sources(self) -> CameraInteractionSources {
        self.active_sources.union(self.held_sources)
    }

    fn activate(&mut self, sources: CameraInteractionSources, now: f32) -> bool {
        let before = self.sources();
        let inactive_sources = self.active_sources.difference(sources);

        self.active_sources = sources;
        self.held_sources = self
            .held_sources
            .union(inactive_sources)
            .difference(sources);
        if !inactive_sources.is_empty() {
            self.held_until = Some(now + SOURCE_HOLD_SECONDS);
        }
        if self.held_sources.is_empty() {
            self.held_until = None;
        }

        before != self.sources()
    }

    fn hold(&mut self, sources: CameraInteractionSources, now: f32) -> bool {
        let before = self.sources();

        self.active_sources = self.active_sources.difference(sources);
        self.held_sources = self.held_sources.union(sources);
        if !sources.is_empty() {
            self.held_until = Some(now + SOURCE_HOLD_SECONDS);
        }

        before != self.sources()
    }

    fn expire(&mut self, now: f32) -> bool {
        if self.held_until.is_none_or(|held_until| now < held_until) {
            return false;
        }

        self.held_until = None;
        if self.held_sources.is_empty() {
            return false;
        }

        self.held_sources = CameraInteractionSources::NONE;
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CameraGuidanceDisplay {
    orbit: CameraInteractionSources,
    pan:   CameraInteractionSources,
    zoom:  CameraInteractionSources,
}

impl CameraGuidanceDisplay {
    const fn from_interaction_state(state: OrbitCamInteractionState) -> Self {
        Self {
            orbit: state.orbit_sources(),
            pan:   state.pan_sources(),
            zoom:  state.zoom_sources(),
        }
    }

    const fn sources(self, kind: OrbitCamInteractionKind) -> CameraInteractionSources {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit,
            OrbitCamInteractionKind::Pan => self.pan,
            OrbitCamInteractionKind::Zoom => self.zoom,
            _ => CameraInteractionSources::NONE,
        }
    }

    const fn all_sources(self) -> CameraInteractionSources {
        self.orbit.union(self.pan).union(self.zoom)
    }
}

const RADIUS: Px = Px(12.0);
const FRAME_PAD: Px = Px(2.0);
const BORDER: Px = Px(2.0);
const INSET: Px = Px(FRAME_PAD.0 + BORDER.0);
const INNER_RADIUS: Px = Px(RADIUS.0 - INSET.0);

const TITLE_SIZE: Pt = Pt(14.0);
const HEADER_SIZE: Pt = Pt(12.0);
const LABEL_SIZE: Pt = Pt(11.0);

const INNER_BG: Color = Color::srgba(0.02, 0.03, 0.07, 0.50);
const BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HEADER_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
const LABEL_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);
const ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
const SOURCE_COLOR: Color = Color::srgba(0.35, 0.8, 1.0, 0.95);

const SOURCE_HOLD_SECONDS: f32 = 0.15;
const TABLE_COLUMN_GAP: f32 = 8.0;
const TABLE_ROW_GAP: f32 = 3.0;
const TABLE_GROUP_GAP: f32 = 7.0;
const TABLE_DIVIDER_WIDTH: Px = Px(1.0);
const ACTION_COLUMN_MIN_WIDTH: Px = Px(46.0);

pub(crate) fn install(app: &mut App) {
    ensure_panel_plugins(app);
    app.add_systems(
        PostUpdate,
        (refresh_changed_guidance_snapshot, refresh_guidance_display),
    );
    app.add_observer(attach_default_guidance_on_orbit_cam_add)
        .add_observer(spawn_guidance_panel_on_add)
        .add_observer(refresh_on_interaction_started)
        .add_observer(refresh_on_interaction_ended)
        .add_observer(refresh_on_sources_changed);
}

fn ensure_panel_plugins(app: &mut App) {
    ensure_plugin(app, DiegeticUiPlugin);
    ensure_plugin(app, MeshPickingPlugin);
}

fn attach_default_guidance_on_orbit_cam_add(
    trigger: On<Add, OrbitCam>,
    mut commands: Commands,
    cameras: Query<(), (With<OrbitCam>, Without<CameraGuidance>)>,
) {
    let camera = trigger.entity;
    if cameras.get(camera).is_ok() {
        commands.entity(camera).insert(CameraGuidance::auto());
    }
}

fn spawn_guidance_panel_on_add(
    trigger: On<Add, CameraGuidance>,
    mut commands: Commands,
    cameras: Query<(
        &CameraGuidance,
        Option<&OrbitCamInteractionState>,
        Option<&OrbitCamPreset>,
        Option<&OrbitCamBindings>,
        Option<&OrbitCamManual>,
    )>,
) {
    let camera = trigger.entity;
    let Ok((guidance, state, preset, bindings, manual)) = cameras.get(camera) else {
        return;
    };
    let snapshot = resolve_guidance_snapshot(guidance, preset, bindings, manual);
    let display = CameraGuidanceDisplay::from_interaction_state(state.copied().unwrap_or_default());
    let unlit = unlit_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(guidance.anchor)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_guidance_tree(&snapshot, display))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((
                CameraGuidancePanel { camera },
                snapshot,
                CameraGuidanceDisplayState::from_display(display),
                panel,
                Transform::default(),
            ));
        },
        Err(error) => {
            error!("fairy_dust: failed to build camera control panel: {error}");
        },
    }
}

fn refresh_changed_guidance_snapshot(
    mut commands: Commands,
    cameras: Query<
        (
            Entity,
            &CameraGuidance,
            Option<&OrbitCamInteractionState>,
            Option<&OrbitCamPreset>,
            Option<&OrbitCamBindings>,
            Option<&OrbitCamManual>,
        ),
        Or<(
            Changed<CameraGuidance>,
            Changed<OrbitCamPreset>,
            Changed<OrbitCamBindings>,
            Changed<OrbitCamManual>,
        )>,
    >,
    mut panels: Query<(
        Entity,
        &CameraGuidancePanel,
        &mut CameraGuidanceDisplayState,
    )>,
) {
    for (camera, guidance, state, preset, bindings, manual) in &cameras {
        let snapshot = resolve_guidance_snapshot(guidance, preset, bindings, manual);
        let display =
            CameraGuidanceDisplay::from_interaction_state(state.copied().unwrap_or_default());
        refresh_camera_guidance_snapshot(camera, snapshot, display, &mut commands, &mut panels);
    }
}

fn refresh_on_interaction_started(
    event: On<OrbitCamInteractionStarted>,
    time: Res<Time<Real>>,
    mut panels: Query<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    update_camera_guidance_display(event.camera, &mut panels, |display| {
        display.activate(event.kind, event.sources, time.elapsed_secs());
    });
}

fn refresh_on_interaction_ended(
    event: On<OrbitCamInteractionEnded>,
    time: Res<Time<Real>>,
    mut panels: Query<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    update_camera_guidance_display(event.camera, &mut panels, |display| {
        display.hold(event.kind, event.sources, time.elapsed_secs());
    });
}

fn refresh_on_sources_changed(
    event: On<OrbitCamInteractionSourcesChanged>,
    time: Res<Time<Real>>,
    mut panels: Query<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    update_camera_guidance_display(event.camera, &mut panels, |display| {
        display.activate(event.kind, event.current, time.elapsed_secs());
    });
}

fn update_camera_guidance_display(
    camera: Entity,
    panels: &mut Query<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
    update: impl Fn(&mut CameraGuidanceDisplayState),
) {
    panels
        .iter_mut()
        .filter(|(panel_camera, _)| panel_camera.camera == camera)
        .for_each(|(_, mut display)| update(&mut display));
}

fn refresh_camera_guidance_snapshot(
    camera: Entity,
    snapshot: CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
    commands: &mut Commands,
    panels: &mut Query<(
        Entity,
        &CameraGuidancePanel,
        &mut CameraGuidanceDisplayState,
    )>,
) {
    for (panel, panel_camera, mut display_state) in panels.iter_mut() {
        if panel_camera.camera == camera {
            commands.entity(panel).insert(snapshot.clone());
            *display_state = CameraGuidanceDisplayState::from_display(display);
            commands.set_tree(panel, build_guidance_tree(&snapshot, display));
        }
    }
}

fn refresh_guidance_display(
    time: Res<Time<Real>>,
    mut commands: Commands,
    mut panels: Query<(
        Entity,
        &CameraGuidanceSnapshot,
        &mut CameraGuidanceDisplayState,
    )>,
) {
    for (panel, snapshot, mut display) in &mut panels {
        display.expire_held_sources(time.elapsed_secs());
        if !display.needs_render {
            continue;
        }

        commands.set_tree(panel, build_guidance_tree(snapshot, display.display()));
        display.needs_render = false;
    }
}

fn unlit_panel_material() -> StandardMaterial {
    StandardMaterial {
        unlit: true,
        ..default_panel_material()
    }
}

fn build_guidance_tree(
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_guidance_layout(&mut builder, snapshot, display);
    builder.build()
}

fn build_guidance_layout(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
) {
    let title = LayoutTextStyle::new(TITLE_SIZE)
        .with_color(TITLE_COLOR)
        .no_wrap();
    let header = LayoutTextStyle::new(HEADER_SIZE)
        .with_color(HEADER_COLOR)
        .no_wrap();
    let label = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(LABEL_COLOR)
        .no_wrap();
    let active = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(ACTIVE_COLOR)
        .no_wrap();
    let source = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(SOURCE_COLOR)
        .no_wrap();

    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(FRAME_PAD))
            .corner_radius(CornerRadius::all(RADIUS))
            .border(Border::all(BORDER, BORDER_ACCENT)),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(Px(10.0)))
                    .child_gap(Px(5.0))
                    .corner_radius(CornerRadius::all(INNER_RADIUS))
                    .background(INNER_BG)
                    .border(Border::all(Px(1.0), BORDER_DIM)),
                |builder| {
                    builder.text(format!("CAMERA: {}", snapshot.camera_label), title.clone());
                    builder.text(
                        format!("{}: {}", snapshot.mode_label, snapshot.mode_value),
                        header.clone(),
                    );
                    build_guidance_table(builder, snapshot, display, &label, &active);
                    if snapshot.show_sources {
                        builder.with(
                            El::new()
                                .width(Sizing::GROW)
                                .height(Sizing::FIT)
                                .child_align_x(AlignX::Center),
                            |builder| {
                                builder.text(source_label(display.all_sources()), source);
                            },
                        );
                    }
                },
            );
        },
    );
}

fn build_guidance_table(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(Px(TABLE_GROUP_GAP))
            .border(
                Border::new()
                    .between_children(TABLE_DIVIDER_WIDTH)
                    .color(BORDER_DIM),
            ),
        |builder| {
            for kind in [
                OrbitCamInteractionKind::Orbit,
                OrbitCamInteractionKind::Pan,
                OrbitCamInteractionKind::Zoom,
            ] {
                build_guidance_group(builder, snapshot, kind, display, label, active);
            }
        },
    );
}

fn build_guidance_group(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    kind: OrbitCamInteractionKind,
    display: CameraGuidanceDisplay,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
    let active_sources = display.sources(kind);
    let rows = snapshot
        .rows
        .iter()
        .filter(|row| row.kind() == kind)
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }

    let group_active = rows.iter().any(|row| row_active(row, active_sources));
    let action_style = if group_active { active } else { label };

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(Px(TABLE_COLUMN_GAP))
            .child_align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .child_gap(Px(TABLE_ROW_GAP)),
                |builder| {
                    for row in rows {
                        let binding_style = if row_active(row, active_sources) {
                            active
                        } else {
                            label
                        };
                        builder.text(row.label(), binding_style.clone());
                    }
                },
            );
            builder.text("->", action_style.clone());
            builder.with(
                El::new()
                    .width(Sizing::fit_min(ACTION_COLUMN_MIN_WIDTH))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(kind_label(kind), action_style.clone());
                },
            );
        },
    );
}

fn resolve_guidance_snapshot(
    guidance: &CameraGuidance,
    preset: Option<&OrbitCamPreset>,
    bindings: Option<&OrbitCamBindings>,
    manual: Option<&OrbitCamManual>,
) -> CameraGuidanceSnapshot {
    match &guidance.content {
        CameraGuidanceContent::Auto => {
            let summary = describe_orbit_cam_controls(preset, bindings, manual);
            snapshot_from_summary(guidance, summary)
        },
        CameraGuidanceContent::Rows(rows) => {
            let (mode_label, mode_value) = resolve_mode_labels(preset, bindings, manual);
            CameraGuidanceSnapshot {
                camera_label: guidance
                    .title
                    .clone()
                    .unwrap_or_else(|| "OrbitCam".to_string()),
                mode_label:   guidance.mode_label.clone().unwrap_or(mode_label),
                mode_value:   guidance.mode_value.clone().unwrap_or(mode_value),
                rows:         rows.clone(),
                show_sources: guidance.show_sources,
            }
        },
    }
}

fn snapshot_from_summary(
    guidance: &CameraGuidance,
    summary: OrbitCamControlSummary,
) -> CameraGuidanceSnapshot {
    CameraGuidanceSnapshot {
        camera_label: guidance.title.clone().unwrap_or(summary.camera_label),
        mode_label:   summary.mode_label,
        mode_value:   summary.mode_value,
        rows:         summary.rows.into_iter().map(Into::into).collect(),
        show_sources: guidance.show_sources,
    }
}

fn resolve_mode_labels(
    preset: Option<&OrbitCamPreset>,
    bindings: Option<&OrbitCamBindings>,
    manual: Option<&OrbitCamManual>,
) -> (String, String) {
    if manual.is_some() {
        return ("Input".to_string(), "Manual".to_string());
    }
    if bindings.is_some() {
        return ("Bindings".to_string(), "Custom".to_string());
    }
    let preset = preset.copied().unwrap_or_default();
    ("Preset".to_string(), preset_mode_value(preset).to_string())
}

const fn preset_mode_value(preset: OrbitCamPreset) -> &'static str {
    match preset {
        OrbitCamPreset::SimpleMouse => "SimpleMouse",
        OrbitCamPreset::BlenderLike => "BlenderLike",
        _ => "Custom",
    }
}

const fn row_active(row: &CameraGuidanceRow, sources: CameraInteractionSources) -> bool {
    if sources.is_empty() {
        return false;
    }
    sources.intersects(row.camera_interaction_sources())
}

fn source_label(sources: CameraInteractionSources) -> String {
    let mut labels = Vec::new();
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::MOUSE,
        "button-drag",
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

const fn kind_label(kind: OrbitCamInteractionKind) -> &'static str {
    match kind {
        OrbitCamInteractionKind::Orbit => "Orbit",
        OrbitCamInteractionKind::Pan => "Pan",
        OrbitCamInteractionKind::Zoom => "Zoom",
        _ => "",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ended_source_is_held_until_expiry() {
        let mut display = CameraGuidanceDisplayState::default();

        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.0,
        );
        assert!(display.needs_render);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::SMOOTH_SCROLL
        );

        display.needs_render = false;
        display.hold(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.0,
        );
        assert!(!display.needs_render);

        display.expire_held_sources(1.14);
        assert!(!display.needs_render);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::SMOOTH_SCROLL
        );

        display.expire_held_sources(1.15);
        assert!(display.needs_render);
        assert!(display.display().orbit.is_empty());
    }

    #[test]
    fn repeated_scroll_edges_do_not_request_rebuilds_before_expiry() {
        let mut display = CameraGuidanceDisplayState::default();

        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.0,
        );
        display.needs_render = false;
        display.hold(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.0,
        );
        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.05,
        );
        display.hold(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.1,
        );

        assert!(!display.needs_render);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::SMOOTH_SCROLL
        );
    }

    #[test]
    fn alternating_sources_hold_union_until_expiry() {
        let mut display = CameraGuidanceDisplayState::default();

        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::MOUSE,
            1.0,
        );
        display.needs_render = false;

        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.05,
        );
        assert!(display.needs_render);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::MOUSE.union(CameraInteractionSources::SMOOTH_SCROLL)
        );

        display.needs_render = false;
        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::MOUSE,
            1.1,
        );
        assert!(!display.needs_render);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::MOUSE.union(CameraInteractionSources::SMOOTH_SCROLL)
        );

        display.expire_held_sources(1.24);
        assert!(!display.needs_render);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::MOUSE.union(CameraInteractionSources::SMOOTH_SCROLL)
        );

        display.expire_held_sources(1.25);
        assert!(display.needs_render);
        assert_eq!(display.display().orbit, CameraInteractionSources::MOUSE);
    }

    #[test]
    fn source_label_lists_sources_without_brackets() {
        let sources = CameraInteractionSources::MOUSE.union(CameraInteractionSources::PINCH);

        assert_eq!(source_label(sources), "button-drag + pinch");
        assert_eq!(source_label(CameraInteractionSources::NONE), "idle");
    }
}
