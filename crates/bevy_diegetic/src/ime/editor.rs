//! Screen-space editor rendering and anchoring for active IME sessions.

use bevy::math::Rect;
use bevy::prelude::*;
use bevy::window::WindowRef;
use bevy_kana::ToF32;

use super::ActiveImeSession;
use super::ImeApplied;
use super::ImeBufferSnapshot;
use super::ImeCancelCause;
use super::ImeCanceled;
use super::ImeCommitCause;
use super::ImeCursorState;
use super::ImePreedit;
use super::ImePreeditBoundary;
use super::ImeRequestCancel;
use super::ImeRequestCommit;
use super::ImeSelectionSnapshot;
use super::ImeSessionAnchor;
use super::ImeSessionId;
use super::ImeTarget;
use super::ImeTextChanged;
use super::ImeValidationRejected;
use crate::AlignX;
use crate::AlignY;
use crate::Anchor;
use crate::Border;
use crate::BoundingBox;
use crate::ComputedDiegeticPanel;
use crate::DiegeticPanel;
use crate::DiegeticPanelCommands;
use crate::Direction;
use crate::El;
use crate::LayoutBuilder;
use crate::LayoutTextStyle;
use crate::LayoutTree;
use crate::Padding;
use crate::PanelFieldId;
use crate::PanelFieldRecord;
use crate::Px;
use crate::Sizing;

const EDITOR_CAMERA_ORDER: isize = 120;
const DEFAULT_EDITOR_WIDTH: f32 = 180.0;
const DEFAULT_EDITOR_HEIGHT: f32 = 34.0;
const MIN_EDITOR_WIDTH: f32 = 72.0;
const MAX_EDITOR_WIDTH: f32 = 420.0;
const EDITOR_EXTRA_WIDTH: f32 = 18.0;
const EDITOR_FONT_SIZE: f32 = 16.0;
const EDITOR_PADDING_X: f32 = 8.0;
const EDITOR_PADDING_Y: f32 = 5.0;
const EDITOR_GAP: f32 = 3.0;
const CARET_WIDTH: f32 = 2.0;
const CARET_HEIGHT: f32 = 20.0;
const EMPTY_TEXT_ADVANCE: f32 = 8.0;
const EDITOR_BORDER_WIDTH: f32 = 1.0;
const EDITOR_CORNER_RADIUS: f32 = 5.0;
const SOURCE_RECT_MIN_AXIS: f32 = 1.0;

const EDITOR_BACKGROUND: Color = Color::srgba(0.025, 0.028, 0.034, 0.96);
const EDITOR_BORDER: Color = Color::srgba(0.42, 0.72, 0.86, 0.92);
const EDITOR_TEXT: Color = Color::srgb(0.92, 0.94, 0.96);
const EDITOR_PREEDIT: Color = Color::srgb(0.70, 0.86, 1.0);
const EDITOR_SELECTION: Color = Color::srgba(0.18, 0.45, 0.64, 0.82);
const EDITOR_VALIDATION: Color = Color::srgb(1.0, 0.48, 0.40);
const EDITOR_CARET: Color = Color::srgb(0.86, 0.96, 1.0);

/// Field projection captured from panel picking before the session id exists.
#[derive(Resource, Clone, Debug, Default)]
pub(super) struct PendingImePanelAnchor {
    pending: Option<ImePanelAnchorSource>,
}

impl PendingImePanelAnchor {
    pub(super) fn store(
        &mut self,
        panel: Entity,
        field_id: PanelFieldId,
        camera: Entity,
        window: Entity,
    ) {
        self.pending = Some(ImePanelAnchorSource {
            panel,
            field_id,
            camera,
            window,
        });
    }

    fn take_for(&mut self, target: &ImeTarget, window: Entity) -> Option<ImePanelAnchorSource> {
        let pending = self.pending.as_ref()?;
        if pending.window != window || !pending.matches_target(target) {
            return None;
        }
        self.pending.take()
    }
}

#[derive(Clone, Debug)]
struct ImePanelAnchorSource {
    panel:    Entity,
    field_id: PanelFieldId,
    camera:   Entity,
    window:   Entity,
}

impl ImePanelAnchorSource {
    fn matches_target(&self, target: &ImeTarget) -> bool {
        match target {
            ImeTarget::WorldPanelField { panel, field_id }
            | ImeTarget::ScreenPanelField { panel, field_id } => {
                self.panel == *panel && self.field_id == *field_id
            },
            ImeTarget::AppOwned { .. } => false,
        }
    }
}

/// Active screen-space editor state.
#[derive(Resource, Debug, Default)]
pub(super) struct ImeEditorState {
    active: Option<ImeEditor>,
}

impl ImeEditorState {
    const fn active(&self) -> Option<&ImeEditor> { self.active.as_ref() }

    const fn active_mut(&mut self) -> Option<&mut ImeEditor> { self.active.as_mut() }

    fn session_id(&self) -> Option<ImeSessionId> {
        self.active.as_ref().map(|editor| editor.session_id)
    }

    fn is_editor_panel(&self, entity: Entity) -> bool {
        self.active
            .as_ref()
            .is_some_and(|editor| editor.panel == entity)
    }

    fn clear(&mut self, commands: &mut Commands) {
        if let Some(editor) = self.active.take() {
            commands.entity(editor.panel).despawn();
        }
    }
}

#[derive(Debug)]
struct ImeEditor {
    session_id: ImeSessionId,
    target:     ImeTarget,
    window:     Entity,
    snapshot:   ImeBufferSnapshot,
    validation: Option<String>,
    panel:      Entity,
    source:     Option<ImePanelAnchorSource>,
    app_anchor: Option<ImeSessionAnchor>,
    anchor:     Option<ImeEditorAnchor>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ImeEditorAnchor {
    screen_rect: Rect,
    editor_pos:  Vec2,
    editor_size: Vec2,
    caret_pos:   Vec2,
}

/// Last panel click that was classified as outside the active editor.
#[derive(Resource, Debug, Default)]
pub(super) struct ImeBlurIntent {
    latest: Option<ImeBlurClassification>,
}

impl ImeBlurIntent {
    const fn set(&mut self, session_id: ImeSessionId, clicked_panel: Entity, target: &ImeTarget) {
        self.latest = Some(ImeBlurClassification {
            session_id,
            clicked_panel,
            source_panel: source_panel(target),
        });
    }

    fn clear_session(&mut self, session_id: ImeSessionId) {
        if self
            .latest
            .as_ref()
            .is_some_and(|intent| intent.session_id == session_id)
        {
            self.latest = None;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImeBlurClassification {
    session_id:    ImeSessionId,
    clicked_panel: Entity,
    source_panel:  Option<Entity>,
}

/// Marker on the transient editor panel.
#[derive(Component, Debug)]
struct ImeEditorPanel;

pub(super) fn observe_panel_clicks(trigger: On<Add, DiegeticPanel>, mut commands: Commands) {
    commands
        .entity(trigger.event_target())
        .observe(classify_panel_click);
}

pub(super) fn update_editor_from_text_changed(
    event: On<ImeTextChanged>,
    active_session: Res<ActiveImeSession>,
    mut pending_anchor: ResMut<PendingImePanelAnchor>,
    mut editor_state: ResMut<ImeEditorState>,
    mut blur_intent: ResMut<ImeBlurIntent>,
    mut commands: Commands,
) {
    let event = event.event();
    let Some(window) = active_session.active_window() else {
        return;
    };
    let source = pending_anchor.take_for(&event.target, window);
    let app_anchor = active_session.active_anchor();

    let needs_spawn = editor_state
        .active()
        .is_none_or(|editor| editor.session_id != event.session_id);
    if needs_spawn {
        editor_state.clear(&mut commands);
        let Some(panel) = spawn_editor_panel(window, &event.snapshot, None, &mut commands) else {
            return;
        };
        editor_state.active = Some(ImeEditor {
            session_id: event.session_id,
            target: event.target.clone(),
            window,
            snapshot: event.snapshot.clone(),
            validation: None,
            panel,
            source,
            app_anchor,
            anchor: None,
        });
    } else if let Some(editor) = editor_state.active_mut() {
        if source.is_some() {
            editor.source = source;
        }
        editor.target = event.target.clone();
        editor.window = window;
        editor.snapshot = event.snapshot.clone();
        editor.app_anchor = app_anchor;
        editor.validation = None;
    }

    blur_intent.clear_session(event.session_id);
    if let Some(editor) = editor_state.active() {
        commands.set_tree(
            editor.panel,
            editor_tree(&editor.snapshot, editor.validation.as_deref()),
        );
    }
}

pub(super) fn update_editor_validation(
    event: On<ImeValidationRejected>,
    mut editor_state: ResMut<ImeEditorState>,
    mut commands: Commands,
) {
    let event = event.event();
    let Some(editor) = editor_state.active_mut() else {
        return;
    };
    if editor.session_id != event.session_id {
        return;
    }

    editor.validation = Some(format!("{:?}", event.reason));
    commands.set_tree(
        editor.panel,
        editor_tree(&editor.snapshot, editor.validation.as_deref()),
    );
}

pub(super) fn close_editor_on_cancel(
    event: On<ImeCanceled>,
    mut editor_state: ResMut<ImeEditorState>,
    mut blur_intent: ResMut<ImeBlurIntent>,
    mut commands: Commands,
) {
    let session_id = event.event().session_id;
    if editor_state.session_id() != Some(session_id) {
        return;
    }
    blur_intent.clear_session(session_id);
    editor_state.clear(&mut commands);
}

pub(super) fn close_editor_on_apply(
    event: On<ImeApplied>,
    mut editor_state: ResMut<ImeEditorState>,
    mut blur_intent: ResMut<ImeBlurIntent>,
    mut commands: Commands,
) {
    let session_id = event.event().session_id;
    if editor_state.session_id() != Some(session_id) {
        return;
    }
    blur_intent.clear_session(session_id);
    editor_state.clear(&mut commands);
}

pub(super) fn update_editor_anchor(
    mut editor_state: ResMut<ImeEditorState>,
    mut panel_queries: ParamSet<(
        Query<&mut DiegeticPanel>,
        Query<(&DiegeticPanel, &ComputedDiegeticPanel, &GlobalTransform)>,
    )>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window>,
    mut commands: Commands,
) {
    let Some(editor) = editor_state.active_mut() else {
        return;
    };
    let Ok(window) = windows.get(editor.window) else {
        return;
    };

    let screen_rect = {
        let source_panels = panel_queries.p1();
        target_screen_rect(editor, &source_panels, &cameras, window)
    };
    let Some(screen_rect) = screen_rect else {
        commands.trigger(ImeRequestCancel {
            session_id: editor.session_id,
            cause:      ImeCancelCause::TargetStale,
        });
        return;
    };

    let editor_size = editor_size(screen_rect);
    let editor_pos = clamp_editor_position(screen_rect.min, editor_size, window);
    let caret_pos = caret_position(editor_pos, editor_size, &editor.snapshot);
    editor.anchor = Some(ImeEditorAnchor {
        screen_rect,
        editor_pos,
        editor_size,
        caret_pos,
    });

    let mut panels = panel_queries.p0();
    let Ok(mut panel) = panels.get_mut(editor.panel) else {
        return;
    };
    let _ = panel.set_size((Px(editor_size.x), Px(editor_size.y)));
    panel.set_screen_position(editor_pos);
}

pub(super) fn update_window_ime_position(
    editor_state: Res<ImeEditorState>,
    mut windows: Query<&mut Window>,
) {
    let Some(editor) = editor_state.active() else {
        return;
    };
    let Some(anchor) = editor.anchor else {
        return;
    };
    let Ok(mut window) = windows.get_mut(editor.window) else {
        return;
    };
    window.ime_position = anchor.caret_pos;
}

pub(super) fn handle_blur_intent(
    mut blur_intent: ResMut<ImeBlurIntent>,
    active_session: Res<ActiveImeSession>,
    mut commands: Commands,
) {
    let Some(intent) = blur_intent.latest.take() else {
        return;
    };
    if active_session.active_session_id() != Some(intent.session_id) {
        return;
    }
    if active_session.is_pending_commit() {
        return;
    }

    let Some(target) = active_session.active_target() else {
        return;
    };
    if intent.is_inside_focus_scope(target) {
        return;
    }

    commands.trigger(ImeRequestCommit {
        session_id: intent.session_id,
        cause:      ImeCommitCause::Blur,
    });
}

fn classify_panel_click(
    mut click: On<Pointer<Click>>,
    editor_state: Res<ImeEditorState>,
    mut blur_intent: ResMut<ImeBlurIntent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let Some(session_id) = editor_state.session_id() else {
        return;
    };
    let clicked_panel = click.event_target();
    if editor_state.is_editor_panel(clicked_panel) {
        click.propagate(false);
        return;
    }

    let Some(editor) = editor_state.active() else {
        return;
    };
    blur_intent.set(session_id, clicked_panel, &editor.target);
    click.propagate(false);
}

fn spawn_editor_panel(
    window: Entity,
    snapshot: &ImeBufferSnapshot,
    validation: Option<&str>,
    commands: &mut Commands,
) -> Option<Entity> {
    let panel = match DiegeticPanel::screen()
        .size(Px(DEFAULT_EDITOR_WIDTH), Px(DEFAULT_EDITOR_HEIGHT))
        .anchor(Anchor::TopLeft)
        .screen_position(0.0, 0.0)
        .camera_order(EDITOR_CAMERA_ORDER)
        .window(WindowRef::Entity(window))
        .with_tree(editor_tree(snapshot, validation))
        .build()
    {
        Ok(panel) => panel,
        Err(error) => {
            bevy::log::error!(
                target: "bevy_diegetic::ime",
                "failed to build IME editor panel: {error:?}"
            );
            return None;
        },
    };

    Some(
        commands
            .spawn((ImeEditorPanel, panel, Transform::default()))
            .id(),
    )
}

fn target_screen_rect(
    editor: &ImeEditor,
    panels: &Query<(&DiegeticPanel, &ComputedDiegeticPanel, &GlobalTransform)>,
    cameras: &Query<(&Camera, &GlobalTransform)>,
    window: &Window,
) -> Option<Rect> {
    match editor.target {
        ImeTarget::WorldPanelField {
            panel,
            ref field_id,
        }
        | ImeTarget::ScreenPanelField {
            panel,
            ref field_id,
        } => {
            let (panel, computed, panel_transform) = panels.get(panel).ok()?;
            let record = field_record(computed, field_id)?;
            let source = editor.source.as_ref()?;
            let (camera, camera_transform) = cameras.get(source.camera).ok()?;
            project_field_record(record, panel, panel_transform, camera, camera_transform)
        },
        ImeTarget::AppOwned { .. } => Some(app_anchor_rect(editor.app_anchor, window)),
    }
}

const fn source_panel(target: &ImeTarget) -> Option<Entity> {
    match *target {
        ImeTarget::WorldPanelField { panel, .. } | ImeTarget::ScreenPanelField { panel, .. } => {
            Some(panel)
        },
        ImeTarget::AppOwned { .. } => None,
    }
}

impl ImeBlurClassification {
    fn is_inside_focus_scope(&self, target: &ImeTarget) -> bool {
        self.source_panel
            .is_some_and(|panel| panel == self.clicked_panel)
            || matches!(target, ImeTarget::AppOwned { owner, .. } if *owner == self.clicked_panel)
    }
}

fn field_record<'a>(
    computed: &'a ComputedDiegeticPanel,
    field_id: &PanelFieldId,
) -> Option<&'a PanelFieldRecord> {
    computed
        .field_records()
        .iter()
        .find(|record| !record.duplicate_id && record.field_id == *field_id)
}

fn project_field_record(
    record: &PanelFieldRecord,
    panel: &DiegeticPanel,
    panel_transform: &GlobalTransform,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> Option<Rect> {
    let corners = panel_local_corners(record.bounds, panel);
    let mut points = Vec::with_capacity(corners.len());
    for corner in corners {
        let world = panel_transform.transform_point(corner);
        let viewport = camera.world_to_viewport(camera_transform, world).ok()?;
        points.push(viewport);
    }
    rect_from_points(&points)
}

fn panel_local_corners(bounds: BoundingBox, panel: &DiegeticPanel) -> [Vec3; 4] {
    let points_to_world = panel.points_to_world();
    let (anchor_x, anchor_y) = panel.anchor_offsets();
    let left = bounds.x.mul_add(points_to_world, -anchor_x);
    let right = (bounds.x + bounds.width).mul_add(points_to_world, -anchor_x);
    let top = (-bounds.y).mul_add(points_to_world, anchor_y);
    let bottom = (-(bounds.y + bounds.height)).mul_add(points_to_world, anchor_y);
    [
        Vec3::new(left, top, 0.0),
        Vec3::new(right, top, 0.0),
        Vec3::new(right, bottom, 0.0),
        Vec3::new(left, bottom, 0.0),
    ]
}

fn rect_from_points(points: &[Vec2]) -> Option<Rect> {
    let first = *points.first()?;
    let mut min = first;
    let mut max = first;
    for point in points.iter().copied().skip(1) {
        if !point.is_finite() {
            return None;
        }
        min = min.min(point);
        max = max.max(point);
    }
    if max.x - min.x < SOURCE_RECT_MIN_AXIS || max.y - min.y < SOURCE_RECT_MIN_AXIS {
        return None;
    }
    Some(Rect { min, max })
}

fn fallback_screen_rect(window: &Window) -> Rect {
    let origin = window.cursor_position().unwrap_or(Vec2::ZERO);
    Rect {
        min: origin,
        max: origin + Vec2::new(DEFAULT_EDITOR_WIDTH, DEFAULT_EDITOR_HEIGHT),
    }
}

fn app_anchor_rect(anchor: Option<ImeSessionAnchor>, window: &Window) -> Rect {
    match anchor {
        Some(ImeSessionAnchor::ScreenRect(rect)) => rect,
        Some(ImeSessionAnchor::ScreenPoint(point)) => Rect {
            min: point,
            max: point + Vec2::new(DEFAULT_EDITOR_WIDTH, DEFAULT_EDITOR_HEIGHT),
        },
        None => fallback_screen_rect(window),
    }
}

fn editor_size(screen_rect: Rect) -> Vec2 {
    let width =
        (screen_rect.width() + EDITOR_EXTRA_WIDTH).clamp(MIN_EDITOR_WIDTH, MAX_EDITOR_WIDTH);
    Vec2::new(width, DEFAULT_EDITOR_HEIGHT)
}

fn clamp_editor_position(position: Vec2, editor_size: Vec2, window: &Window) -> Vec2 {
    let max_x = (window.width() - editor_size.x).max(0.0);
    let max_y = (window.height() - editor_size.y).max(0.0);
    Vec2::new(position.x.clamp(0.0, max_x), position.y.clamp(0.0, max_y))
}

fn caret_position(editor_pos: Vec2, editor_size: Vec2, snapshot: &ImeBufferSnapshot) -> Vec2 {
    let measured_len = display_text_len(snapshot);
    let text_len = if measured_len > 0.0 {
        measured_len
    } else {
        EMPTY_TEXT_ADVANCE
    };
    let cursor = display_cursor_len(snapshot).min(text_len);
    let content_width = EDITOR_PADDING_X.mul_add(-2.0, editor_size.x).max(0.0);
    let caret_x = EDITOR_PADDING_X + content_width * (cursor / text_len);
    Vec2::new(editor_pos.x + caret_x, editor_pos.y + EDITOR_PADDING_Y)
}

fn display_text_len(snapshot: &ImeBufferSnapshot) -> f32 {
    if let Some(preedit) = &snapshot.preedit {
        let replaced = preedit.replacement.end.as_usize() - preedit.replacement.start.as_usize();
        let len = snapshot.committed_text.len() - replaced + preedit.text.len();
        return len.to_f32();
    }
    snapshot.committed_text.len().to_f32()
}

fn display_cursor_len(snapshot: &ImeBufferSnapshot) -> f32 {
    if let Some(preedit) = &snapshot.preedit {
        let start = preedit.replacement.start.as_usize();
        let preedit_cursor = preedit
            .cursor
            .map_or(preedit.text.len(), ImePreeditBoundary::as_usize);
        return (start + preedit_cursor).to_f32();
    }

    match &snapshot.cursor {
        ImeCursorState::Insertion(boundary) => boundary.as_usize().to_f32(),
        ImeCursorState::Selection(selection) => selection.focus.as_usize().to_f32(),
    }
}

fn editor_tree(snapshot: &ImeBufferSnapshot, validation: Option<&str>) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::xy(EDITOR_PADDING_X, EDITOR_PADDING_Y))
            .child_gap(EDITOR_GAP)
            .direction(Direction::TopToBottom)
            .background(EDITOR_BACKGROUND)
            .border(Border::all(EDITOR_BORDER_WIDTH, EDITOR_BORDER))
            .corner_radius(EDITOR_CORNER_RADIUS),
    );

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(0.0)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| append_buffer(builder, snapshot),
    );

    if let Some(validation) = validation {
        add_text(&mut builder, validation, EDITOR_VALIDATION);
    }

    builder.build()
}

fn append_buffer(builder: &mut LayoutBuilder, snapshot: &ImeBufferSnapshot) {
    if let Some(preedit) = &snapshot.preedit {
        append_preedit_buffer(builder, snapshot, preedit);
        return;
    }

    match &snapshot.cursor {
        ImeCursorState::Insertion(boundary) => {
            let index = boundary.as_usize();
            add_text(builder, &snapshot.committed_text[..index], EDITOR_TEXT);
            add_caret(builder);
            add_text(builder, &snapshot.committed_text[index..], EDITOR_TEXT);
        },
        ImeCursorState::Selection(selection) => {
            let (start, end) = selection_range(selection);
            add_text(builder, &snapshot.committed_text[..start], EDITOR_TEXT);
            add_selected_text(builder, &snapshot.committed_text[start..end]);
            add_text(builder, &snapshot.committed_text[end..], EDITOR_TEXT);
        },
    }
}

fn append_preedit_buffer(
    builder: &mut LayoutBuilder,
    snapshot: &ImeBufferSnapshot,
    preedit: &ImePreedit,
) {
    let start = preedit.replacement.start.as_usize();
    let end = preedit.replacement.end.as_usize();
    let cursor = preedit
        .cursor
        .map_or(preedit.text.len(), ImePreeditBoundary::as_usize);

    add_text(builder, &snapshot.committed_text[..start], EDITOR_TEXT);
    add_text(builder, &preedit.text[..cursor], EDITOR_PREEDIT);
    add_caret(builder);
    add_text(builder, &preedit.text[cursor..], EDITOR_PREEDIT);
    add_text(builder, &snapshot.committed_text[end..], EDITOR_TEXT);
}

fn selection_range(selection: &ImeSelectionSnapshot) -> (usize, usize) {
    let anchor = selection.anchor.as_usize();
    let focus = selection.focus.as_usize();
    (anchor.min(focus), anchor.max(focus))
}

fn add_text(builder: &mut LayoutBuilder, text: &str, color: Color) {
    if text.is_empty() {
        return;
    }
    builder.text(
        text,
        LayoutTextStyle::new(EDITOR_FONT_SIZE)
            .with_color(color)
            .no_wrap(),
    );
}

fn add_selected_text(builder: &mut LayoutBuilder, text: &str) {
    if text.is_empty() {
        return;
    }
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .background(EDITOR_SELECTION)
            .padding(Padding::xy(EDITOR_GAP, 0.0)),
        |builder| add_text(builder, text, EDITOR_TEXT),
    );
}

fn add_caret(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::fixed(CARET_WIDTH))
            .height(Sizing::fixed(CARET_HEIGHT))
            .background(EDITOR_CARET),
        |_| {},
    );
}

#[cfg(test)]
mod tests {
    use bevy::math::Rect;
    use bevy::math::Vec2;
    use bevy::prelude::Window;
    use bevy_kana::ToF32;

    use super::caret_position;
    use super::clamp_editor_position;
    use super::display_cursor_len;
    use super::display_text_len;
    use super::editor_size;
    use crate::ImeBufferBoundary;
    use crate::ImeBufferRange;
    use crate::ImeBufferSnapshot;
    use crate::ImeCursorState;
    use crate::ImePreedit;
    use crate::ImePreeditBoundary;
    use crate::ImeSelectionSnapshot;

    fn assert_float_eq(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= f32::EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    fn insertion_snapshot(text: &str, cursor: usize) -> ImeBufferSnapshot {
        ImeBufferSnapshot {
            committed_text: text.to_owned(),
            cursor:         ImeCursorState::Insertion(ImeBufferBoundary::new(cursor)),
            preedit:        None,
        }
    }

    #[test]
    fn caret_uses_snapshot_cursor_without_splitting_utf8() {
        let snapshot = insertion_snapshot("aé日", "aé".len());

        assert_float_eq(display_text_len(&snapshot), "aé日".len().to_f32());
        assert_float_eq(display_cursor_len(&snapshot), "aé".len().to_f32());
    }

    #[test]
    fn preedit_replaces_selection_for_display_length() {
        let snapshot = ImeBufferSnapshot {
            committed_text: "abcdef".to_owned(),
            cursor:         ImeCursorState::Selection(ImeSelectionSnapshot {
                anchor: ImeBufferBoundary::new(1),
                focus:  ImeBufferBoundary::new(4),
            }),
            preedit:        Some(ImePreedit {
                text:        "xy".to_owned(),
                replacement: ImeBufferRange {
                    start: ImeBufferBoundary::new(1),
                    end:   ImeBufferBoundary::new(4),
                },
                cursor:      Some(ImePreeditBoundary::new(1)),
            }),
        };

        assert_float_eq(display_text_len(&snapshot), 5.0);
        assert_float_eq(display_cursor_len(&snapshot), 2.0);
    }

    #[test]
    fn editor_position_clamps_to_window_bounds() {
        let mut window = Window::default();
        window.resolution.set(320.0, 160.0);
        let position =
            clamp_editor_position(Vec2::new(300.0, 150.0), Vec2::new(80.0, 40.0), &window);

        assert_eq!(position, Vec2::new(240.0, 120.0));
    }

    #[test]
    fn caret_position_tracks_editor_width() {
        let snapshot = insertion_snapshot("abcd", 2);
        let caret = caret_position(Vec2::ZERO, Vec2::new(104.0, 34.0), &snapshot);

        assert!(caret.x > 40.0);
        assert!(caret.x < 70.0);
    }

    #[test]
    fn editor_size_is_bounded_from_source_rect() {
        let size = editor_size(Rect::from_corners(Vec2::ZERO, Vec2::new(900.0, 20.0)));

        assert_float_eq(size.x, super::MAX_EDITOR_WIDTH);
    }
}
