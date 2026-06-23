//! Keyboard shortcut help overlay for title bars.

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use bevy_enhanced_input::prelude::*;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;

use super::ControlActivation;
use super::TitleBarControlState;
use super::constants::BODY_COLOR;
use super::constants::CAMERA_PRESET_KEYS;
use super::constants::CAMERA_PRESET_LABEL;
use super::constants::CLOSE_HINT;
use super::constants::DIVIDER_COLOR;
use super::constants::HELP_CLOSE_CONTEXT_PRIORITY;
use super::constants::HELP_CLOSE_HINT_COLUMN_WIDTH;
use super::constants::HELP_CLOSE_HINT_SIZE;
use super::constants::HELP_CONTROL;
use super::constants::HELP_KEY_COLUMN_WIDTH;
use super::constants::HELP_PANEL_CHILD_GAP;
use super::constants::HELP_ROW_GAP;
use super::constants::HELP_SEPARATOR_HEIGHT;
use super::constants::HELP_TABLE_COLUMN_GAP;
use super::constants::HELP_TITLE;
use super::constants::HOME_AABB_KEYS;
use super::constants::HOME_AABB_LABEL;
use super::constants::SCREEN_PANEL_KEYS;
use super::constants::SCREEN_PANEL_LABEL;
use super::default_inner_background;
use super::screen_panel_frame;
use crate::camera_control_panel::CameraGuidancePanel;
use crate::camera_control_panel::CameraPresetSwitching;
use crate::camera_home::CameraHomeMarker;
use crate::constants::LABEL_SIZE;
use crate::constants::TITLE_COLOR;
use crate::constants::TITLE_SIZE;
use crate::ensure_plugin;

/// Always-active context holding the Shift+/ toggle.
#[derive(Component)]
struct HelpContext;

/// Higher-priority context inserted on the `KeyboardShortcutHelp` overlay
/// entity. It owns the `CloseHelp`/Esc action while that entity exists and
/// consumes Esc so closing the overlay does not also fire a caller's Esc binding.
#[derive(Component)]
struct HelpCloseContext;

action!(ShowHelp);
event!(ShowHelpEvent);
action!(CloseHelp);
event!(CloseHelpEvent);

pub(super) fn install(app: &mut App) {
    ensure_plugin(app, EnhancedInputPlugin);
    app.add_input_context::<HelpContext>();
    app.add_input_context::<HelpCloseContext>();
    app.add_systems(Startup, spawn_help_context);
    bind_action_system!(app, ShowHelp, ShowHelpEvent, show_or_toggle_help);
    bind_action_system!(app, CloseHelp, CloseHelpEvent, close_help);
}

fn spawn_help_context(mut commands: Commands) {
    commands.spawn((
        HelpContext,
        Actions::<HelpContext>::spawn(SpawnWith(|spawner: &mut ActionSpawner<HelpContext>| {
            spawner.spawn((
                Action::<ShowHelp>::new(),
                bindings![KeyCode::Slash.with_mod_keys(ModKeys::SHIFT)],
            ));
        })),
    ));
}

#[derive(Component)]
pub(super) struct KeyboardShortcutHelp;

#[derive(Clone, Copy)]
enum ShortcutPresence {
    Present,
    Absent,
}

impl ShortcutPresence {
    const fn is_present(self) -> bool { matches!(self, Self::Present) }
}

#[derive(Clone, Copy)]
struct HelpShortcuts {
    home_marker:   ShortcutPresence,
    camera_preset: ShortcutPresence,
}

struct HelpRow {
    keys:  &'static str,
    label: &'static str,
}

/// Toggles the overlay on Shift+/: despawns it when open, otherwise spawns it
/// (reading which optional shortcuts apply).
fn show_or_toggle_help(
    mut commands: Commands,
    overlay: Query<Entity, With<KeyboardShortcutHelp>>,
    home_markers: Query<Entity, With<CameraHomeMarker>>,
    camera_panels: Query<Entity, With<CameraGuidancePanel>>,
    preset_switching: Option<Res<CameraPresetSwitching>>,
    mut bars: Query<&mut TitleBarControlState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !overlay.is_empty() {
        for entity in &overlay {
            commands.entity(entity).despawn();
        }
        set_help_chip(&mut bars, ControlActivation::Inactive);
        return;
    }

    let preset_switching_enabled =
        preset_switching.is_none_or(|switching| *switching == CameraPresetSwitching::Enabled);
    let home_marker = if home_markers.is_empty() {
        ShortcutPresence::Absent
    } else {
        ShortcutPresence::Present
    };
    let camera_preset = if !camera_panels.is_empty() && preset_switching_enabled {
        ShortcutPresence::Present
    } else {
        ShortcutPresence::Absent
    };
    spawn_help_overlay(
        &mut commands,
        HelpShortcuts {
            home_marker,
            camera_preset,
        },
        &mut materials,
    );
    set_help_chip(&mut bars, ControlActivation::Active);
}

/// Closes the overlay on Esc. Bound inside [`HelpCloseContext`], which consumes
/// Esc so a caller's Esc binding doesn't also fire while the overlay is open.
fn close_help(
    mut commands: Commands,
    overlay: Query<Entity, With<KeyboardShortcutHelp>>,
    mut bars: Query<&mut TitleBarControlState>,
) {
    for entity in &overlay {
        commands.entity(entity).despawn();
    }
    set_help_chip(&mut bars, ControlActivation::Inactive);
}

/// Highlights or clears the always-present `?` help chip on every title bar.
fn set_help_chip(bars: &mut Query<&mut TitleBarControlState>, activation: ControlActivation) {
    for mut bar in bars.iter_mut() {
        bar.set_active(HELP_CONTROL, activation);
    }
}

fn spawn_help_overlay(
    commands: &mut Commands,
    shortcuts: HelpShortcuts,
    materials: &mut Assets<StandardMaterial>,
) {
    let unlit = super::screen_panel_material_handle(materials);
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::Center)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_help_tree(shortcuts))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((
                KeyboardShortcutHelp,
                panel,
                Transform::default(),
                HelpCloseContext,
                ContextPriority::<HelpCloseContext>::new(HELP_CLOSE_CONTEXT_PRIORITY),
                Actions::<HelpCloseContext>::spawn(SpawnWith(
                    |spawner: &mut ActionSpawner<HelpCloseContext>| {
                        spawner.spawn((
                            Action::<CloseHelp>::new(),
                            ActionSettings {
                                consume_input: true,
                                ..default()
                            },
                            bindings![KeyCode::Escape],
                        ));
                    },
                )),
            ));
        },
        Err(error) => {
            error!("fairy_dust: failed to build keyboard shortcut help: {error}");
        },
    }
}

fn build_help_tree(shortcuts: HelpShortcuts) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_help_layout(&mut builder, shortcuts);
    builder.build()
}

fn build_help_layout(builder: &mut LayoutBuilder, shortcuts: HelpShortcuts) {
    let title = TextStyle::new(TITLE_SIZE).with_color(TITLE_COLOR).no_wrap();
    let hint = TextStyle::new(HELP_CLOSE_HINT_SIZE)
        .with_color(BODY_COLOR)
        .no_wrap();
    let label = TextStyle::new(LABEL_SIZE).with_color(BODY_COLOR).no_wrap();

    screen_panel_frame(
        builder,
        Sizing::FIT,
        Sizing::FIT,
        default_inner_background(),
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .gap(HELP_PANEL_CHILD_GAP),
                |builder| {
                    build_title_row(builder, &title, &hint);
                    build_separator(builder);
                    build_shortcut_table(builder, shortcuts, &label);
                },
            );
        },
    );
}

fn build_title_row(builder: &mut LayoutBuilder, title: &TextStyle, hint: &TextStyle) {
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(HELP_TABLE_COLUMN_GAP)
            .align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new().width(Sizing::GROW).height(Sizing::FIT),
                |builder| {
                    builder.text(HELP_TITLE, title.clone());
                },
            );
            builder.with(
                El::new()
                    .width(Sizing::fixed(HELP_CLOSE_HINT_COLUMN_WIDTH))
                    .height(Sizing::FIT)
                    .align_x(AlignX::Right),
                |builder| {
                    builder.text(CLOSE_HINT, hint.clone());
                },
            );
        },
    );
}

fn build_separator(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(HELP_SEPARATOR_HEIGHT))
            .background(DIVIDER_COLOR),
        |_| {},
    );
}

fn build_shortcut_table(builder: &mut LayoutBuilder, shortcuts: HelpShortcuts, label: &TextStyle) {
    builder.with(
        El::column()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .gap(HELP_ROW_GAP),
        |builder| {
            for row in shortcut_rows(shortcuts) {
                build_shortcut_row(builder, row, label);
            }
        },
    );
}

fn build_shortcut_row(builder: &mut LayoutBuilder, row: HelpRow, label: &TextStyle) {
    builder.with(
        El::row()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .gap(HELP_TABLE_COLUMN_GAP)
            .align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(HELP_KEY_COLUMN_WIDTH))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(row.keys, label.clone());
                },
            );
            builder.with(
                El::new().width(Sizing::FIT).height(Sizing::FIT),
                |builder| {
                    builder.text(row.label, label.clone());
                },
            );
        },
    );
}

fn shortcut_rows(shortcuts: HelpShortcuts) -> Vec<HelpRow> {
    let mut rows = Vec::new();
    if shortcuts.home_marker.is_present() {
        rows.push(HelpRow {
            keys:  HOME_AABB_KEYS,
            label: HOME_AABB_LABEL,
        });
    }
    rows.push(HelpRow {
        keys:  SCREEN_PANEL_KEYS,
        label: SCREEN_PANEL_LABEL,
    });
    if shortcuts.camera_preset.is_present() {
        rows.push(HelpRow {
            keys:  CAMERA_PRESET_KEYS,
            label: CAMERA_PRESET_LABEL,
        });
    }
    rows
}
