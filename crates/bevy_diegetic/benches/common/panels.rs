use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use bevy_diegetic::Unit;
use clay_layout::ClayLayoutScope;
use clay_layout::Declaration;
use clay_layout::fit;
use clay_layout::fixed;
use clay_layout::grow;
use clay_layout::layout::Alignment;
use clay_layout::layout::LayoutAlignmentX;
use clay_layout::layout::LayoutAlignmentY;
use clay_layout::layout::LayoutDirection;

use super::measurement::CLAY_FONT_SIZE;
use super::measurement::FONT_SIZE;
use super::rows::StatusRow;

pub const PANEL_SIZE: f32 = 160.0;
pub const RESIZED_PANEL_SIZE: f32 = 192.0;

#[derive(Clone, Copy)]
pub struct DiegeticStatusTreeStyle {
    pub text_color:         Color,
    pub root_background:    Color,
    pub divider_background: Color,
    pub header_background:  Color,
    pub body_background:    Color,
    pub root_child_gap:     f32,
}

impl Default for DiegeticStatusTreeStyle {
    fn default() -> Self {
        Self {
            text_color:         Color::WHITE,
            root_background:    Color::srgb_u8(180, 96, 122),
            divider_background: Color::srgb_u8(74, 196, 172),
            header_background:  Color::srgb_u8(52, 98, 90),
            body_background:    Color::srgb_u8(22, 28, 34),
            root_child_gap:     5.0,
        }
    }
}

#[must_use = "bench panels use this unit for layout and font conversion"]
pub fn layout_unit(size: f32) -> Unit { Unit::Custom(1.0 / size) }

#[must_use = "raw benchmarks need the same unit conversion as the public path"]
pub fn layout_to_points(size: f32) -> f32 { layout_unit(size).to_points() }

pub fn build_clay_status_panel<'a>(
    layout: &mut ClayLayoutScope<'a, 'a, (), ()>,
    rows: &[StatusRow],
) {
    layout.with(
        Declaration::new()
            .layout()
            .width(grow!())
            .height(grow!())
            .padding(clay_layout::layout::Padding::all(8))
            .direction(LayoutDirection::TopToBottom)
            .child_gap(5)
            .end()
            .background_color((180, 96, 122).into()),
        |clay| {
            build_clay_header(clay);
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(fixed!(4.0))
                    .end()
                    .background_color((74, 196, 172).into()),
                |_| {},
            );
            build_clay_body(clay, rows);
        },
    );
}

#[must_use = "benchmarks need the tree as fixture input"]
pub fn build_diegetic_status_tree(rows: &[StatusRow]) -> LayoutTree {
    build_diegetic_status_tree_with_style(rows, DiegeticStatusTreeStyle::default())
}

#[must_use = "benchmarks need the tree as fixture input"]
pub fn build_diegetic_status_tree_with_text_color(
    rows: &[StatusRow],
    text_color: Color,
) -> LayoutTree {
    build_diegetic_status_tree_with_style(
        rows,
        DiegeticStatusTreeStyle {
            text_color,
            ..Default::default()
        },
    )
}

#[must_use = "benchmarks need the tree as fixture input"]
pub fn build_diegetic_status_tree_with_style(
    rows: &[StatusRow],
    style: DiegeticStatusTreeStyle,
) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(8.0))
            .direction(Direction::TopToBottom)
            .child_gap(style.root_child_gap)
            .background(style.root_background),
    );
    build_diegetic_header(&mut builder, style);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(4.0))
            .background(style.divider_background),
        |_| {},
    );
    build_diegetic_body(&mut builder, rows, style);
    builder.build()
}

#[must_use = "benchmarks need the panel component as fixture input"]
#[allow(
    clippy::expect_used,
    reason = "bench fixture construction should fail loudly if static panel dimensions are invalid"
)]
pub fn bench_panel(tree: LayoutTree, size: f32) -> DiegeticPanel {
    let unit = layout_unit(size);
    let dim = bevy_diegetic::Dimension {
        value: size,
        unit:  Some(unit),
    };
    DiegeticPanel::world()
        .size(
            bevy_diegetic::Sizing::Fixed(dim),
            bevy_diegetic::Sizing::Fixed(dim),
        )
        .font_unit(unit)
        .with_tree(tree)
        .build()
        .expect("bench panel dimensions must be valid")
}

fn build_clay_header<'a>(clay: &mut ClayLayoutScope<'a, 'a, (), ()>) {
    clay.with(
        Declaration::new()
            .layout()
            .width(grow!())
            .height(grow!(FONT_SIZE, 20.0))
            .padding(clay_layout::layout::Padding::new(5, 5, 4, 4))
            .child_alignment(Alignment::new(
                LayoutAlignmentX::Left,
                LayoutAlignmentY::Center,
            ))
            .end()
            .background_color((52, 98, 90).into()),
        |clay| {
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(fit!())
                    .direction(LayoutDirection::LeftToRight)
                    .end(),
                |clay| {
                    clay.with(
                        Declaration::new()
                            .layout()
                            .width(fit!())
                            .height(grow!())
                            .end(),
                        |clay| {
                            clay.text(
                                "STATUS",
                                clay_layout::text::TextConfig::new()
                                    .font_size(CLAY_FONT_SIZE)
                                    .end(),
                            );
                        },
                    );
                    clay.with(
                        Declaration::new()
                            .layout()
                            .width(grow!())
                            .height(fixed!(1.0))
                            .end(),
                        |_| {},
                    );
                    clay.with(
                        Declaration::new()
                            .layout()
                            .width(fit!())
                            .height(grow!())
                            .end(),
                        |clay| {
                            clay.text(
                                "BENCH",
                                clay_layout::text::TextConfig::new()
                                    .font_size(CLAY_FONT_SIZE)
                                    .end(),
                            );
                        },
                    );
                },
            );
        },
    );
}

fn build_clay_body<'a>(clay: &mut ClayLayoutScope<'a, 'a, (), ()>, rows: &[StatusRow]) {
    clay.with(
        Declaration::new()
            .layout()
            .width(grow!())
            .height(grow!())
            .end()
            .background_color((22, 28, 34).into()),
        |clay| {
            clay.with(
                Declaration::new()
                    .layout()
                    .width(grow!())
                    .padding(clay_layout::layout::Padding::all(5))
                    .direction(LayoutDirection::TopToBottom)
                    .child_gap(2)
                    .end(),
                |clay| {
                    for (label, value) in rows {
                        clay.with(
                            Declaration::new()
                                .layout()
                                .width(grow!())
                                .height(fit!())
                                .direction(LayoutDirection::LeftToRight)
                                .end(),
                            |clay| {
                                clay.text(
                                    label,
                                    clay_layout::text::TextConfig::new()
                                        .font_size(CLAY_FONT_SIZE)
                                        .end(),
                                );
                                clay.with(Declaration::new().layout().width(grow!()).end(), |_| {});
                                clay.text(
                                    value,
                                    clay_layout::text::TextConfig::new()
                                        .font_size(CLAY_FONT_SIZE)
                                        .end(),
                                );
                            },
                        );
                    }
                },
            );
        },
    );
}

fn build_diegetic_header(builder: &mut LayoutBuilder, style: DiegeticStatusTreeStyle) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::grow_range(FONT_SIZE, 20.0))
            .padding(Padding::new(5.0, 5.0, 4.0, 4.0))
            .child_align_y(AlignY::Center)
            .background(style.header_background),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight),
                |builder| {
                    builder.with(
                        El::new().width(Sizing::FIT).height(Sizing::GROW),
                        |builder| {
                            builder.text(
                                "STATUS",
                                TextStyle::new(FONT_SIZE).with_color(style.text_color),
                            );
                        },
                    );
                    builder.with(
                        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                        |_| {},
                    );
                    builder.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::GROW)
                            .child_align_x(AlignX::Right),
                        |builder| {
                            builder.text(
                                "BENCH",
                                TextStyle::new(FONT_SIZE).with_color(style.text_color),
                            );
                        },
                    );
                },
            );
        },
    );
}

fn build_diegetic_body(
    builder: &mut LayoutBuilder,
    rows: &[StatusRow],
    style: DiegeticStatusTreeStyle,
) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(style.body_background),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .padding(Padding::all(5.0))
                    .direction(Direction::TopToBottom)
                    .child_gap(2.0),
                |builder| {
                    for (label, value) in rows {
                        builder.with(
                            El::new()
                                .width(Sizing::GROW)
                                .height(Sizing::FIT)
                                .direction(Direction::LeftToRight),
                            |builder| {
                                builder.text(
                                    *label,
                                    TextStyle::new(FONT_SIZE).with_color(style.text_color),
                                );
                                builder.with(
                                    El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                    |_| {},
                                );
                                builder.text(
                                    *value,
                                    TextStyle::new(FONT_SIZE).with_color(style.text_color),
                                );
                            },
                        );
                    }
                },
            );
        },
    );
}
