//! @generated `bevy_example_template`
//! OpenType font feature showcase.
//!
//! Demonstrates `FontFeatures` by showing text with each OpenType feature
//! enabled (shaper default) vs explicitly disabled. Loads Noto Sans for
//! `liga` samples and uses the built-in `JetBrains` Mono for `calt`/`kern`.
//! Rendered as a [`DiegeticPanel`] hovering above a visible ground plane.

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Font;
use bevy_diegetic::FontFeatureFlags;
use bevy_diegetic::FontFeatures;
use bevy_diegetic::FontRegistered;
use bevy_diegetic::GlyphLoadingPolicy;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_window_manager::WindowManagerPlugin;

/// World-space panel height. Width follows the window aspect ratio.
const PANEL_WORLD_HEIGHT: f32 = 3.1;

/// Space between the panel's bottom edge and the ground plane.
const PANEL_GROUND_CLEARANCE: f32 = 0.45;

/// Base offset used to place the ground plane in front of the origin.
const PANEL_FORWARD_OFFSET: f32 = 1.35;

/// Panel sits this far back from the plane's front edge.
const PANEL_FRONT_DEPTH_FRACTION: f32 = 0.10;

/// Extra plane coverage around the panel.
const GROUND_SIDE_MARGIN: f32 = 1.4;
const GROUND_FRONT_MARGIN: f32 = 0.65;
const GROUND_BACK_MARGIN: f32 = 2.0;

/// Layout height in points (width follows window aspect ratio).
const LAYOUT_HEIGHT: f32 = 792.0;

/// Font size for feature samples (pt).
const SAMPLE_SIZE: f32 = 48.0;

/// Font size for Off/On labels (pt).
const ON_OFF_SIZE: f32 = 14.0;

/// Font size for section headers (pt).
const SECTION_SIZE: f32 = 18.0;

/// Font size for the typeface label shown at the top-right of each cell (pt).
const FONT_NAME_SIZE: f32 = 16.0;

/// Marker for the showcase panel.
#[derive(Component)]
struct ShowcasePanel;

#[derive(Component)]
struct GroundPlane;

/// Keeps font handles alive so Bevy doesn't unload the assets.
#[derive(Resource, Default)]
struct FontHandles(Vec<Handle<Font>>);

/// Tracks the EB Garamond font ID once registered.
#[derive(Resource, Default)]
struct SerifFontId(Option<u16>);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            LagrangePlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
        ))
        .init_resource::<FontHandles>()
        .init_resource::<SerifFontId>()
        .add_observer(on_font_registered)
        .add_systems(Startup, setup)
        .add_systems(Update, resize_panel)
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut font_handles: ResMut<FontHandles>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    windows: Query<&Window>,
    noto_id: Res<SerifFontId>,
) {
    font_handles
        .0
        .push(asset_server.load("fonts/EBGaramond-Regular.ttf"));

    let Ok(window) = windows.single() else {
        return;
    };
    let (layout_w, layout_h) = layout_dimensions(window);
    let world_h = PANEL_WORLD_HEIGHT;
    let world_w = world_h * (layout_w / layout_h);
    let (ground_w, ground_d) = ground_dimensions(world_w);
    let ground_z = ground_center_z();

    commands.spawn((
        GroundPlane,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(ground_w, ground_d))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.08, 0.08),
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, ground_z),
    ));

    commands.spawn((
        ShowcasePanel,
        DiegeticPanel::builder()
            .size((layout_w, layout_h))
            .layout_unit(Unit::Points)
            .world_height(PANEL_WORLD_HEIGHT)
            .anchor(Anchor::TopLeft)
            .layout(|b| {
                build_panel_content(b, noto_id.0);
            })
            .build(),
        panel_transform(world_w, world_h),
    ));

    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(1.5, 7.5, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-1.5, 7.5, -6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((OrbitCam {
        focus: Vec3::new(0.067_647_74, 1.913_066_5, 2.400_296_7),
        radius: Some(4.385_594_4),
        yaw: Some(-0.004_848_164),
        pitch: Some(0.026_128_9),
        button_orbit: MouseButton::Middle,
        button_pan: MouseButton::Middle,
        modifier_pan: Some(KeyCode::ShiftLeft),
        trackpad_behavior: TrackpadBehavior::BlenderLike {
            modifier_pan:  Some(KeyCode::ShiftLeft),
            modifier_zoom: Some(KeyCode::ControlLeft),
        },
        trackpad_pinch_to_zoom_enabled: true,
        ..default()
    },));
}

/// Returns layout dimensions in points, matching the window aspect ratio.
fn layout_dimensions(window: &Window) -> (f32, f32) {
    let aspect = window.width() / window.height();
    (LAYOUT_HEIGHT * aspect, LAYOUT_HEIGHT)
}

fn ground_dimensions(world_w: f32) -> (f32, f32) {
    (
        GROUND_SIDE_MARGIN.mul_add(2.0, world_w),
        GROUND_FRONT_MARGIN + GROUND_BACK_MARGIN + 2.0,
    )
}

fn ground_center_z() -> f32 {
    (GROUND_FRONT_MARGIN - GROUND_BACK_MARGIN).mul_add(0.5, PANEL_FORWARD_OFFSET)
}

fn panel_z() -> f32 {
    let (_, ground_d) = ground_dimensions(0.0);
    ground_d.mul_add(0.5 - PANEL_FRONT_DEPTH_FRACTION, ground_center_z())
}

fn panel_transform(world_w: f32, world_h: f32) -> Transform {
    Transform::from_xyz(-world_w * 0.5, world_h + PANEL_GROUND_CLEARANCE, panel_z())
}

fn resize_panel(
    windows: Query<&Window, Changed<Window>>,
    mut panels: Query<
        (&mut DiegeticPanel, &mut Transform),
        (With<ShowcasePanel>, Without<GroundPlane>),
    >,
    mut ground: Query<(&mut Mesh3d, &mut Transform), (With<GroundPlane>, Without<ShowcasePanel>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    noto_id: Res<SerifFontId>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let (layout_w, layout_h) = layout_dimensions(window);
    let world_h = PANEL_WORLD_HEIGHT;
    let world_w = world_h * (layout_w / layout_h);
    let (ground_w, ground_d) = ground_dimensions(world_w);
    let ground_z = ground_center_z();

    for (mut panel, mut transform) in &mut panels {
        #[allow(clippy::float_cmp)]
        if panel.width == layout_w && panel.height == layout_h {
            continue;
        }
        let new = DiegeticPanel::builder()
            .size((layout_w, layout_h))
            .layout_unit(Unit::Points)
            .world_height(PANEL_WORLD_HEIGHT)
            .anchor(Anchor::TopLeft)
            .layout(|b| {
                build_panel_content(b, noto_id.0);
            })
            .build();
        *panel = new;
        *transform = panel_transform(world_w, world_h);
    }

    for (mut mesh3d, mut transform) in &mut ground {
        mesh3d.0 = meshes.add(Plane3d::default().mesh().size(ground_w, ground_d));
        transform.translation.z = ground_z;
    }
}

fn on_font_registered(
    trigger: On<FontRegistered>,
    mut noto_id: ResMut<SerifFontId>,
    mut panels: Query<&mut DiegeticPanel, With<ShowcasePanel>>,
    windows: Query<&Window>,
) {
    info!(
        "FontRegistered: {} (id: {}, {})",
        trigger.name, trigger.id.0, trigger.source
    );
    noto_id.0 = Some(trigger.id.0);

    let Ok(window) = windows.single() else {
        return;
    };
    let (layout_w, layout_h) = layout_dimensions(window);
    for mut panel in &mut panels {
        let new = DiegeticPanel::builder()
            .size((layout_w, layout_h))
            .layout_unit(Unit::Points)
            .world_height(PANEL_WORLD_HEIGHT)
            .anchor(Anchor::TopLeft)
            .layout(|b| {
                build_panel_content(b, noto_id.0);
            })
            .build();
        *panel = new;
    }
}

// ── Panel layout ────────────────────────────────────────────────────────────

/// Populates the panel layout. Called from the builder's `.layout()` closure.
/// All spatial values are in points.
fn build_panel_content(b: &mut LayoutBuilder, serif_font_id: Option<u16>) {
    let bg = Color::srgb_u8(40, 40, 45);
    let border_color = Color::srgb_u8(70, 75, 85);
    let column_border_color = Color::srgba(0.75, 0.8, 0.9, 0.3);
    let section_color = Color::srgb(0.55, 0.75, 1.0);
    let label_color = Color::srgba(1.0, 1.0, 1.0, 0.45);
    let on_color = Color::WHITE;
    let off_color = Color::srgb(0.7, 0.7, 0.7);

    let progressive = GlyphLoadingPolicy::Progressive;

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(12.0))
            .direction(Direction::TopToBottom)
            .child_gap(8.0)
            .background(bg)
            .border(Border::all(1.0, border_color)),
        |b| {
            b.text(
                "Font Features",
                LayoutTextStyle::new(SECTION_SIZE + 4.0)
                    .with_color(section_color)
                    .with_loading_policy(progressive),
            );

            let serif_font = serif_font_id.unwrap_or(0);
            let serif_name = if serif_font_id.is_some() {
                "EB Garamond"
            } else {
                "(loading...)"
            };

            build_feature_grid(
                b,
                serif_name,
                serif_font,
                section_color,
                label_color,
                on_color,
                off_color,
                column_border_color,
                progressive,
            );
        },
    );
}

/// Builds the 2x2 feature grid (LIGA|CALT over DLIG|KERN).
fn build_feature_grid(
    b: &mut LayoutBuilder,
    serif_name: &str,
    serif_font: u16,
    section_color: Color,
    label_color: Color,
    on_color: Color,
    off_color: Color,
    column_border_color: Color,
    loading_policy: GlyphLoadingPolicy,
) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom)
            .child_gap(12.0),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .child_gap(12.0),
                |b| {
                    build_feature_column(
                        b,
                        "LIGA — Standard Ligatures",
                        serif_name,
                        serif_font,
                        FontFeatureFlags::LIGA,
                        true,
                        &["fi", "fl", "ffi", "ffl"],
                        section_color,
                        label_color,
                        on_color,
                        off_color,
                        column_border_color,
                        loading_policy,
                    );
                    build_feature_column(
                        b,
                        "CALT — Contextual Alternates",
                        "JetBrains Mono",
                        0,
                        FontFeatureFlags::CALT,
                        true,
                        &["::", "->", "=>", "!="],
                        section_color,
                        label_color,
                        on_color,
                        off_color,
                        column_border_color,
                        loading_policy,
                    );
                },
            );
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .child_gap(12.0),
                |b| {
                    build_feature_column(
                        b,
                        "DLIG — Discretionary",
                        serif_name,
                        serif_font,
                        FontFeatureFlags::DLIG,
                        false,
                        &["Th", "st", "ct"],
                        section_color,
                        label_color,
                        on_color,
                        off_color,
                        column_border_color,
                        loading_policy,
                    );
                    build_feature_column(
                        b,
                        "KERN — Kerning",
                        serif_name,
                        serif_font,
                        FontFeatureFlags::KERN,
                        true,
                        &["AVAV", "Type", "Wolf"],
                        section_color,
                        label_color,
                        on_color,
                        off_color,
                        column_border_color,
                        loading_policy,
                    );
                },
            );
        },
    );
}

/// Builds a single feature column: header, font name, then pairs of
/// on/off samples for each test string.
fn build_feature_column(
    b: &mut LayoutBuilder,
    title: &str,
    font_name: &str,
    font_id: u16,
    feature: FontFeatureFlags,
    default_on: bool,
    samples: &[&str],
    section_color: Color,
    label_color: Color,
    on_color: Color,
    off_color: Color,
    column_border_color: Color,
    loading_policy: GlyphLoadingPolicy,
) {
    // "on" explicitly enables the feature, "off" explicitly disables it.
    // For features on by default (liga, calt, kern), "on" = default, "off" = disabled.
    // For features off by default (dlig), "on" = enabled, "off" = default.
    let on_features = if default_on {
        FontFeatures::new()
    } else {
        FontFeatures::new().with(feature)
    };
    let off_features = if default_on {
        FontFeatures::new().without(feature)
    } else {
        FontFeatures::new()
    };

    let on_config = LayoutTextStyle::new(SAMPLE_SIZE)
        .with_font(font_id)
        .with_color(on_color)
        .with_font_features(on_features)
        .with_loading_policy(loading_policy);

    let off_config = LayoutTextStyle::new(SAMPLE_SIZE)
        .with_font(font_id)
        .with_color(off_color)
        .with_font_features(off_features)
        .with_loading_policy(loading_policy);

    let label_config = LayoutTextStyle::new(ON_OFF_SIZE)
        .with_color(label_color)
        .with_loading_policy(loading_policy);
    let font_name_config = LayoutTextStyle::new(FONT_NAME_SIZE)
        .with_font(font_id)
        .with_color(label_color)
        .with_loading_policy(loading_policy);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(8.0))
            .direction(Direction::TopToBottom)
            .child_gap(4.0)
            .border(Border::all(0.75, column_border_color)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight)
                    .child_gap(8.0)
                    .child_align_y(AlignY::Top),
                |b| {
                    b.text(
                        title,
                        LayoutTextStyle::new(SECTION_SIZE)
                            .with_color(section_color)
                            .with_loading_policy(loading_policy),
                    );
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .child_align_x(AlignX::Right),
                        |b| {
                            b.text(font_name, font_name_config.clone());
                        },
                    );
                },
            );

            build_sample_rows(b, samples, &label_config, &on_config, &off_config);
        },
    );
}

/// Builds the Off/On header row and per-sample comparison rows.
fn build_sample_rows(
    b: &mut LayoutBuilder,
    samples: &[&str],
    label_config: &LayoutTextStyle,
    on_config: &LayoutTextStyle,
    off_config: &LayoutTextStyle,
) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom)
            .child_gap(0.0),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::percent(0.11))
                    .padding(Padding::new(6.0, 4.0, 0.0, 4.0))
                    .direction(Direction::LeftToRight)
                    .child_gap(10.0)
                    .child_align_y(AlignY::Bottom),
                |b| {
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .child_alignment(AlignX::Center, AlignY::Top),
                        |b| {
                            b.text("Off", label_config.clone());
                        },
                    );
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .child_alignment(AlignX::Center, AlignY::Top),
                        |b| {
                            b.text("On", label_config.clone());
                        },
                    );
                },
            );

            for &sample in samples {
                b.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .padding(Padding::new(0.0, 4.0, 0.0, 4.0))
                        .direction(Direction::LeftToRight)
                        .child_gap(10.0),
                    |b| {
                        b.with(
                            El::new()
                                .width(Sizing::GROW)
                                .height(Sizing::GROW)
                                .child_alignment(AlignX::Center, AlignY::Center),
                            |b| {
                                b.text(sample, off_config.clone());
                            },
                        );
                        b.with(
                            El::new()
                                .width(Sizing::GROW)
                                .height(Sizing::GROW)
                                .child_alignment(AlignX::Center, AlignY::Center),
                            |b| {
                                b.text(sample, on_config.clone());
                            },
                        );
                    },
                );
            }
        },
    );
}
