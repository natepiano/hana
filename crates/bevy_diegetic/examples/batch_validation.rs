//! Batching validation scene for SDF surfaces, text, and analytic shapes.
//!
//! The scene displays authored record counts, material-table rows, and expected
//! batch counts alongside live renderer counters.

use bevy::camera::primitives::Aabb;
use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CalloutCap;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::DiegeticPerfStats;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::LineStyle;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::PanelCircle;
use bevy_diegetic::PanelCoord;
use bevy_diegetic::PanelDraw;
use bevy_diegetic::PanelLine;
use bevy_diegetic::PanelPoint;
use bevy_diegetic::Sidedness;
use bevy_diegetic::Sizing;
use bevy_diegetic::SurfaceShadow;
use bevy_diegetic::TextStyle;
use bevy_diegetic::default_panel_material;
use bevy_kana::ToF32;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::StatsPanelRow;
use fairy_dust::StatsPanelSection;
use fairy_dust::TitleBar;
use fairy_dust::diegetic_stats_sections_panel;
use fairy_dust::diegetic_stats_sections_tree;
use fairy_dust::screen_panel_frame;
use fairy_dust::screen_panel_material;

const PANEL_W: f32 = 170.0;
const PANEL_H: f32 = 132.0;
const PANEL_GAP_X: f32 = 0.012;
const PANEL_GAP_Y: f32 = 0.012;
const MM_TO_WORLD: f32 = 0.001;
const PANEL_WORLD_W: f32 = PANEL_W * MM_TO_WORLD;
const PANEL_WORLD_H: f32 = PANEL_H * MM_TO_WORLD;
const PANEL_STEP_X: f32 = PANEL_WORLD_W + PANEL_GAP_X;
const PANEL_STEP_Y: f32 = PANEL_WORLD_H + PANEL_GAP_Y;
const PANEL_GRID_CENTER_Y: f32 = 0.17;
const PANEL_HOME_PAD: f32 = 0.012;
const GROUND_SIZE: f32 = 1.45;
const HOME_FOCUS: Vec3 = Vec3::new(0.0, PANEL_GRID_CENTER_Y, 0.0);
const HOME_RADIUS: f32 = 0.50;
const HOME_PITCH: f32 = 0.0;
const HOME_MARGIN: f32 = 0.15;
// Point light placed in front of and up-left of the metallic center card
// (world center ~(-0.091, 0.242, 0), front face +Z) to land a specular glint.
const GLINT_LIGHT_POS: Vec3 = Vec3::new(-0.13, 0.30, 0.16);
const GLINT_LIGHT_LUMENS: f32 = 2000.0;

const AUTHORED_PANELS: usize = 4;

// Authored content each panel draws, counted by family. Each panel renders its
// own copy in the upper-right corner; the global "authored content" totals are
// the sum of these four, so per-panel and aggregate readouts stay consistent.
// One El background = one sdf fill, one El border = one sdf border, one
// `builder.text` = one text run, one panel-shape render record = one path row.
const SDF_PANEL_STATS: PanelStats = PanelStats {
    sdf_fills:        4,
    sdf_borders:      5,
    material_slots:   9,
    readout_reason:   "value share / texture split",
    text_runs:        10,
    shape_records:    0,
    shape_primitives: 0,
};
const TEXT_PANEL_STATS: PanelStats = PanelStats {
    sdf_fills:        8,
    sdf_borders:      3,
    material_slots:   12,
    readout_reason:   "value share / texture split",
    text_runs:        23,
    shape_records:    0,
    shape_primitives: 0,
};
const SHAPE_PANEL_STATS: PanelStats = PanelStats {
    sdf_fills:        3,
    sdf_borders:      4,
    material_slots:   6,
    readout_reason:   "shape rows share; texture splits",
    text_runs:        8,
    shape_records:    6,
    shape_primitives: 11,
};
const MIXED_PANEL_STATS: PanelStats = PanelStats {
    sdf_fills:        5,
    sdf_borders:      2,
    material_slots:   7,
    readout_reason:   "families split",
    text_runs:        15,
    shape_records:    1,
    shape_primitives: 3,
};

const AUTHORED_SDF_FILLS: usize = SDF_PANEL_STATS.sdf_fills
    + TEXT_PANEL_STATS.sdf_fills
    + SHAPE_PANEL_STATS.sdf_fills
    + MIXED_PANEL_STATS.sdf_fills;
const AUTHORED_SDF_BORDERS: usize = SDF_PANEL_STATS.sdf_borders
    + TEXT_PANEL_STATS.sdf_borders
    + SHAPE_PANEL_STATS.sdf_borders
    + MIXED_PANEL_STATS.sdf_borders;
const AUTHORED_TEXT_RUNS: usize = SDF_PANEL_STATS.text_runs
    + TEXT_PANEL_STATS.text_runs
    + SHAPE_PANEL_STATS.text_runs
    + MIXED_PANEL_STATS.text_runs;
const AUTHORED_SHAPE_RECORDS: usize = SDF_PANEL_STATS.shape_records
    + TEXT_PANEL_STATS.shape_records
    + SHAPE_PANEL_STATS.shape_records
    + MIXED_PANEL_STATS.shape_records;
const AUTHORED_SHAPE_PRIMITIVES: usize = SDF_PANEL_STATS.shape_primitives
    + TEXT_PANEL_STATS.shape_primitives
    + SHAPE_PANEL_STATS.shape_primitives
    + MIXED_PANEL_STATS.shape_primitives;

const SDF_ANIMATION_GREEN_OFFSET: f32 = 2.1;
const SDF_ANIMATION_RED_OFFSET: f32 = 4.2;
const SDF_ANIMATION_SPEED: f32 = 0.9;
const FPS_UPDATE_INTERVAL: f32 = 1.0;

// Author's prediction of how many draw batches each on-screen panel collapses
// into, per render family. Batches merge globally by compatibility key, so these
// are intent values the live `actual` row is reconciled against. Columns are
// text, shape, sdf — the order the ledger renders.
struct PanelExpected {
    label:       &'static str,
    text:        usize,
    shape:       usize,
    sdf:         usize,
    // True for the panel whose live alpha-mode run forms its own text batch when
    // the selector leaves Blend, adding one to the predicted text total.
    alpha_split: bool,
}

// Predicted text batches for one panel under the active alpha mode: the baseline
// count plus the live alpha-mode split when the selector is off Blend.
const fn panel_text_for_mode(panel: &PanelExpected, alpha_index: usize) -> usize {
    if panel.alpha_split && alpha_index != ALPHA_DEFAULT_INDEX {
        panel.text + 1
    } else {
        panel.text
    }
}

// Predicted global batch totals [text, shape, sdf] for the active alpha mode.
// Shape and sdf totals are alpha-invariant; only text gains the live alpha-mode
// split.
fn expected_totals(alpha_index: usize) -> [usize; 3] {
    let mut totals = [0_usize; 3];
    for panel in &EXPECTED_BATCHES {
        totals[0] += panel_text_for_mode(panel, alpha_index);
        totals[1] += panel.shape;
        totals[2] += panel.sdf;
    }
    totals
}

const EXPECTED_BATCHES: [PanelExpected; 8] = [
    PanelExpected {
        label:       "Title",
        text:        1,
        shape:       0,
        sdf:         1,
        alpha_split: false,
    },
    PanelExpected {
        label:       "Info",
        text:        1,
        shape:       0,
        sdf:         1,
        alpha_split: false,
    },
    PanelExpected {
        label:       "Camera",
        text:        1,
        shape:       0,
        sdf:         1,
        alpha_split: false,
    },
    PanelExpected {
        label:       "SDF",
        text:        1,
        shape:       0,
        sdf:         2,
        alpha_split: false,
    },
    PanelExpected {
        label:       "Text",
        text:        4,
        shape:       0,
        sdf:         1,
        alpha_split: true,
    },
    PanelExpected {
        label:       "Shapes",
        text:        1,
        shape:       3,
        sdf:         1,
        alpha_split: false,
    },
    PanelExpected {
        label:       "Mixed",
        text:        1,
        shape:       1,
        sdf:         1,
        alpha_split: false,
    },
    PanelExpected {
        label:       "Expected",
        text:        1,
        shape:       0,
        sdf:         1,
        alpha_split: false,
    },
];

// Per-family column colors, matching `panel_stats_block`: text green, shape
// yellow, sdf blue.
const LEDGER_FAMILY_COLORS: [Color; 3] = [ACCENT_GREEN, ACCENT_YELLOW, ACCENT_BLUE];
const LEDGER_TITLE_FONT_SIZE: f32 = 11.25;
const LEDGER_FONT_SIZE: f32 = 10.0;
const LEDGER_LABEL_WIDTH: f32 = 74.0;
const LEDGER_NUM_WIDTH: f32 = 40.0;
const LEDGER_ROW_GAP: f32 = 2.0;
const LEDGER_CELL_GAP: f32 = 4.0;
const LEDGER_TABLE_WIDTH: f32 = LEDGER_LABEL_WIDTH + 3.0 * (LEDGER_NUM_WIDTH + LEDGER_CELL_GAP);
const LEDGER_SEPARATOR_COLOR: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const CARD_RADIUS: Mm = Mm(4.0);
const PANEL_PAD: Mm = Mm(4.0);
const ROW_GAP: f32 = 4.0;
const TITLE_FONT_SIZE: f32 = 18.75;
const SUBTITLE_FONT_SIZE: f32 = 13.5;
const BODY_FONT_SIZE: f32 = 15.75;
const SMALL_FONT_SIZE: f32 = 12.75;
const MATERIAL_GROUP_TITLE_FONT_SIZE: f32 = 16.0;
const MATERIAL_CASE_FONT_SIZE: f32 = 15.0;
const MATERIAL_CASE_CAPTION_FONT_SIZE: f32 = 11.75;
const MATERIAL_GROUP_GAP: f32 = 2.0;
const MATERIAL_CASE_GAP: f32 = 0.5;
const MATERIAL_CASE_PAD_X: f32 = 2.0;
const MATERIAL_CASE_PAD_Y: f32 = 1.0;
const MATERIAL_VALUE_GAP: f32 = 2.0;
const STATS_FONT_SIZE: f32 = 12.0;
const SWATCH_FONT_SIZE: f32 = 31.5;
const MIXED_LABEL_WIDTH: f32 = 58.0;
const MIXED_ROW_BG: Color = Color::srgba(0.14, 0.16, 0.20, 0.92);
const MATERIAL_CASE_BG: Color = Color::srgba(0.10, 0.11, 0.13, 0.72);
const CARD_BG: Color = Color::srgba(0.055, 0.065, 0.075, 0.94);
const CARD_BG_ALT: Color = Color::srgba(0.075, 0.055, 0.075, 0.94);
const CARD_BORDER: Color = Color::srgba(0.34, 0.56, 0.72, 0.75);
const CARD_BORDER_WARM: Color = Color::srgba(0.84, 0.56, 0.26, 0.78);
const ACCENT_BLUE: Color = Color::srgb(0.24, 0.62, 0.95);
const ACCENT_GREEN: Color = Color::srgb(0.32, 0.88, 0.54);
const ACCENT_YELLOW: Color = Color::srgb(0.95, 0.78, 0.24);
const ACCENT_RED: Color = Color::srgb(0.95, 0.34, 0.30);
// Over-bright warm readout. Base color is a material-table value, so boosting it
// past 1.0 keeps the run in the Shared group's single batch (unlike `Unlit`,
// which the plan classifies as a pipeline splitter).
const EMISSIVE_WARM: Color = Color::linear_rgb(3.6, 2.3, 0.2);
// Sharp cool-white glint. `metallic` is a material-table value, so this row stays
// in the Shared group's single batch. Kept at full range (<=1.0) so it reads as a
// crisp highlight without crossing the bloom threshold the warm readout owns.
const GLINT: Color = Color::linear_rgb(0.95, 0.97, 1.0);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.92, 0.96);
const TEXT_MUTED: Color = Color::srgba(0.64, 0.70, 0.78, 0.9);
// Light cell for the live alpha case: `AlphaMode::Multiply` multiplies its ink
// into the background and vanishes on dark, so the selector's run sits on a
// light swatch where multiply reads as a tint. Caption and ink go dark to match.
const ALPHA_CELL_BG: Color = Color::srgb(0.82, 0.84, 0.88);
const ALPHA_CELL_INK: Color = Color::srgb(0.10, 0.12, 0.16);
const ALPHA_CELL_CAPTION: Color = Color::srgba(0.28, 0.32, 0.40, 0.95);

// The alpha modes the center-left selector cycles through, in number-key order
// (Digit1..Digit7). The selected mode is applied to the SDF panel's fills and
// borders and to the Text panel's live alpha case, so both render in the same
// mode and the per-mode shadow behavior is observable on the ground plane.
const ALPHA_MODES: [(&str, AlphaMode); 7] = [
    ("Opaque", AlphaMode::Opaque),
    ("Blend", AlphaMode::Blend),
    ("Mask 0.5", AlphaMode::Mask(0.5)),
    ("Premultiplied", AlphaMode::Premultiplied),
    ("Add", AlphaMode::Add),
    ("Multiply", AlphaMode::Multiply),
    ("AlphaToCoverage", AlphaMode::AlphaToCoverage),
];
// Blend is the panel default, so the selector starts there and nothing changes
// until a number is pressed.
const ALPHA_DEFAULT_INDEX: usize = 1;
const ALPHA_KEYS: [KeyCode; 7] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
];
const ALPHA_ROW_GAP: f32 = 2.0;
const ALPHA_ROW_WIDTH: f32 = 120.0;
const ALPHA_SELECTED_BG: Color = Color::srgba(0.24, 0.62, 0.95, 0.22);

/// The alpha mode the SDF panel fills/borders and the Text panel alpha case
/// currently render in, chosen from [`ALPHA_MODES`] by the center-left selector.
#[derive(Resource)]
struct AlphaModeSelection {
    index: usize,
}

impl Default for AlphaModeSelection {
    fn default() -> Self {
        Self {
            index: ALPHA_DEFAULT_INDEX,
        }
    }
}

impl AlphaModeSelection {
    const fn mode(&self) -> AlphaMode { ALPHA_MODES[self.index].1 }
}

/// Authored draw counts for one panel, split by render family. Drives both the
/// panel's own upper-right readout and the summed global "authored content"
/// totals.
#[derive(Clone, Copy)]
struct PanelStats {
    /// Element backgrounds, each one authored SDF fill surface.
    sdf_fills:        usize,
    /// Element borders, each one authored SDF border surface.
    sdf_borders:      usize,
    /// Authored SDF fill/border material-table rows for this panel.
    material_slots:   usize,
    /// Short share/split text drawn by `panel_stats_block`.
    readout_reason:   &'static str,
    /// `builder.text` runs.
    text_runs:        usize,
    /// Panel-shape `PathRenderRecord` rows predicted for this panel.
    shape_records:    usize,
    /// Total stroked/filled marks across all shape groups.
    shape_primitives: usize,
}

impl PanelStats {
    /// Fills plus borders: the panel's total authored SDF surfaces.
    const fn sdf_surfaces(self) -> usize { self.sdf_fills + self.sdf_borders }

    /// SDF surfaces, text runs, and analytic path groups rendered by this panel.
    const fn rendered_records(self) -> usize {
        self.sdf_surfaces() + self.text_runs + self.shape_records
    }
}

#[derive(Component)]
struct BatchValidationPanel;

/// Marker for the SDF material-value animation panel.
#[derive(Component)]
struct BatchValidationSdfPanel {
    /// Texture handle reused by the animated SDF material cases.
    image: Handle<Image>,
}

/// Registered SDF source material handles reused by the animated validation panel.
#[derive(Clone, Resource)]
struct SdfSurfaceMaterialHandles {
    /// Source material for the panel-default card.
    panel_default: Handle<StandardMaterial>,
    /// Source material for the metallic animated card.
    metallic:      Handle<StandardMaterial>,
    /// Source material for the emissive animated card.
    emissive:      Handle<StandardMaterial>,
    /// Source material for the texture-backed card.
    image:         Handle<StandardMaterial>,
}

impl SdfSurfaceMaterialHandles {
    /// Registers the four SDF card materials once and returns their handles.
    fn new(
        materials: &mut Assets<StandardMaterial>,
        image: Handle<Image>,
        alpha: AlphaMode,
    ) -> Self {
        Self {
            panel_default: materials.add(panel_default_card_material(alpha)),
            metallic:      materials.add(with_alpha(metallic_glint_material(0.0), alpha)),
            emissive:      materials.add(with_alpha(emissive_fill_material(0.0), alpha)),
            image:         materials.add(with_alpha(image_fill_material(image), alpha)),
        }
    }

    /// Updates the existing material assets for the current animation frame.
    fn refresh(
        &self,
        materials: &mut Assets<StandardMaterial>,
        image: Handle<Image>,
        phase: f32,
        alpha: AlphaMode,
    ) {
        replace_material_asset(
            materials,
            &self.panel_default,
            panel_default_card_material(alpha),
        );
        replace_material_asset(
            materials,
            &self.metallic,
            with_alpha(metallic_glint_material(phase), alpha),
        );
        replace_material_asset(
            materials,
            &self.emissive,
            with_alpha(emissive_fill_material(phase), alpha),
        );
        replace_material_asset(
            materials,
            &self.image,
            with_alpha(image_fill_material(image), alpha),
        );
    }
}

/// Registered text source material handles reused by the text validation panel.
#[derive(Clone, Resource)]
struct TextPanelMaterialHandles {
    /// Panel-level default text material.
    panel_default: Handle<StandardMaterial>,
    /// Source material for the emissive scalar/vector row.
    emissive:      Handle<StandardMaterial>,
    /// Source material for the metallic scalar/vector row.
    metallic:      Handle<StandardMaterial>,
    /// Source material carrying a base-color texture for the texture split row.
    texture:       Handle<StandardMaterial>,
}

impl TextPanelMaterialHandles {
    /// Registers the text validation source materials once and returns handles.
    fn new(materials: &mut Assets<StandardMaterial>, image: Handle<Image>) -> Self {
        Self {
            panel_default: materials.add(text_panel_default_material()),
            emissive:      materials.add(text_emissive_material()),
            metallic:      materials.add(text_metallic_material()),
            texture:       materials.add(text_texture_material(image)),
        }
    }
}

/// Registered panel-shape source material handles used by the shape validation panel.
#[derive(Clone)]
struct ShapePanelMaterialHandles {
    /// Panel-level shape material default.
    panel_default: Handle<StandardMaterial>,
    /// Shape-local emissive material used through `PanelLine::material`.
    local_line:    Handle<StandardMaterial>,
    /// Scalar material used through `LineStyle::material`.
    style_line:    Handle<StandardMaterial>,
    /// Scalar material used through `PanelCircle::material`.
    circle:        Handle<StandardMaterial>,
    /// Source material whose alpha mode splits the shape batch.
    alpha_split:   Handle<StandardMaterial>,
    /// Source material whose texture resource splits the shape batch.
    texture:       Handle<StandardMaterial>,
}

impl ShapePanelMaterialHandles {
    /// Registers the panel-shape validation source materials once.
    fn new(materials: &mut Assets<StandardMaterial>, image: Handle<Image>) -> Self {
        Self {
            panel_default: materials.add(shape_panel_default_material()),
            local_line:    materials.add(shape_local_line_material()),
            style_line:    materials.add(shape_style_line_material()),
            circle:        materials.add(shape_circle_material()),
            alpha_split:   materials.add(shape_alpha_split_material()),
            texture:       materials.add(shape_texture_material(image)),
        }
    }
}

/// Marker for the Text material panel, rebuilt when the alpha selection changes
/// so its live alpha case re-renders in the chosen mode.
#[derive(Component)]
struct BatchValidationTextPanel;

/// Marker for the center-left alpha-mode selector panel.
#[derive(Component)]
struct AlphaSelectorPanel;

#[derive(Component)]
struct BatchValidationStatsPanel;

#[derive(Component)]
struct BatchValidationLedgerPanel;

#[derive(Resource, Default)]
struct LastDisplayedStats {
    key: String,
}

// Outcome of checking the predicted batch totals against the live renderer
// counts. `Stabilizing` holds until the observed batch totals stay unchanged for
// `VALIDATION_STABLE_FRAMES` consecutive frames; the latch re-arms whenever the
// alpha selection changes.
#[derive(Resource, Default)]
struct ValidationStatus {
    state:         ValidationState,
    last_observed: Option<[usize; 3]>,
    stable_frames: u32,
    last_alpha:    usize,
}

#[derive(Default, PartialEq, Eq)]
enum ValidationState {
    #[default]
    Stabilizing,
    Match,
    Mismatch {
        expected: [usize; 3],
        observed: [usize; 3],
    },
}

const VALIDATION_STABLE_FRAMES: u32 = 30;

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_hdr()
        .with_perf_mode()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_orbit_cam_preset(
            |cam| {
                cam.focus = HOME_FOCUS;
                cam.radius = Some(HOME_RADIUS);
                cam.yaw = Some(0.0);
                cam.pitch = Some(HOME_PITCH);
            },
            OrbitCamPreset::blender_like(),
        )
        .with_stable_transparency()
        .with_bloom()
        .with_environment_map()
        .with_camera_home()
        .yaw(0.0)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .anchor(Anchor::CenterLeft)
        .with_title_bar(
            TitleBar::new()
                .with_title("Batch Validation")
                .with_anchor(Anchor::TopLeft),
        )
        .with_camera_control_panel()
        .with_shortcut(ALPHA_KEYS[0], |mut sel: ResMut<AlphaModeSelection>| {
            sel.index = 0;
        })
        .with_shortcut(ALPHA_KEYS[1], |mut sel: ResMut<AlphaModeSelection>| {
            sel.index = 1;
        })
        .with_shortcut(ALPHA_KEYS[2], |mut sel: ResMut<AlphaModeSelection>| {
            sel.index = 2;
        })
        .with_shortcut(ALPHA_KEYS[3], |mut sel: ResMut<AlphaModeSelection>| {
            sel.index = 3;
        })
        .with_shortcut(ALPHA_KEYS[4], |mut sel: ResMut<AlphaModeSelection>| {
            sel.index = 4;
        })
        .with_shortcut(ALPHA_KEYS[5], |mut sel: ResMut<AlphaModeSelection>| {
            sel.index = 5;
        })
        .with_shortcut(ALPHA_KEYS[6], |mut sel: ResMut<AlphaModeSelection>| {
            sel.index = 6;
        })
        .init_resource::<LastDisplayedStats>()
        .init_resource::<AlphaModeSelection>()
        .init_resource::<ValidationStatus>()
        .add_systems(
            Startup,
            (
                spawn_validation_panels,
                spawn_stats_panel,
                spawn_expected_batches_panel,
                spawn_alpha_selector_panel,
            ),
        )
        .add_systems(Update, validate_batch_counts.before(update_stats_panel))
        .add_systems(Update, update_stats_panel)
        .add_systems(Update, animate_sdf_surface_panel)
        .add_systems(Update, apply_alpha_selection)
        .run();
}

fn spawn_validation_panels(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    selection: Res<AlphaModeSelection>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sdf_fill_image = asset_server.load("textures/array_texture.png");
    let alpha = selection.mode();
    let sdf_materials =
        SdfSurfaceMaterialHandles::new(&mut materials, sdf_fill_image.clone(), alpha);
    let text_materials = TextPanelMaterialHandles::new(&mut materials, sdf_fill_image.clone());
    let shape_materials = ShapePanelMaterialHandles::new(&mut materials, sdf_fill_image.clone());
    commands.insert_resource(sdf_materials.clone());
    commands.insert_resource(text_materials.clone());
    let panels = [
        (
            "sdf-surfaces",
            build_sdf_surface_panel(&sdf_materials),
            None,
            None,
        ),
        (
            "text-materials",
            build_text_panel(&text_materials, alpha),
            Some(text_materials.panel_default.clone()),
            None,
        ),
        (
            "analytic-shapes",
            build_shape_panel(&shape_materials),
            None,
            Some(shape_materials.panel_default.clone()),
        ),
        ("mixed-stack", build_mixed_panel(), None, None),
    ];
    for (index, (name, tree, text_material, shape_material)) in panels.into_iter().enumerate() {
        let (x, y) = panel_grid_position(index);
        let panel = validation_panel(tree, index, text_material, shape_material, &mut materials);
        match panel {
            Ok(panel) => {
                let mut entity = commands.spawn((
                    Name::new(format!("batch validation {name}")),
                    BatchValidationPanel,
                    CameraHomeTarget,
                    panel,
                    Transform::from_xyz(x, y, 0.0),
                ));
                if index == 0 {
                    entity.insert(BatchValidationSdfPanel {
                        image: sdf_fill_image.clone(),
                    });
                }
                if index == 1 {
                    entity.insert(BatchValidationTextPanel);
                }
            },
            Err(error) => error!("batch_validation: failed to build {name}: {error}"),
        }
    }
    commands.spawn((
        CameraHomeTarget,
        Aabb::from_min_max(
            Vec3::new(
                -PANEL_STEP_X.mul_add(0.5, PANEL_WORLD_W.mul_add(0.5, PANEL_HOME_PAD)),
                PANEL_WORLD_H.mul_add(
                    -0.5,
                    PANEL_STEP_Y.mul_add(-0.5, PANEL_GRID_CENTER_Y) - PANEL_HOME_PAD,
                ),
                -PANEL_HOME_PAD,
            ),
            Vec3::new(
                PANEL_STEP_X.mul_add(0.5, PANEL_WORLD_W.mul_add(0.5, PANEL_HOME_PAD)),
                PANEL_WORLD_H.mul_add(
                    0.5,
                    PANEL_STEP_Y.mul_add(0.5, PANEL_GRID_CENTER_Y) + PANEL_HOME_PAD,
                ),
                PANEL_HOME_PAD,
            ),
        ),
        Transform::default(),
    ));
    // Small point light in front of the metallic center card, offset up-left so
    // its specular reflection localizes into a glint on that card. Studio lights
    // are broad and only produce a uniform sheen on a flat metal facing the
    // camera; a point light's direction varies across the surface, so the
    // highlight lands as a hot spot.
    commands.spawn((
        Name::new("glint key light"),
        PointLight {
            intensity: GLINT_LIGHT_LUMENS,
            range: 0.3,
            radius: 0.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(GLINT_LIGHT_POS),
    ));
}

fn animate_sdf_surface_panel(
    time: Res<Time>,
    selection: Res<AlphaModeSelection>,
    sdf_materials: Res<SdfSurfaceMaterialHandles>,
    panels: Query<&BatchValidationSdfPanel>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let phase = time.elapsed_secs() * SDF_ANIMATION_SPEED;
    let alpha = selection.mode();
    for panel in &panels {
        sdf_materials.refresh(&mut materials, panel.image.clone(), phase, alpha);
    }
}

// Repaints the center-left selector (highlighting the chosen mode) and rebuilds
// the Text panel's live alpha case whenever a number key changes the selection.
// The SDF panel needs no rebuild here: `animate_sdf_surface_panel` edits the
// existing material assets, and the fill producer reads those assets each frame.
fn apply_alpha_selection(
    selection: Res<AlphaModeSelection>,
    text_materials: Res<TextPanelMaterialHandles>,
    selectors: Query<Entity, With<AlphaSelectorPanel>>,
    text_panels: Query<Entity, With<BatchValidationTextPanel>>,
    mut commands: Commands,
) {
    if !selection.is_changed() {
        return;
    }
    for entity in &selectors {
        commands.set_tree(entity, alpha_selector_tree(selection.index));
    }
    for entity in &text_panels {
        commands.set_tree(entity, build_text_panel(&text_materials, selection.mode()));
    }
}

fn panel_grid_position(index: usize) -> (f32, f32) {
    let column = (index % 2).to_f32();
    let row = (index / 2).to_f32();
    let x = (column - 0.5) * PANEL_STEP_X;
    let y = (0.5 - row).mul_add(PANEL_STEP_Y, PANEL_GRID_CENTER_Y);
    (x, y)
}

fn validation_panel(
    tree: LayoutTree,
    index: usize,
    text_material: Option<Handle<StandardMaterial>>,
    shape_material: Option<Handle<StandardMaterial>>,
    materials: &mut Assets<StandardMaterial>,
) -> Result<DiegeticPanel, bevy_diegetic::PanelBuildError> {
    let mut material = default_panel_material();
    material.base_color = if index.is_multiple_of(2) {
        CARD_BG
    } else {
        CARD_BG_ALT
    };
    let builder = DiegeticPanel::world()
        .size(Mm(PANEL_W), Mm(PANEL_H))
        .anchor(Anchor::Center)
        .surface_shadow(SurfaceShadow::On);
    let builder = match text_material {
        Some(material) => builder.text_material(material),
        None => builder,
    };
    let builder = match shape_material {
        Some(material) => builder.shape_material(material),
        None => builder,
    };
    if index == 3 {
        builder.with_tree(tree).build()
    } else {
        let material = materials.add(material);
        builder.material(material).with_tree(tree).build()
    }
}

fn spawn_stats_panel(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let sections = validation_stats_sections(None, 0.0, 0.0);
    match diegetic_stats_sections_panel(&sections, &mut materials) {
        Ok(panel) => {
            commands.spawn((BatchValidationStatsPanel, panel, Transform::default()));
        },
        Err(error) => error!("batch_validation: failed to build stats panel: {error}"),
    }
}

fn spawn_expected_batches_panel(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    match build_expected_batches_panel(
        None,
        ALPHA_DEFAULT_INDEX,
        &ValidationState::Stabilizing,
        &mut materials,
    ) {
        Ok(panel) => {
            commands.spawn((BatchValidationLedgerPanel, panel, Transform::default()));
        },
        Err(error) => error!("batch_validation: failed to build expected-batches panel: {error}"),
    }
}

fn spawn_alpha_selector_panel(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    match build_alpha_selector_panel(ALPHA_DEFAULT_INDEX, &mut materials) {
        Ok(panel) => {
            commands.spawn((AlphaSelectorPanel, panel, Transform::default()));
        },
        Err(error) => error!("batch_validation: failed to build alpha selector panel: {error}"),
    }
}

fn build_alpha_selector_panel(
    index: usize,
    materials: &mut Assets<StandardMaterial>,
) -> Result<DiegeticPanel, bevy_diegetic::PanelBuildError> {
    let unlit = materials.add(screen_panel_material());
    DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::CenterLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(alpha_selector_tree(index))
        .build()
}

// Center-left key legend: a title plus one numbered row per alpha mode. The
// selected row is tinted and sits on a highlight bar so the current choice is
// obvious; pressing the matching number key (1-7) selects that mode for the SDF
// panel's fills/borders and the Text panel's live alpha case.
fn alpha_selector_tree(selected: usize) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    screen_panel_frame(
        &mut builder,
        Sizing::FIT,
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .gap(ALPHA_ROW_GAP),
                |builder| {
                    builder.text("alpha mode  (1-7)", ledger_title_style());
                    for (slot, (label, _)) in ALPHA_MODES.iter().enumerate() {
                        alpha_selector_row(builder, slot, label, slot == selected);
                    }
                },
            );
        },
    );
    builder.build()
}

fn alpha_selector_row(builder: &mut LayoutBuilder, slot: usize, label: &str, selected: bool) {
    let number = slot + 1;
    let color = if selected { ACCENT_GREEN } else { TEXT_MUTED };
    let mut row = El::row()
        .width(Sizing::fixed(ALPHA_ROW_WIDTH))
        .height(Sizing::FIT)
        .gap(LEDGER_CELL_GAP)
        .padding(Padding::new(3.0, 3.0, 1.0, 1.0))
        .corner_radius(CornerRadius::all(Mm(0.8)))
        .alignment(AlignX::Left, AlignY::Center);
    if selected {
        row = row.background(ALPHA_SELECTED_BG);
    }
    builder.with(row, |builder| {
        builder.text(format!("{number}"), ledger_cell_style(color));
        builder.text(label, ledger_cell_style(color));
    });
}

// Latches the live batch totals: once `[text, shape, sdf]` batch counts hold
// steady for `VALIDATION_STABLE_FRAMES` frames, compares them against the
// predicted totals for the active alpha mode and records match/mismatch. SDF
// material animation changes table values, not batch counts, so it never
// disturbs the latch. Re-arms when the alpha selection changes.
fn validate_batch_counts(
    perf: Res<DiegeticPerfStats>,
    selection: Res<AlphaModeSelection>,
    mut status: ResMut<ValidationStatus>,
) {
    let observed = [
        perf.batch.batches,
        perf.line_batch.batches,
        perf.panel_geometry.sdf_batches,
    ];
    if selection.index != status.last_alpha {
        status.last_alpha = selection.index;
        status.state = ValidationState::Stabilizing;
        status.last_observed = None;
        status.stable_frames = 0;
    }
    // A zero in any family means the renderer has not populated that count yet;
    // hold in `Stabilizing` rather than latch onto a partial frame.
    if observed.contains(&0) {
        status.state = ValidationState::Stabilizing;
        status.last_observed = None;
        status.stable_frames = 0;
        return;
    }
    if status.last_observed == Some(observed) {
        status.stable_frames += 1;
    } else {
        status.last_observed = Some(observed);
        status.stable_frames = 0;
        status.state = ValidationState::Stabilizing;
    }
    if status.stable_frames < VALIDATION_STABLE_FRAMES {
        return;
    }
    let expected = expected_totals(selection.index);
    status.state = if observed == expected {
        ValidationState::Match
    } else {
        ValidationState::Mismatch { expected, observed }
    };
}

fn update_stats_panel(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    diegetic_perf: Res<DiegeticPerfStats>,
    selection: Res<AlphaModeSelection>,
    validation: Res<ValidationStatus>,
    panels: Query<Entity, With<BatchValidationStatsPanel>>,
    ledger_panels: Query<Entity, With<BatchValidationLedgerPanel>>,
    mut last: ResMut<LastDisplayedStats>,
    mut commands: Commands,
    mut timer: Local<Option<Timer>>,
) {
    let timer =
        timer.get_or_insert_with(|| Timer::from_seconds(FPS_UPDATE_INTERVAL, TimerMode::Repeating));
    timer.tick(time.delta());
    if !timer.just_finished() {
        return;
    }

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(Diagnostic::smoothed);
    let sections =
        validation_stats_sections(Some(&diegetic_perf), fps.unwrap_or(0.0), time.delta_secs());
    let key = format!(
        "{}|alpha={}|val={}",
        stats_key(&sections),
        selection.index,
        validation_key(&validation.state)
    );
    if key == last.key {
        return;
    }
    last.key = key;
    for panel in &panels {
        commands.set_tree(panel, diegetic_stats_sections_tree(&sections));
    }
    for panel in &ledger_panels {
        commands.set_tree(
            panel,
            expected_batches_tree(Some(&diegetic_perf), selection.index, &validation.state),
        );
    }
}

// Short discriminant for the dedup key so the ledger rebuilds when the
// validation outcome changes even after the live counts stop moving.
fn validation_key(state: &ValidationState) -> String {
    match state {
        ValidationState::Stabilizing => "stabilizing".to_owned(),
        ValidationState::Match => "match".to_owned(),
        ValidationState::Mismatch { expected, observed } => {
            format!("mismatch:{expected:?}:{observed:?}")
        },
    }
}

fn validation_stats_sections(
    perf: Option<&DiegeticPerfStats>,
    fps: f64,
    frame_secs: f32,
) -> Vec<StatsPanelSection> {
    let perf = perf.cloned().unwrap_or_default();
    let batch = &perf.batch;
    let shape_batch = perf.line_batch;
    let sdf = perf.panel_geometry;
    let material_table = perf.material_table;
    let current_draws = sdf.sdf_batches + batch.batches + shape_batch.batches;
    vec![
        StatsPanelSection::new(
            "frame",
            [
                StatsPanelRow::new(
                    "profile",
                    if cfg!(debug_assertions) {
                        "debug"
                    } else {
                        "release"
                    },
                ),
                StatsPanelRow::new("fps", format!("{fps:.0}"))
                    .detail(format!("frame: {:.2} ms", frame_secs * 1000.0)),
            ],
        ),
        StatsPanelSection::new(
            "authored content",
            [
                StatsPanelRow::new("panels", AUTHORED_PANELS.to_string())
                    .detail("four world panels, each a different batching case"),
                StatsPanelRow::new("sdf fills", AUTHORED_SDF_FILLS.to_string()),
                StatsPanelRow::new("sdf borders", AUTHORED_SDF_BORDERS.to_string())
                    .detail("fills and borders should become SDF batch records"),
                StatsPanelRow::new("text runs", AUTHORED_TEXT_RUNS.to_string()),
                StatsPanelRow::new("shape records", AUTHORED_SHAPE_RECORDS.to_string()),
                StatsPanelRow::new("shape primitives", AUTHORED_SHAPE_PRIMITIVES.to_string())
                    .detail("lines, arrows, circles, dividers"),
            ],
        ),
        StatsPanelSection::new(
            "observed renderer",
            [
                StatsPanelRow::new("sdf batches", sdf.sdf_batches.to_string()),
                StatsPanelRow::new("sdf records", sdf.sdf_records.to_string()),
                StatsPanelRow::new("text batches", batch.batches.to_string()),
                StatsPanelRow::new("text runs", batch.runs.to_string()),
                StatsPanelRow::new("text glyphs", batch.glyph_records.to_string())
                    .detail("live glyph instance records"),
                StatsPanelRow::new(
                    "text uploads",
                    (batch.instance_uploads + batch.run_table_uploads).to_string(),
                )
                .detail(format!(
                    "instance {} + run-table {}",
                    batch.instance_uploads, batch.run_table_uploads
                )),
                StatsPanelRow::new("shape batches", shape_batch.batches.to_string()),
                StatsPanelRow::new("path records", shape_batch.records.to_string()),
                StatsPanelRow::new("shape uploads", shape_batch.uploads.to_string()),
                StatsPanelRow::new("draws today", current_draws.to_string()).detail(format!(
                    "sdf {} + text {} + shapes {}",
                    sdf.sdf_batches, batch.batches, shape_batch.batches
                )),
            ],
        ),
        StatsPanelSection::new(
            "material table",
            [
                StatsPanelRow::new("sdf batches", sdf.sdf_batches.to_string())
                    .detail("scalar/vector materials share; texture resource splits"),
                StatsPanelRow::new("sdf records", sdf.sdf_records.to_string()),
                StatsPanelRow::new("sdf uploads", sdf.sdf_uploads.to_string()),
                StatsPanelRow::new("table rows", material_table.rows.to_string()),
                StatsPanelRow::new("table bytes", material_table.upload_bytes.to_string()),
                StatsPanelRow::new("table capacity", material_table.capacity.to_string())
                    .detail("row capacity stays steady during SDF material animation"),
            ],
        ),
    ]
}

fn stats_key(sections: &[StatsPanelSection]) -> String {
    let mut key = String::new();
    for section in sections {
        key.push_str(&section.title);
        key.push('|');
        for row in &section.rows {
            key.push_str(&row.label);
            key.push('=');
            key.push_str(&row.value);
            key.push('|');
            for detail in &row.details {
                key.push_str(detail);
                key.push('|');
            }
        }
    }
    key
}

fn build_expected_batches_panel(
    perf: Option<&DiegeticPerfStats>,
    alpha_index: usize,
    status: &ValidationState,
    materials: &mut Assets<StandardMaterial>,
) -> Result<DiegeticPanel, bevy_diegetic::PanelBuildError> {
    let unlit = materials.add(screen_panel_material());
    DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(expected_batches_tree(perf, alpha_index, status))
        .build()
}

// The bottom-left ledger: a title, a header row, one row per panel with its
// predicted text/shape/sdf batch counts for the active alpha mode, then a
// `predicted` totals row, a live `actual` row from the renderer counters, and a
// validation status line latched by `validate_batch_counts`.
fn expected_batches_tree(
    perf: Option<&DiegeticPerfStats>,
    alpha_index: usize,
    status: &ValidationState,
) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    screen_panel_frame(
        &mut builder,
        Sizing::FIT,
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .gap(LEDGER_ROW_GAP),
                |builder| {
                    builder.text("expected batches", ledger_title_style());
                    ledger_row(
                        builder,
                        "",
                        TEXT_MUTED,
                        ["text".to_owned(), "shape".to_owned(), "sdf".to_owned()],
                    );
                    for panel in &EXPECTED_BATCHES {
                        ledger_row(
                            builder,
                            panel.label,
                            TEXT_MUTED,
                            [
                                panel_text_for_mode(panel, alpha_index).to_string(),
                                panel.shape.to_string(),
                                panel.sdf.to_string(),
                            ],
                        );
                    }
                    let totals = expected_totals(alpha_index);
                    ledger_separator(builder);
                    ledger_row(
                        builder,
                        "predicted",
                        TEXT_MAIN,
                        [
                            totals[0].to_string(),
                            totals[1].to_string(),
                            totals[2].to_string(),
                        ],
                    );
                    let perf = perf.cloned().unwrap_or_default();
                    ledger_row(
                        builder,
                        "actual",
                        TEXT_MAIN,
                        [
                            perf.batch.batches.to_string(),
                            perf.line_batch.batches.to_string(),
                            perf.panel_geometry.sdf_batches.to_string(),
                        ],
                    );
                    ledger_separator(builder);
                    let (status_text, status_color) = match status {
                        ValidationState::Stabilizing => ("stabilizing…".to_owned(), TEXT_MUTED),
                        ValidationState::Match => ("validation: match".to_owned(), ACCENT_GREEN),
                        ValidationState::Mismatch { expected, observed } => (
                            format!("mismatch predicted {expected:?} actual {observed:?}"),
                            ACCENT_RED,
                        ),
                    };
                    builder.text(status_text, ledger_cell_style(status_color));
                },
            );
        },
    );
    builder.build()
}

// One table row: a fixed-width left label cell plus three right-aligned numeric
// cells colored by family (text/shape/sdf).
fn ledger_row(builder: &mut LayoutBuilder, label: &str, label_color: Color, cells: [String; 3]) {
    builder.with(
        El::row()
            .width(Sizing::fixed(LEDGER_TABLE_WIDTH))
            .height(Sizing::FIT)
            .gap(LEDGER_CELL_GAP)
            .alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(LEDGER_LABEL_WIDTH))
                    .height(Sizing::FIT)
                    .alignment(AlignX::Left, AlignY::Center),
                |builder| {
                    builder.text(label, ledger_cell_style(label_color));
                },
            );
            for (cell, color) in cells.into_iter().zip(LEDGER_FAMILY_COLORS) {
                builder.with(
                    El::new()
                        .width(Sizing::fixed(LEDGER_NUM_WIDTH))
                        .height(Sizing::FIT)
                        .alignment(AlignX::Right, AlignY::Center),
                    |builder| {
                        builder.text(cell, ledger_cell_style(color));
                    },
                );
            }
        },
    );
}

fn ledger_separator(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::fixed(LEDGER_TABLE_WIDTH))
            .height(Sizing::fixed(1.0))
            .background(LEDGER_SEPARATOR_COLOR),
        |_builder| {},
    );
}

fn ledger_title_style() -> TextStyle {
    TextStyle::new(LEDGER_TITLE_FONT_SIZE)
        .bold()
        .with_color(TEXT_MAIN)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn ledger_cell_style(color: Color) -> TextStyle {
    TextStyle::new(LEDGER_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

// Four SDF fills laid out 2x2. The panel-default / metallic / emissive cards
// differ only in StandardMaterial table values, so post-material-table they
// share one SDF batch. The image card carries a
// `base_color_texture`, a batch-compatibility splitter, so it forms its own
// batch — the SDF column's predicted 2 batches in the expected-batches ledger.
fn build_sdf_surface_panel(materials: &SdfSurfaceMaterialHandles) -> LayoutTree {
    let mut builder = panel_root();
    panel_header(
        &mut builder,
        "SDF fills + borders",
        "three batch together, image splits",
        SDF_PANEL_STATS,
    );
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(ROW_GAP),
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .gap(ROW_GAP),
                |builder| {
                    sdf_panel_default_card(
                        builder,
                        "panel default",
                        "builder material",
                        ACCENT_BLUE,
                        materials.panel_default.clone(),
                    );
                    sdf_fill_card(
                        builder,
                        "El material",
                        "base color animates",
                        ACCENT_GREEN,
                        materials.metallic.clone(),
                    );
                },
            );
            builder.with(
                El::column()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .gap(ROW_GAP),
                |builder| {
                    sdf_fill_card(
                        builder,
                        "El material",
                        "emissive value",
                        ACCENT_YELLOW,
                        materials.emissive.clone(),
                    );
                    sdf_fill_card(
                        builder,
                        "image",
                        "texture splits",
                        ACCENT_RED,
                        materials.image.clone(),
                    );
                },
            );
        },
    );
    builder.build()
}

// Stamps the selected alpha mode onto a card material so the SDF fill and its
// border (which inherits the fill material) both render in that mode.
const fn with_alpha(mut material: StandardMaterial, alpha: AlphaMode) -> StandardMaterial {
    material.alpha_mode = alpha;
    material
}

fn text_panel_default_material() -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = TEXT_MAIN;
    material.alpha_mode = AlphaMode::Blend;
    material
}

fn text_emissive_material() -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::srgb(0.08, 0.06, 0.02);
    material.emissive = EMISSIVE_WARM.to_linear() * 1.4;
    material.alpha_mode = AlphaMode::Blend;
    material
}

fn text_metallic_material() -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = GLINT;
    material.metallic = 1.0;
    material.perceptual_roughness = 0.26;
    material.reflectance = 0.8;
    material.alpha_mode = AlphaMode::Blend;
    material
}

fn text_texture_material(image: Handle<Image>) -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::WHITE;
    material.base_color_texture = Some(image);
    material.alpha_mode = AlphaMode::Blend;
    material
}

fn shape_panel_default_material() -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::srgb(0.08, 0.10, 0.13);
    material.metallic = 0.0;
    material.perceptual_roughness = 0.42;
    material.alpha_mode = AlphaMode::Blend;
    material
}

fn shape_local_line_material() -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::srgb(0.05, 0.06, 0.02);
    material.emissive = EMISSIVE_WARM.to_linear();
    material.alpha_mode = AlphaMode::Blend;
    material
}

fn shape_style_line_material() -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = GLINT;
    material.metallic = 1.0;
    material.perceptual_roughness = 0.24;
    material.reflectance = 0.8;
    material.alpha_mode = AlphaMode::Blend;
    material
}

fn shape_circle_material() -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::srgb(0.12, 0.08, 0.18);
    material.perceptual_roughness = 0.72;
    material.reflectance = 0.28;
    material.alpha_mode = AlphaMode::Blend;
    material
}

fn shape_alpha_split_material() -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = ACCENT_RED;
    material.alpha_mode = AlphaMode::Add;
    material
}

fn shape_texture_material(image: Handle<Image>) -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::WHITE;
    material.base_color_texture = Some(image);
    material.alpha_mode = AlphaMode::Blend;
    material
}

fn build_text_panel(materials: &TextPanelMaterialHandles, alpha: AlphaMode) -> LayoutTree {
    let mut builder = panel_root();
    panel_header(
        &mut builder,
        "Text material cases",
        "same batch values vs split keys",
        TEXT_PANEL_STATS,
    );
    builder.with(
        // GROW height: claim the panel's leftover vertical space below the
        // header so the two group columns (also GROW height) stretch to a
        // matching height instead of each hugging its own content.
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(ROW_GAP),
        |builder| {
            shared_text_material_group(builder, materials);
            divergent_group(builder, materials, alpha);
        },
    );
    builder.build()
}

fn build_shape_panel(materials: &ShapePanelMaterialHandles) -> LayoutTree {
    let mut builder = panel_root();
    panel_header(
        &mut builder,
        "Analytic shapes",
        "source materials share or split",
        SHAPE_PANEL_STATS,
    );
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(ROW_GAP),
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .gap(ROW_GAP),
                |builder| {
                    shape_group_card(
                        builder,
                        materials,
                        0,
                        "panel default",
                        "shape_material",
                        ACCENT_BLUE,
                    );
                    shape_group_card(
                        builder,
                        materials,
                        1,
                        "line local",
                        "PanelLine::material",
                        ACCENT_GREEN,
                    );
                    shape_group_card(
                        builder,
                        materials,
                        2,
                        "line style",
                        "LineStyle::material",
                        ACCENT_YELLOW,
                    );
                },
            );
            builder.with(
                El::column()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .gap(ROW_GAP),
                |builder| {
                    shape_group_card(
                        builder,
                        materials,
                        3,
                        "circle local",
                        "PanelCircle::material",
                        ACCENT_BLUE,
                    );
                    shape_group_card(builder, materials, 4, "alpha split", "Add mode", ACCENT_RED);
                    shape_group_card(
                        builder,
                        materials,
                        5,
                        "texture split",
                        "texture resource splits",
                        ACCENT_GREEN,
                    );
                },
            );
        },
    );
    builder.build()
}

fn build_mixed_panel() -> LayoutTree {
    let mut builder = panel_root();
    panel_header(
        &mut builder,
        "Mixed stack",
        "one panel, several draw families",
        MIXED_PANEL_STATS,
    );
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(2.0),
        |builder| {
            mixed_row(
                builder,
                "SDF surface",
                "global default",
                "2 records",
                ACCENT_BLUE,
                Border::all(Mm(0.4), ACCENT_BLUE),
            );
            mixed_row(
                builder,
                "Text run A",
                "shared material",
                "1 run",
                ACCENT_GREEN,
                Border::new(),
            );
            mixed_row(
                builder,
                "Text run B",
                "different color",
                "same batch",
                ACCENT_YELLOW,
                Border::new(),
            );
            mixed_shape_row(
                builder,
                "Shape default",
                "global/default",
                "3 primitives",
                ACCENT_RED,
            );
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .padding(Padding::new(4.0, 4.0, 4.0, 4.0))
                    .background(MIXED_ROW_BG)
                    .alignment(AlignX::Left, AlignY::Center),
                |builder| {
                    builder.text("Expected: SDF + text + path batches", body_style(TEXT_MAIN));
                },
            );
        },
    );
    builder.build()
}

fn panel_root() -> LayoutBuilder {
    LayoutBuilder::with_root(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(ROW_GAP)
            .padding(Padding::all(PANEL_PAD))
            .border(Border::all(Mm(0.45), CARD_BORDER))
            .corner_radius(CornerRadius::all(CARD_RADIUS)),
    )
}

fn panel_header(builder: &mut LayoutBuilder, title: &str, subtitle: &str, stats: PanelStats) {
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(ROW_GAP)
            .padding(Padding::new(1.0, 1.0, 0.0, 2.0))
            .alignment(AlignX::Left, AlignY::Top),
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .gap(1.0),
                |builder| {
                    builder.text(title, title_style());
                    builder.text(subtitle, subtitle_style(TEXT_MUTED));
                },
            );
            panel_stats_block(builder, stats);
        },
    );
}

// The panel's own authored counts, drawn small in the upper-right corner.
// `panel_stats_block` omits per-panel batch and upload counts: `DiegeticPerfStats`
// reports observed batch/upload totals globally because batches span panels and
// uploads happen per batch buffer.
fn panel_stats_block(builder: &mut LayoutBuilder, stats: PanelStats) {
    builder.with(
        El::column()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .gap(0.5)
            .alignment(AlignX::Right, AlignY::Top),
        |builder| {
            stats_block_row(
                builder,
                &[
                    (format!("sdf {}", stats.sdf_surfaces()), ACCENT_BLUE),
                    (format!("text {}", stats.text_runs), ACCENT_GREEN),
                    (format!("shape {}", stats.shape_records), ACCENT_YELLOW),
                ],
            );
            stats_block_row(
                builder,
                &[
                    (format!("records {}", stats.rendered_records()), TEXT_MAIN),
                    (format!("slots {}", stats.material_slots), ACCENT_RED),
                ],
            );
            builder.text(stats.readout_reason, stats_style(TEXT_MUTED));
        },
    );
}

// One right-aligned line of the stats block: several short colored readouts side
// by side, so the per-panel counts stay legible in three lines instead of six.
fn stats_block_row(builder: &mut LayoutBuilder, cells: &[(String, Color)]) {
    builder.with(
        El::row()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .gap(4.0)
            .alignment(AlignX::Right, AlignY::Top),
        |builder| {
            for (text, color) in cells {
                builder.text(text.clone(), stats_style(*color));
            }
        },
    );
}

fn shared_text_material_group(builder: &mut LayoutBuilder, materials: &TextPanelMaterialHandles) {
    builder.with(
        // GROW height so both group columns fill the row and share a height; the
        // FIT cases inside keep their content from inflating the panel.
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(MATERIAL_GROUP_GAP)
            .padding(Padding::all(Mm(1.5)))
            .background(Color::srgba(0.02, 0.03, 0.04, 0.30))
            .border(Border::all(Mm(0.3), ACCENT_GREEN))
            .corner_radius(CornerRadius::all(Mm(1.3))),
        |builder| {
            material_group_header(builder, "Shared group", "varies table values");
            material_group_spacer(builder);
            material_case_block_with_style(
                builder,
                "style color",
                "cool blue",
                material_value_style(ACCENT_BLUE),
            );
            material_group_spacer(builder);
            material_case_block_with_style(
                builder,
                "local material",
                "emissive value",
                material_value_style(EMISSIVE_WARM).with_material(materials.emissive.clone()),
            );
            material_group_spacer(builder);
            material_case_block_with_style(
                builder,
                "local material",
                "metallic glint",
                material_value_style(GLINT).with_material(materials.metallic.clone()),
            );
        },
    );
}

fn material_group_header(builder: &mut LayoutBuilder, title: &str, subtitle: &str) {
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(MATERIAL_CASE_GAP),
        |builder| {
            builder.text(title, material_group_title_style());
            builder.text(subtitle, material_caption_style(TEXT_MUTED));
        },
    );
}

fn material_group_spacer(builder: &mut LayoutBuilder) {
    builder.with(
        El::new().width(Sizing::GROW).height(Sizing::GROW),
        |_builder| {},
    );
}

// Each case stacks a muted caption above its value. The value owns the full
// group width and wraps, so the long divergent-group strings cannot clip at the
// card edge the way a fixed-label-width row did.
fn material_case_block_with_style(
    builder: &mut LayoutBuilder,
    label: &str,
    value: &str,
    style: TextStyle,
) {
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(MATERIAL_CASE_GAP)
            .padding(Padding::xy(MATERIAL_CASE_PAD_X, MATERIAL_CASE_PAD_Y))
            .corner_radius(CornerRadius::all(Mm(1.5)))
            .background(MATERIAL_CASE_BG)
            .alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.text(label, material_caption_style(TEXT_MUTED));
            builder.text(value, style);
        },
    );
}

// The Divergent group authors text runs that actually carry the batch splitters,
// so each one forms its own text batch (observable in `batch.batches` / the
// ledger's `actual` text count). The texture row splits by resource; the
// cull-mode rows split by sidedness; the live alpha row splits only when the
// selector leaves the panel-default Blend mode.
fn divergent_group(
    builder: &mut LayoutBuilder,
    materials: &TextPanelMaterialHandles,
    alpha: AlphaMode,
) {
    builder.with(
        // GROW height to match the Shared group; see `shared_text_material_group`.
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(MATERIAL_GROUP_GAP)
            .padding(Padding::all(Mm(1.5)))
            .background(Color::srgba(0.02, 0.03, 0.04, 0.30))
            .border(Border::all(Mm(0.3), ACCENT_RED))
            .corner_radius(CornerRadius::all(Mm(1.3))),
        |builder| {
            material_group_header(builder, "Divergent group", "splits compatibility");
            material_group_spacer(builder);
            divergent_alpha_case(builder, alpha);
            material_group_spacer(builder);
            divergent_texture_case(builder, materials);
            material_group_spacer(builder);
            divergent_cull_case(builder);
        },
    );
}

// Live alpha-mode case: one run rendered in the mode the center-left selector
// currently holds, so the text material's alpha tracks the same selection that
// drives the SDF panel. Sits on a light cell so `Multiply` reads as a tint
// rather than vanishing into the dark panel.
fn divergent_alpha_case(builder: &mut LayoutBuilder, alpha: AlphaMode) {
    divergent_case_shell(
        builder,
        "alpha mode (selector)",
        ALPHA_CELL_BG,
        ALPHA_CELL_CAPTION,
        |builder| {
            builder.text(
                alpha_mode_label(alpha),
                material_value_style(ALPHA_CELL_INK).with_alpha_mode(alpha),
            );
        },
    );
}

// Short display name for an alpha mode, matching the selector's row labels.
fn alpha_mode_label(alpha: AlphaMode) -> &'static str {
    ALPHA_MODES
        .iter()
        .find(|(_, mode)| *mode == alpha)
        .map_or("custom", |(label, _)| *label)
}

// Texture-backed text samples the material image across the run-local box UV,
// and the texture resource forms its own compatibility batch.
fn divergent_texture_case(builder: &mut LayoutBuilder, materials: &TextPanelMaterialHandles) {
    divergent_case_shell(
        builder,
        "texture",
        MATERIAL_CASE_BG,
        TEXT_MUTED,
        |builder| {
            builder.text(
                "image glyphs",
                material_value_style(TEXT_MAIN).with_material(materials.texture.clone()),
            );
        },
    );
}

// Cull-mode split: three runs, one per `Sidedness`. `FrontOnly` and `BackOnly`
// each form their own text batch; `BothSides` matches the panel default and
// joins the shared batch. `BackOnly` culls front faces, so it is invisible from
// the front camera and only legible when the orbit camera swings behind.
fn divergent_cull_case(builder: &mut LayoutBuilder) {
    divergent_case_shell(
        builder,
        "cull mode",
        MATERIAL_CASE_BG,
        TEXT_MUTED,
        |builder| {
            builder.text(
                "FrontOnly",
                material_value_style(ACCENT_RED).with_sidedness(Sidedness::FrontOnly),
            );
            builder.text(
                "BackOnly",
                material_value_style(ACCENT_RED).with_sidedness(Sidedness::BackOnly),
            );
            builder.text(
                "BothSides",
                material_value_style(ACCENT_RED).with_sidedness(Sidedness::BothSides),
            );
        },
    );
}

// A divergent case: a captioned cell whose value runs wrap in a row, so each
// run keeps its own `TextStyle` (and thus its own batch key) instead of being
// merged into one descriptive string.
fn divergent_case_shell(
    builder: &mut LayoutBuilder,
    caption: &str,
    cell_bg: Color,
    caption_color: Color,
    values: impl FnOnce(&mut LayoutBuilder),
) {
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(MATERIAL_CASE_GAP)
            .padding(Padding::xy(MATERIAL_CASE_PAD_X, MATERIAL_CASE_PAD_Y))
            .corner_radius(CornerRadius::all(Mm(1.5)))
            .background(cell_bg)
            .alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.text(caption, material_caption_style(caption_color));
            // FIT width, not GROW: a GROW-width row seeds to ~0 width in the
            // bottom-up fit pass, forcing its text runs to wrap per word and
            // measure tall, which balloons the case height. FIT measures each
            // run at its intrinsic single-line width and packs them left.
            builder.with(
                El::row()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .gap(MATERIAL_VALUE_GAP),
                values,
            );
        },
    );
}

// One SDF fill: an El with a material background and a border (rendered in a
// single quad), captioned with its material name and role. The label/caption
// column carries no background, so it adds no SDF surface — only the outer El.
fn sdf_fill_card(
    builder: &mut LayoutBuilder,
    label: &str,
    caption: &str,
    accent: Color,
    material: Handle<StandardMaterial>,
) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(2.0))
            .material(material)
            .border(Border::all(Mm(0.3), accent))
            .corner_radius(CornerRadius::all(Mm(1.4)))
            .alignment(AlignX::Center, AlignY::Center),
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .gap(1.0)
                    .alignment(AlignX::Center, AlignY::Center),
                |builder| {
                    builder.text(label, swatch_style(accent));
                    builder.text(caption, small_style(TEXT_MUTED));
                },
            );
        },
    );
}

fn sdf_panel_default_card(
    builder: &mut LayoutBuilder,
    label: &str,
    caption: &str,
    accent: Color,
    material: Handle<StandardMaterial>,
) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(2.0))
            .material(material)
            .border(Border::all(Mm(0.3), accent))
            .corner_radius(CornerRadius::all(Mm(1.4)))
            .alignment(AlignX::Center, AlignY::Center),
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .gap(1.0)
                    .alignment(AlignX::Center, AlignY::Center),
                |builder| {
                    builder.text(label, swatch_style(accent));
                    builder.text(caption, small_style(TEXT_MUTED));
                },
            );
        },
    );
}

fn panel_default_card_material(alpha: AlphaMode) -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = ACCENT_BLUE;
    material.alpha_mode = alpha;
    material
}

fn replace_material_asset(
    materials: &mut Assets<StandardMaterial>,
    handle: &Handle<StandardMaterial>,
    material: StandardMaterial,
) {
    if let Some(mut existing) = materials.get_mut(handle) {
        *existing = material;
    }
}

// Matte dielectric SDF fill. Differs from the metallic and emissive cards only
// in StandardMaterial table values, so the three share a batch.
fn animated_unit(phase: f32, offset: f32) -> f32 { (phase + offset).sin().mul_add(0.5, 0.5) }

// Brushed-metal SDF fill whose base color also sweeps each frame, so the fill
// visibly cycles while staying batch-compatible with the panel-default and
// emissive cards: base color, metallic, roughness, and reflectance are all
// material-table values, never batch splitters. Keeps `default_panel_material`'s
// double-sided / no-cull setup so the diegetic panel renders from both faces.
fn metallic_glint_material(phase: f32) -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::srgb(
        animated_unit(phase, SDF_ANIMATION_RED_OFFSET).mul_add(0.25, 0.18),
        animated_unit(phase, SDF_ANIMATION_GREEN_OFFSET).mul_add(0.30, 0.28),
        animated_unit(phase, 0.0).mul_add(0.30, 0.50),
    );
    material.metallic = 1.0;
    material.perceptual_roughness =
        animated_unit(phase, SDF_ANIMATION_GREEN_OFFSET).mul_add(0.35, 0.35);
    material.reflectance = animated_unit(phase, SDF_ANIMATION_RED_OFFSET).mul_add(0.2, 0.7);
    material
}

// Emissive SDF fill: a warm self-lit readout color. `emissive` is a table value,
// so this stays batch-compatible with the colored and metallic cards.
fn emissive_fill_material(phase: f32) -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::srgb(0.06, 0.05, 0.03);
    let intensity = animated_unit(phase, SDF_ANIMATION_GREEN_OFFSET).mul_add(0.8, 0.6);
    material.emissive = EMISSIVE_WARM.to_linear() * intensity;
    material
}

// Image SDF fill: a white base color modulated by `base_color_texture`, sampled
// over the quad UV. The texture handle is a batch-compatibility splitter, so this
// card forms its own SDF batch separate from the three table-value cards.
fn image_fill_material(image: Handle<Image>) -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::WHITE;
    material.base_color_texture = Some(image.clone());
    material.emissive_texture = Some(image);
    material.emissive = LinearRgba::WHITE;
    material
}

// Each card draws only the shape its label names. The shapes live in their own
// strip element to the right of the label, so they resolve against that
// element's local space: every row spans `start(3)` to `end(6)`, vertically
// centered, which makes all three lines the same length regardless of width.
fn shape_strip(
    materials: &ShapePanelMaterialHandles,
    index: usize,
    color: Color,
) -> Vec<bevy_diegetic::PanelShape> {
    // A fresh line spanning the strip width, built per row because `PanelLine`
    // and `PanelPoint` move on each builder call. The end inset varies: a
    // centered end cap (circle/diamond) extends half its size past its end
    // point, but the arrowhead tip lands on its end point, so the arrow's line
    // ends further right to reach the same rightmost point.
    let span = |end_inset: f32| {
        PanelLine::new(
            PanelPoint::new(PanelCoord::start(Mm(3.0)), PanelCoord::percent(0.5)),
            PanelPoint::new(PanelCoord::end(Mm(end_inset)), PanelCoord::percent(0.5)),
        )
        .color(color)
    };
    match index {
        // Panel-level shape material default: no local source handle.
        0 => vec![
            span(3.6)
                .width(Mm(0.5))
                .end_cap(CalloutCap::arrow().solid().length(4.0).width(3.2))
                .into(),
        ],
        // Shape-local material through `PanelLine::material`.
        1 => vec![
            span(6.0)
                .width(Mm(0.45))
                .material(materials.local_line.clone())
                .end_cap(CalloutCap::circle().radius(2.4))
                .into(),
        ],
        // Shape-local material through `LineStyle::material`.
        2 => vec![
            PanelLine::new(
                PanelPoint::new(PanelCoord::start(Mm(3.0)), PanelCoord::percent(0.5)),
                PanelPoint::new(PanelCoord::end(Mm(6.0)), PanelCoord::percent(0.5)),
            )
            .style(
                LineStyle::default()
                    .width(Mm(0.4))
                    .color(color)
                    .material(materials.style_line.clone())
                    .start_cap(CalloutCap::circle().radius(1.8))
                    .end_cap(CalloutCap::diamond().width(3.0).height(3.0)),
            )
            .into(),
        ],
        // Shape-local material through `PanelCircle::material`.
        3 => vec![
            PanelCircle::new(
                PanelPoint::new(PanelCoord::percent(0.5), PanelCoord::percent(0.5)),
                Mm(4.0),
            )
            .color(color)
            .material(materials.circle.clone())
            .into(),
        ],
        // Material alpha mode is a pipeline splitter.
        4 => vec![
            span(5.0)
                .width(Mm(0.62))
                .material(materials.alpha_split.clone())
                .into(),
        ],
        // Base-color texture samples across the shape-local 0..1 box UV.
        _ => vec![
            span(5.0)
                .width(Mm(1.0))
                .material(materials.texture.clone())
                .end_cap(CalloutCap::square().width(3.0).height(3.0))
                .into(),
        ],
    }
}

fn shape_group_card(
    builder: &mut LayoutBuilder,
    materials: &ShapePanelMaterialHandles,
    index: usize,
    label: &str,
    detail: &str,
    color: Color,
) {
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::fixed(28.0))
            .gap(ROW_GAP)
            .padding(Padding::new(4.0, 4.0, 4.0, 4.0))
            .background(Color::srgba(0.02, 0.03, 0.04, 0.42))
            .border(Border::all(Mm(0.25), CARD_BORDER_WARM))
            .corner_radius(CornerRadius::all(Mm(1.2)))
            .alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::fixed(58.0))
                    .height(Sizing::FIT)
                    .gap(1.0),
                |builder| {
                    builder.text(label, body_style(color));
                    builder.text(detail, small_style(TEXT_MUTED));
                },
            );
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .draw(PanelDraw::shapes(shape_strip(materials, index, color))),
                |_builder| {},
            );
        },
    );
}

// The Shape group row's draw family, drawn in the shape strip below the text:
// one stroked line plus a start and end cap — the three analytic primitives the
// row's "line + caps" / "3 primitives" readout describes. The circle start cap
// sits on the line's first point so it touches the stroke.
fn mixed_shape_group(color: Color) -> PanelDraw {
    PanelDraw::shapes([
        PanelLine::new(PanelPoint::new(6.0, 4.0), PanelPoint::new(146.0, 4.0))
            .width(Mm(0.5))
            .color(color)
            .end_cap(CalloutCap::arrow().solid().length(4.0).width(3.2))
            .into(),
        PanelCircle::new(PanelPoint::new(6.0, 4.0), Mm(2.0))
            .color(color)
            .into(),
    ])
}

// The label / value / count text shared by every mixed row.
fn mixed_row_body(
    builder: &mut LayoutBuilder,
    label: &str,
    value: &str,
    count: &str,
    color: Color,
) {
    builder.with(
        El::new()
            .width(Sizing::fixed(MIXED_LABEL_WIDTH))
            .height(Sizing::FIT),
        |builder| {
            builder.text(label, body_style(color));
        },
    );
    builder.with(
        El::new().width(Sizing::GROW).height(Sizing::FIT),
        |builder| {
            builder.text(value, body_style(TEXT_MAIN));
        },
    );
    builder.text(count, body_style(TEXT_MUTED));
}

fn mixed_row(
    builder: &mut LayoutBuilder,
    label: &str,
    value: &str,
    count: &str,
    color: Color,
    border: Border,
) {
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::fixed(15.0))
            .gap(ROW_GAP)
            .padding(Padding::new(4.0, 4.0, 4.0, 4.0))
            .background(MIXED_ROW_BG)
            .border(border)
            .corner_radius(CornerRadius::all(Mm(1.0)))
            .alignment(AlignX::Left, AlignY::Center),
        |builder| mixed_row_body(builder, label, value, count, color),
    );
}

// Taller than a text row: the label/value/count sit on top and the analytic
// shape group draws in its own strip below, so the line and caps no longer
// overlap the text.
fn mixed_shape_row(
    builder: &mut LayoutBuilder,
    label: &str,
    value: &str,
    count: &str,
    color: Color,
) {
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::fixed(26.0))
            .gap(2.0)
            .padding(Padding::new(4.0, 4.0, 4.0, 3.0))
            .background(MIXED_ROW_BG)
            .corner_radius(CornerRadius::all(Mm(1.0)))
            .alignment(AlignX::Left, AlignY::Top),
        |builder| {
            builder.with(
                El::row()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .gap(ROW_GAP),
                |builder| mixed_row_body(builder, label, value, count, color),
            );
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .draw(mixed_shape_group(color)),
                |_builder| {},
            );
        },
    );
}

fn material_group_title_style() -> TextStyle {
    TextStyle::new(MATERIAL_GROUP_TITLE_FONT_SIZE)
        .bold()
        .with_color(TEXT_MAIN)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn material_caption_style(color: Color) -> TextStyle {
    TextStyle::new(MATERIAL_CASE_CAPTION_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn material_value_style(color: Color) -> TextStyle {
    TextStyle::new(MATERIAL_CASE_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn title_style() -> TextStyle {
    TextStyle::new(TITLE_FONT_SIZE)
        .bold()
        .with_color(TEXT_MAIN)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn stats_style(color: Color) -> TextStyle {
    TextStyle::new(STATS_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn subtitle_style(color: Color) -> TextStyle {
    TextStyle::new(SUBTITLE_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn swatch_style(color: Color) -> TextStyle {
    TextStyle::new(SWATCH_FONT_SIZE)
        .bold()
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn body_style(color: Color) -> TextStyle {
    TextStyle::new(BODY_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn small_style(color: Color) -> TextStyle {
    TextStyle::new(SMALL_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}
