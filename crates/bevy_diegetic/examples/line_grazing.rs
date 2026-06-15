//! Line-orientation grazing probe.
//!
//! White panel lines on one wide world panel — several full-width horizontals,
//! a vertical at each end, and one diagonal — at a single stroke width and
//! color. Orbit and pitch the camera to graze each orientation in turn.
//! Orientation is the only variable: a single color keeps every line in one
//! draw slot, so any breakup at grazing is the line path itself, not
//! multi-color OIT ordering. If the horizontals and diagonal break where the
//! end verticals stay solid, the analytic line path has an orientation
//! dependence; if all behave the same, it does not.

use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::CalloutCap;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DrawOverflow;
use bevy_diegetic::El;
use bevy_diegetic::HairlineFade;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Mm;
use bevy_diegetic::PanelDraw;
use bevy_diegetic::PanelLine;
use bevy_diegetic::PanelPoint;
use bevy_diegetic::Sizing;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::TitleBar;

const HOME_PITCH: f32 = 0.18;
const HOME_YAW: f32 = 0.0;
const HOME_MARGIN: f32 = 0.5;

/// Panel width in millimeters; horizontals span its full extent.
const PANEL_W_MM: f32 = 300.0;
/// Panel height in millimeters; verticals span its full extent.
const PANEL_H_MM: f32 = 120.0;
/// Stroke width in millimeters for the end verticals and the diagonal.
const STROKE_MM: f32 = 1.8;
/// Graduated stroke widths (mm) for the full-width horizontals, thin to thick,
/// each on its own row. Isolates whether the grazing stairstep correlates with
/// stroke width: the dilation floor cleans a sub-pixel hairline, but a thick
/// stroke skips that floor and foreshortens with raw edges.
const HORIZONTAL_WIDTHS_MM: [f32; 5] = [0.25, 0.5, 1.0, 1.8, 3.0];
/// Layout y for each graduated horizontal (panel y grows down).
const HORIZONTAL_Y: [f32; 5] = [20.0, 40.0, 60.0, 80.0, 100.0];

/// Side length (world meters) of each tight per-box panel, large to small.
/// These replicate the typography overlay's per-glyph box panels: each box is
/// its own small panel whose merge-group bounds are just the box outline.
const TIGHT_BOX_SIZES_M: [f32; 4] = [0.30, 0.20, 0.12, 0.06];
/// Stroke width (world meters) for the tight box edges — matches the overlay's
/// `bbox_border_width` (~1.2mm) so width is not the variable.
const TIGHT_BOX_STROKE_M: f32 = 0.0012;
/// World Y of the tight boxes' centers, clear above the big reference panel.
const TIGHT_BOX_Y_M: f32 = 0.42;

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::BlenderLike)
        .unclamped()
        .with_stable_transparency()
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Line Grazing")
                .with_background_color(DEFAULT_PANEL_BACKGROUND.with_alpha(0.9))
                .control("H Home"),
        )
        .with_camera_control_panel()
        .add_systems(Startup, spawn_line_panel)
        .run();
}

/// One wide panel: three full-width horizontals, a vertical at each end (the
/// box sides), and the top-left-to-bottom-right diagonal. `DrawOverflow::Visible`
/// keeps the end verticals from clipping at the panel edge.
fn spawn_line_panel(mut commands: Commands) {
    let mut lines = Vec::new();
    for (y, width) in HORIZONTAL_Y.into_iter().zip(HORIZONTAL_WIDTHS_MM) {
        lines.push(white_line((0.0, y), (PANEL_W_MM, y), width));
    }
    lines.push(white_line((0.0, 0.0), (0.0, PANEL_H_MM), STROKE_MM));
    lines.push(white_line(
        (PANEL_W_MM, 0.0),
        (PANEL_W_MM, PANEL_H_MM),
        STROKE_MM,
    ));
    lines.push(white_line((0.0, 0.0), (PANEL_W_MM, PANEL_H_MM), STROKE_MM));

    // Capped, inset vertical dimension arrows merged into the SAME path as the
    // horizontals — the one feature the typography metric panel has that the rig
    // never reproduced. There, solid arrowhead caps and end insets are folded
    // into the analytic path that the plain horizontal guides share; the
    // isolated metric guides (#71: word/ground/boxes/labels/shadows all off)
    // still break. If the horizontals tear once these capped arrows are merged
    // in, the cap/inset geometry is corrupting the shared analytic path.
    for x in [40.0, 150.0, 260.0] {
        lines.push(white_arrow(x, 20.0, 100.0, STROKE_MM));
    }

    // Floated clear of the translucent ground (y = 0.08) so ground OIT
    // compositing is not a confound.
    let transform = Transform::from_xyz(PANEL_W_MM * -0.0005, 0.08, 0.0);
    spawn_overlapping_panel(&mut commands, "Grazing lines panel", lines, transform, true);

    // Tight per-box panels above the reference panel. Same XY plane, same
    // orientation, same stroke width — the ONE difference from the big panel is
    // the merge-group bounds: each box's quad is just the box outline, so its
    // four edge lines sit at the quad boundary with only the fixed-world AA pad
    // beyond them. If these stairstep at grazing where the big panel's lines
    // stay clean, the fixed-world quad pad is clipping the foreshortened AA ramp
    // — the structural difference between the rig and the typography overlay.
    let total = TIGHT_BOX_SIZES_M.iter().sum::<f32>();
    let mut left = -total * 0.5;
    for size in TIGHT_BOX_SIZES_M {
        let center = Vec3::new(size.mul_add(0.5, left), TIGHT_BOX_Y_M, 0.0);
        spawn_tight_box(&mut commands, center, size, TIGHT_BOX_STROKE_M);
        left += size + 0.04;
    }
}

/// One tight box panel sized to the box, replicating the typography overlay's
/// per-glyph `spawn_glyph_box_panels`. Four edge lines inset half a stroke,
/// merged in one path; `Anchor::Center` places the box center at `center`.
fn spawn_tight_box(commands: &mut Commands, center: Vec3, size: f32, line_width: f32) {
    let inset = line_width * 0.5;
    let lines = [
        ((0.0, inset), (size, inset)),
        ((0.0, size - inset), (size, size - inset)),
        ((inset, 0.0), (inset, size)),
        ((size - inset, 0.0), (size - inset, size)),
    ]
    .map(|((x0, y0), (x1, y1))| {
        PanelLine::new(PanelPoint::new(x0, y0), PanelPoint::new(x1, y1))
            .width(line_width)
            .color(Color::WHITE)
    });

    let tree = LayoutBuilder::with_root(
        El::new()
            .size(size, size)
            .hairline_fade(HairlineFade::Full)
            .draw(PanelDraw::lines(lines).overflow(DrawOverflow::Visible)),
    )
    .build();

    let material = StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    };

    let Ok(panel) = DiegeticPanel::world()
        .size(size, size)
        .anchor(Anchor::Center)
        .material(material)
        .with_tree(tree)
        .build()
    else {
        return;
    };

    commands.spawn((
        Name::new("Tight box panel"),
        panel,
        Transform::from_translation(center),
    ));
}

/// Spawns one transparent world panel of `lines` at `transform`; `is_home` tags
/// it as the camera-home target.
fn spawn_overlapping_panel(
    commands: &mut Commands,
    name: &'static str,
    lines: Vec<PanelLine>,
    transform: Transform,
    is_home: bool,
) {
    let tree = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .draw(PanelDraw::lines(lines).overflow(DrawOverflow::Visible)),
    )
    .build();

    let material = StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    };

    let Ok(panel) = DiegeticPanel::world()
        .size(Mm(PANEL_W_MM), Mm(PANEL_H_MM))
        .anchor(Anchor::BottomLeft)
        .material(material)
        .with_tree(tree)
        .build()
    else {
        return;
    };

    if is_home {
        commands.spawn((Name::new(name), CameraHomeTarget, panel, transform));
    } else {
        commands.spawn((Name::new(name), panel, transform));
    }
}

fn white_line(start: (f32, f32), end: (f32, f32), width_mm: f32) -> PanelLine {
    PanelLine::new(
        PanelPoint::new(start.0, start.1),
        PanelPoint::new(end.0, end.1),
    )
    .width(width_mm)
    .color(Color::WHITE)
}

/// A vertical line with solid arrowhead caps and end insets, matching the
/// typography metric panel's dimension arrows.
fn white_arrow(x: f32, y0: f32, y1: f32, width_mm: f32) -> PanelLine {
    let head = width_mm * 4.0;
    PanelLine::new(PanelPoint::new(x, y0), PanelPoint::new(x, y1))
        .width(width_mm)
        .color(Color::WHITE)
        .start_inset(width_mm * 2.0)
        .end_inset(width_mm * 2.0)
        .start_cap(CalloutCap::arrow().solid().length(head).width(head))
        .end_cap(CalloutCap::arrow().solid().length(head).width(head))
}
