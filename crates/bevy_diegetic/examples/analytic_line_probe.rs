//! Standalone analytic line probe.
//!
//! Draws lines through the same analytic-path renderer the text glyphs use,
//! bypassing `PanelDraw::lines` and the whole panel/layout route. A reference
//! glyph sits beside the lines so stroke quality can be compared directly.
//!
//! Each line is an authored `AnalyticLine` (start/end in the placement plane,
//! stroke width, color); the `Transform` places that plane in the world. Tweak
//! the constants or the spawns below to control them.

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::AnalyticLine;
use bevy_diegetic::AnalyticLineProbePlugin;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticText;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::PanelDraw;
use bevy_diegetic::PanelLine;
use bevy_diegetic::PanelPoint;
use bevy_diegetic::Pt;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use bevy_kana::ToF32;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::TitleBar;

const HOME_PITCH: f32 = 0.18;
const HOME_YAW: f32 = 0.0;
const HOME_MARGIN: f32 = 0.55;
const DISPLAY_Z: f32 = 0.05;
const MM_TO_METERS: f32 = 0.001;
const PROBE_RULER_MARKS: i32 = 100;
const PROBE_RULER_HEIGHT_MM: f32 = 100.0;
const TICK_TRACK_MM: f32 = 5.0;
const SPINE_WIDTH_MM: f32 = 0.2;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        //   .with_ground_plane()
        .with_orbit_cam_preset(
            |cam| {
                // Millimeter-scale strokes need unrestricted inspection:
                // clear every example clamp.
                cam.zoom_lower_limit = 0.000_000_001;
                cam.zoom_upper_limit = None;
                cam.pitch_upper_limit = None;
                cam.pitch_lower_limit = None;
            },
            OrbitCamPreset::BlenderLike,
        )
        .with_stable_transparency()
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Analytic Line Probe")
                .with_background_color(DEFAULT_PANEL_BACKGROUND.with_alpha(0.9))
                .control("H Home")
                .control("1 Probe ruler")
                .control("2 Panel ruler")
                .control("3 Spine-only panel"),
        )
        .with_camera_control_panel()
        .add_plugins(AnalyticLineProbePlugin)
        .add_systems(
            Startup,
            (
                spawn_reference_glyph,
                spawn_probe_lines,
                spawn_panel_ruler,
                spawn_spine_only_panel,
            ),
        )
        .add_systems(Update, (focus_keys, aa_toggle))
        .run();
}

fn spawn_reference_glyph(mut commands: Commands) {
    commands.spawn((
        Name::new("Text glyph reference"),
        DiegeticText::world("1")
            .size(0.26)
            .color(Color::WHITE)
            .transform(Transform::from_xyz(-1.2, 0.18, DISPLAY_Z))
            .build(),
    ));
}

/// Metric ruler at true physical scale, matching the `units` example: 0.1mm
/// millimeter ticks, 0.3mm centimeter ticks, 0.2mm spine, 1mm pitch. If this
/// renders cleanly where the `units` panel-line ruler does not, the defect is
/// in the panel-line conversion data, not the shared shader.
fn spawn_probe_lines(mut commands: Commands) {
    let plane = Transform::from_xyz(0.0, 0.0, DISPLAY_Z);
    let spine_x = -0.01;

    commands.spawn((
        Name::new("Probe ruler spine"),
        CameraHomeTarget,
        AnalyticLine::new(
            Vec2::new(spine_x, 0.0),
            Vec2::new(spine_x, PROBE_RULER_HEIGHT_MM * MM_TO_METERS),
        )
        .width(SPINE_WIDTH_MM * MM_TO_METERS)
        .color(Color::WHITE),
        plane,
    ));

    for mark in 0..=PROBE_RULER_MARKS {
        let (length_mm, stroke_mm) = tick_size(mark);
        let y = mark.to_f32() * MM_TO_METERS;
        commands.spawn((
            Name::new(format!("Probe tick {mark}")),
            AnalyticLine::new(
                Vec2::new(length_mm.mul_add(-MM_TO_METERS, spine_x), y),
                Vec2::new(spine_x, y),
            )
            .width(stroke_mm * MM_TO_METERS)
            .color(Color::WHITE),
            plane,
        ));
    }
}

const fn tick_size(mark: i32) -> (f32, f32) {
    if mark % 10 == 0 {
        (5.0, 0.3)
    } else if mark % 5 == 0 {
        (3.5, 0.1)
    } else {
        (2.0, 0.1)
    }
}

/// The identical ruler authored as `PanelDraw::lines` on a `DiegeticPanel`,
/// the route the `units` example uses. Rendering differences between this and
/// the `AnalyticLine` ruler isolate the panel-line conversion.
fn spawn_panel_ruler(mut commands: Commands) {
    let spine_x = 8.0;

    let mut lines = vec![
        PanelLine::new(
            PanelPoint::new(spine_x, 0.0),
            PanelPoint::new(spine_x, PROBE_RULER_HEIGHT_MM),
        )
        .width(SPINE_WIDTH_MM)
        .color(Color::WHITE),
    ];
    for mark in 0..=PROBE_RULER_MARKS {
        let (length_mm, stroke_mm) = tick_size(mark);
        // Panel layout y grows down; mark 0 sits at the panel bottom.
        let y = (PROBE_RULER_MARKS - mark).to_f32();
        lines.push(
            PanelLine::new(
                PanelPoint::new(spine_x - length_mm, y),
                PanelPoint::new(spine_x, y),
            )
            .width(stroke_mm)
            .color(Color::WHITE),
        );
    }

    let tree = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .draw(PanelDraw::lines(lines)),
    )
    .build();

    let Ok(panel) = DiegeticPanel::world()
        .size(Mm(10.0), Mm(100.0))
        .anchor(Anchor::BottomLeft)
        .with_tree(tree)
        .build()
    else {
        return;
    };

    // Panel spine sits at panel-local 8mm; this places it at world x = +0.01,
    // mirroring the probe ruler's spine at -0.01.
    commands.spawn((
        Name::new("Panel-route ruler"),
        CameraHomeTarget,
        panel,
        Transform::from_xyz(0.002, 0.0, DISPLAY_Z),
    ));
}

/// Panels mirroring the `units` metric ruler's nested layout: a grow tick
/// column plus a fixed 0.2mm spine column, each drawing its lines inside a
/// tightly-fitting child element so the owner-bounds clip applies exactly as
/// it does in `units`. The flat-root panels of the earlier bisect rendered
/// cleanly at coarse pixel scales; `units` does not.
fn spawn_spine_only_panel(mut commands: Commands) {
    let panels = [
        SpinePanelConfig::new(25, true),
        SpinePanelConfig::new(50, true),
        SpinePanelConfig::new(100, true),
        SpinePanelConfig::new(297, true),
        SpinePanelConfig::new(297, false),
    ];
    for (index, config) in panels.into_iter().enumerate() {
        let Some(panel) = nested_ruler_panel(config) else {
            continue;
        };
        commands.spawn((
            Name::new(format!("Nested ruler {}mm", config.height_marks)),
            panel,
            Transform::from_xyz(0.014_f32.mul_add(index.to_f32(), 0.022), 0.0, DISPLAY_Z),
        ));
    }
}

#[derive(Clone, Copy)]
struct SpinePanelConfig {
    height_marks: i32,
    labeled:      bool,
}

impl SpinePanelConfig {
    const fn new(height_marks: i32, labeled: bool) -> Self {
        Self {
            height_marks,
            labeled,
        }
    }

    fn height_mm(self) -> f32 { self.height_marks.to_f32() }
}

fn nested_ruler_panel(config: SpinePanelConfig) -> Option<DiegeticPanel> {
    DiegeticPanel::world()
        .size(Mm(10.0), Mm(config.height_mm()))
        .anchor(Anchor::BottomLeft)
        .with_tree(nested_ruler_tree(config))
        .build()
        .ok()
}

fn nested_ruler_tree(config: SpinePanelConfig) -> LayoutTree {
    use bevy_diegetic::Direction;

    let (ticks, spine) = nested_ruler_lines(config);
    let mut builder = LayoutBuilder::new(Mm(10.0), Mm(config.height_mm()));
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            add_label_space(b, config);
            add_tick_track(b, ticks, spine);
        },
    );
    builder.build()
}

fn nested_ruler_lines(config: SpinePanelConfig) -> (Vec<PanelLine>, PanelLine) {
    let height_mm = config.height_mm();
    let mut ticks = Vec::new();
    for mark in 0..=config.height_marks {
        let (length_mm, stroke_mm) = tick_size(mark);
        let center_min = stroke_mm * 0.5;
        let center_max = stroke_mm.mul_add(-0.5, height_mm);
        let y = (height_mm - mark.to_f32()).clamp(center_min, center_max);
        ticks.push(
            PanelLine::new(
                PanelPoint::new(TICK_TRACK_MM - length_mm, y),
                PanelPoint::new(TICK_TRACK_MM, y),
            )
            .width(stroke_mm)
            .color(Color::WHITE),
        );
    }
    let spine_center = SPINE_WIDTH_MM * 0.5;
    let spine = PanelLine::new(
        PanelPoint::new(spine_center, 0.0),
        PanelPoint::new(spine_center, height_mm),
    )
    .width(SPINE_WIDTH_MM)
    .color(Color::WHITE);
    (ticks, spine)
}

fn add_label_space(builder: &mut LayoutBuilder, config: SpinePanelConfig) {
    use bevy_diegetic::Direction;

    if !config.labeled {
        // Same column geometry, no text: the discriminating variable is the
        // glyphs, not the layout.
        builder.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        return;
    }

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom)
            .child_align_x(AlignX::Right)
            .padding(Padding::new(Mm(0.0), Mm(1.0), Mm(0.0), Mm(0.0))),
        |b| add_labels(b, config),
    );
}

fn add_labels(builder: &mut LayoutBuilder, config: SpinePanelConfig) {
    let label_style = TextStyle::new(Pt(8.0)).with_color(Color::WHITE);
    let last_centimeter_mark = ((config.height_marks - 5) / 10).max(0);
    let top_spacer = last_centimeter_mark
        .to_f32()
        .mul_add(-10.0, config.height_mm())
        - 5.0;

    if top_spacer > 0.0 {
        builder.with(
            El::new()
                .height(Sizing::fixed(Mm(top_spacer)))
                .width(Sizing::GROW),
            |_| {},
        );
    }
    for centimeter in (1..=last_centimeter_mark).rev() {
        builder.with(
            El::new()
                .height(Sizing::fixed(Mm(10.0)))
                .width(Sizing::GROW)
                .child_align_x(AlignX::Right)
                .child_align_y(AlignY::Center),
            |b| {
                b.text(format!("{centimeter}"), label_style.clone());
            },
        );
    }
    builder.with(
        El::new().height(Sizing::fixed(Mm(5.0))).width(Sizing::GROW),
        |_| {},
    );
}

fn add_tick_track(builder: &mut LayoutBuilder, ticks: Vec<PanelLine>, spine: PanelLine) {
    use bevy_diegetic::Direction;

    builder.with(
        El::new()
            .width(Sizing::fixed(Mm(TICK_TRACK_MM + SPINE_WIDTH_MM)))
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .draw(PanelDraw::lines(ticks)),
                |_| {},
            );
            b.with(
                El::new()
                    .width(Sizing::fixed(Mm(SPINE_WIDTH_MM)))
                    .height(Sizing::GROW)
                    .draw(PanelDraw::lines([spine])),
                |_| {},
            );
        },
    );
}

/// A toggles anti-aliasing Off/Both to split winding defects from AA defects.
fn aa_toggle(keys: Res<ButtonInput<KeyCode>>, mut setting: ResMut<bevy_diegetic::TextAntiAlias>) {
    if keys.just_pressed(KeyCode::KeyA) {
        *setting = if *setting == bevy_diegetic::TextAntiAlias::Off {
            bevy_diegetic::TextAntiAlias::Both
        } else {
            bevy_diegetic::TextAntiAlias::Off
        };
    }
}

/// Number keys move the orbit focus to each ruler for close inspection.
fn focus_keys(keys: Res<ButtonInput<KeyCode>>, mut cams: Query<&mut bevy_lagrange::OrbitCam>) {
    let target = if keys.just_pressed(KeyCode::Digit1) {
        Some(-0.01)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(0.01)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(0.03)
    } else {
        None
    };
    let Some(x) = target else { return };
    for mut cam in &mut cams {
        cam.target_focus = Vec3::new(x, 0.05, DISPLAY_Z);
        cam.target_radius = 0.12;
    }
}
