use bevy::camera::ClearColorConfig;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::diagnostic::FrameCount;
use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;
use bevy::winit::WinitMonitors;
use bevy_clerestory::CurrentMonitor;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::MonitorInfo;
use bevy_clerestory::Monitors;
use bevy_clerestory::WindowKey;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TITLE_COLOR;
use fairy_dust::TITLE_SIZE;
use hana_diegetic::Anchor;
use hana_diegetic::DiegeticPanel;
use hana_diegetic::DiegeticPanelCommands;
use hana_diegetic::El;
use hana_diegetic::Fit;
use hana_diegetic::LayoutBuilder;
use hana_diegetic::Sizing;
use hana_diegetic::StableTransparency;
use hana_diegetic::Text;
use hana_diegetic::TextStyle;

use super::constants::*;
use super::setup::UnregisteredControl;
use super::trace::ProbeTrace;
use super::window_trace::NativeMonitorState;
use super::window_trace::native_monitor_state;

#[derive(Component)]
pub(super) struct ProbeContentAttached;

#[derive(Clone, Copy, Component)]
pub(super) struct ProbeContentOwner {
    window: Entity,
}

#[derive(Component)]
pub(super) struct ProbeWindowPanel;

#[derive(Component)]
pub(super) struct ProbeWindowClearCamera;

#[derive(Component)]
struct ProbeTransparencyCamera;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeWindowRole {
    PrimaryAutomatic,
    ManagedAutomatic,
    ApplicationControlled,
    UnregisteredControl,
}

impl ProbeWindowRole {
    fn from_window(
        primary: bool,
        managed: Option<&ManagedWindow>,
        unregistered: bool,
    ) -> Option<Self> {
        match (primary, managed, unregistered) {
            (true, None, false) => Some(Self::PrimaryAutomatic),
            (false, Some(managed), false) if managed.name == AUTOMATIC_WINDOW_KEY => {
                Some(Self::ManagedAutomatic)
            },
            (false, Some(managed), false) if managed.name == APPLICATION_WINDOW_KEY => {
                Some(Self::ApplicationControlled)
            },
            (false, None, true) => Some(Self::UnregisteredControl),
            _ => None,
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::PrimaryAutomatic => "Primary Automatic",
            Self::ManagedAutomatic => "Managed Automatic",
            Self::ApplicationControlled => "Application Controlled",
            Self::UnregisteredControl => "Unregistered Control",
        }
    }

    const fn key_label(self) -> &'static str {
        match self {
            Self::PrimaryAutomatic => "Primary",
            Self::ManagedAutomatic => "Managed(hotplug-automatic)",
            Self::ApplicationControlled => "Managed(hotplug-application)",
            Self::UnregisteredControl => "none",
        }
    }

    const fn recovery_label(self) -> &'static str {
        match self {
            Self::PrimaryAutomatic | Self::ManagedAutomatic => "automatic fallback and return",
            Self::ApplicationControlled => "application-controlled return",
            Self::UnregisteredControl => "unregistered; no return",
        }
    }

    fn trace_window_key(self) -> String {
        match self {
            Self::PrimaryAutomatic => format!("{:?}", WindowKey::Primary),
            Self::ManagedAutomatic => {
                format!("{:?}", WindowKey::Managed(AUTOMATIC_WINDOW_KEY.into()))
            },
            Self::ApplicationControlled => {
                format!("{:?}", WindowKey::Managed(APPLICATION_WINDOW_KEY.into()))
            },
            Self::UnregisteredControl => VALUE_UNREGISTERED.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum ProbeTargetMonitor {
    #[default]
    Pending,
    Selected(MonitorInfo),
    MissingAtStartup {
        selected_index: usize,
    },
}

#[derive(Default, Resource)]
pub(super) struct ProbeTarget(ProbeTargetMonitor);

impl ProbeTarget {
    pub(super) fn capture(&mut self, selected_index: usize, monitor: Option<&MonitorInfo>) {
        self.0 = monitor.map_or(
            ProbeTargetMonitor::MissingAtStartup { selected_index },
            |monitor| ProbeTargetMonitor::Selected(*monitor),
        );
    }
}

#[derive(Clone, Debug, PartialEq, Component)]
pub(super) struct ProbeWindowMetadata {
    role:   ProbeWindowRole,
    fields: Vec<(&'static str, String)>,
}

#[derive(Default, Resource)]
pub(super) enum ProbePanelMaterial {
    #[default]
    Uninitialized,
    Ready(Handle<StandardMaterial>),
}

impl ProbePanelMaterial {
    fn handle(&mut self, materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
        match self {
            Self::Uninitialized => {
                let handle = fairy_dust::screen_panel_material_handle(materials);
                *self = Self::Ready(handle.clone());
                handle
            },
            Self::Ready(handle) => handle.clone(),
        }
    }
}

fn field(name: &str, value: impl std::fmt::Debug) -> (String, String) {
    (name.into(), format!("{value:?}"))
}

fn monitor_label(entity: Entity, monitor: MonitorInfo) -> String {
    format!(
        "entity {entity:?}; index {}; scale {:.2}; {:?}",
        monitor.index, monitor.scale, monitor.identity
    )
}

fn requested_target_label(target: ProbeTargetMonitor) -> String {
    match target {
        ProbeTargetMonitor::Pending => "waiting for startup inventory".into(),
        ProbeTargetMonitor::Selected(monitor) => format!(
            "index {}; scale {:.2}; {:?}",
            monitor.index, monitor.scale, monitor.identity
        ),
        ProbeTargetMonitor::MissingAtStartup { selected_index } => {
            format!("index {selected_index}; absent at startup")
        },
    }
}

fn bevy_monitor_label(
    on_monitor: Option<&OnMonitor>,
    current_monitor: Option<&CurrentMonitor>,
) -> String {
    match (on_monitor, current_monitor) {
        (Some(on_monitor), Some(current_monitor)) => {
            monitor_label(on_monitor.0, current_monitor.monitor_info)
        },
        (Some(on_monitor), None) => format!("linked to {:?}; CurrentMonitor absent", on_monitor.0),
        (None, Some(current_monitor)) => format!(
            "OnMonitor absent; index {}; scale {:.2}; {:?}",
            current_monitor.monitor_info.index,
            current_monitor.monitor_info.scale,
            current_monitor.monitor_info.identity,
        ),
        (None, None) => "association not available".into(),
    }
}

fn native_monitor_label(state: NativeMonitorState) -> String {
    match state {
        NativeMonitorState::WindowUnavailable => "native window not available".into(),
        NativeMonitorState::NoMonitorHandle => "no native monitor handle".into(),
        NativeMonitorState::HandleUnmatched => "native handle did not match Bevy inventory".into(),
        NativeMonitorState::Matched { entity, monitor } => monitor_label(entity, monitor),
    }
}

fn window_metadata(
    role: ProbeWindowRole,
    window_entity: Entity,
    window: &Window,
    on_monitor: Option<&OnMonitor>,
    current_monitor: Option<&CurrentMonitor>,
    target: ProbeTargetMonitor,
    native_monitor: NativeMonitorState,
) -> ProbeWindowMetadata {
    ProbeWindowMetadata {
        role,
        fields: vec![
            ("window entity", format!("{window_entity:?}")),
            ("window key", role.key_label().into()),
            ("recovery", role.recovery_label().into()),
            ("original target", requested_target_label(target)),
            (
                "bevy current",
                bevy_monitor_label(on_monitor, current_monitor),
            ),
            ("native current", native_monitor_label(native_monitor)),
            ("mode", format!("{:?}", window.mode)),
            (
                "window state",
                format!(
                    "position {:?}; size {:?}",
                    window.position,
                    window.resolution.physical_size()
                ),
            ),
        ],
    }
}

fn build_panel_layout(builder: &mut LayoutBuilder, metadata: &ProbeWindowMetadata) {
    let title = TextStyle::new(TITLE_SIZE).with_color(TITLE_COLOR);
    let label = TextStyle::new(LABEL_SIZE).with_color(PANEL_LABEL_COLOR);
    let value = TextStyle::new(LABEL_SIZE).with_color(TITLE_COLOR);
    fairy_dust::screen_panel_frame(
        builder,
        Sizing::FIT,
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .gap(PANEL_CONTENT_GAP),
                |builder| {
                    builder.text((metadata.role.title(), title));
                    for (label_text, value_text) in &metadata.fields {
                        builder.with(
                            El::row()
                                .width(Sizing::FIT)
                                .height(Sizing::FIT)
                                .gap(PANEL_COLUMN_GAP),
                            |builder| {
                                builder.text(
                                    Text::new(*label_text, label.clone())
                                        .measure_as(PANEL_LABEL_MEASURE),
                                );
                                builder.text((value_text, value.clone()));
                            },
                        );
                    }
                },
            );
        },
    );
}

fn build_panel(
    window: Entity,
    metadata: &ProbeWindowMetadata,
    material: Handle<StandardMaterial>,
) -> Result<DiegeticPanel, hana_diegetic::PanelBuildError> {
    DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::Center)
        .window_entity(window)
        .material(material.clone())
        .text_material(material)
        .layout(|builder| build_panel_layout(builder, metadata))
        .build()
}

fn spawn_window_clear_camera(commands: &mut Commands, window: Entity) {
    commands.spawn((
        ProbeWindowClearCamera,
        ProbeContentOwner { window },
        Camera3d::default(),
        Camera {
            order: PANEL_CLEAR_CAMERA_ORDER,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        RenderTarget::Window(WindowRef::Entity(window)),
        RenderLayers::none(),
        Msaa::Off,
        Tonemapping::None,
    ));
}

pub(super) fn spawn_transparency_camera(mut commands: Commands) {
    commands.spawn((
        ProbeTransparencyCamera,
        Camera3d::default(),
        Camera {
            is_active: false,
            ..default()
        },
        StableTransparency,
        Tonemapping::None,
    ));
}

pub(super) fn remove_window_clear_camera(
    remove: On<Remove, Window>,
    cameras: Query<(Entity, &ProbeContentOwner), With<ProbeWindowClearCamera>>,
    mut commands: Commands,
) {
    for (camera, owner) in &cameras {
        if owner.window == remove.entity {
            commands.entity(camera).despawn();
        }
    }
}

pub(super) fn attach_window_content(
    windows: Query<
        (
            Entity,
            &Window,
            Has<PrimaryWindow>,
            Option<&ManagedWindow>,
            Has<UnregisteredControl>,
            Option<&OnMonitor>,
            Option<&CurrentMonitor>,
        ),
        Without<ProbeContentAttached>,
    >,
    installed_monitors: Option<Res<Monitors>>,
    winit_monitors: Option<Res<WinitMonitors>>,
    target: Res<ProbeTarget>,
    mut panel_material: ResMut<ProbePanelMaterial>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
    _: NonSendMarker,
) {
    for (window_entity, window, primary, managed, unregistered, on_monitor, current_monitor) in
        &windows
    {
        let Some(role) = ProbeWindowRole::from_window(primary, managed, unregistered) else {
            continue;
        };
        let native_monitor = winit_monitors.as_deref().map_or(
            NativeMonitorState::WindowUnavailable,
            |winit_monitors| {
                native_monitor_state(window_entity, installed_monitors.as_deref(), winit_monitors)
            },
        );
        let metadata = window_metadata(
            role,
            window_entity,
            window,
            on_monitor,
            current_monitor,
            target.0,
            native_monitor,
        );
        let material = panel_material.handle(&mut materials);
        let Ok(panel) = build_panel(window_entity, &metadata, material) else {
            error!("restore_after_reconnect: failed to build diagnostics panel");
            continue;
        };

        commands.entity(window_entity).insert(ProbeContentAttached);
        spawn_window_clear_camera(&mut commands, window_entity);
        commands.spawn((
            ProbeWindowPanel,
            ProbeContentOwner {
                window: window_entity,
            },
            metadata,
            panel,
            Transform::default(),
        ));
        trace.record(
            frame_count.0,
            PRODUCER_SETUP_CONTENT,
            KIND_CONTENT_ATTACHED,
            vec![
                field(FIELD_WINDOW, window_entity),
                (FIELD_WINDOW_KEY.into(), role.trace_window_key()),
            ],
        );
    }
}

pub(super) fn refresh_window_panels(
    windows: Query<
        (Entity, &Window, Option<&OnMonitor>, Option<&CurrentMonitor>),
        Or<(Changed<Window>, Changed<OnMonitor>, Changed<CurrentMonitor>)>,
    >,
    panels: Query<(Entity, &ProbeContentOwner, &ProbeWindowMetadata), With<ProbeWindowPanel>>,
    installed_monitors: Option<Res<Monitors>>,
    winit_monitors: Res<WinitMonitors>,
    target: Res<ProbeTarget>,
    mut commands: Commands,
    _: NonSendMarker,
) {
    for (window_entity, window, on_monitor, current_monitor) in &windows {
        for (panel_entity, owner, previous) in &panels {
            if owner.window != window_entity {
                continue;
            }
            let metadata = window_metadata(
                previous.role,
                window_entity,
                window,
                on_monitor,
                current_monitor,
                target.0,
                native_monitor_state(
                    window_entity,
                    installed_monitors.as_deref(),
                    &winit_monitors,
                ),
            );
            if metadata == *previous {
                continue;
            }
            let mut builder =
                LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
            build_panel_layout(&mut builder, &metadata);
            commands.set_tree(panel_entity, builder.build());
            commands.entity(panel_entity).insert(metadata);
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::window::MonitorSelection;
    use bevy::window::WindowMode;
    use bevy_clerestory::MonitorIdentity;
    use hana_diegetic::CoordinateSpace;

    use super::*;
    use crate::setup::probe_window;

    const MONITOR_PHYSICAL_SIZE: UVec2 = UVec2::new(1_920, 1_080);
    const MONITOR_SCALE: f64 = 1.0;
    const TARGET_INDEX: usize = 1;

    fn current_monitor() -> CurrentMonitor {
        CurrentMonitor {
            monitor_info:          MonitorInfo {
                identity:          MonitorIdentity::Unverified,
                index:             TARGET_INDEX,
                scale:             MONITOR_SCALE,
                physical_position: IVec2::ZERO,
                physical_size:     MONITOR_PHYSICAL_SIZE,
            },
            effective_window_mode: WindowMode::Windowed,
        }
    }

    fn content_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<StandardMaterial>>()
            .init_resource::<FrameCount>()
            .init_resource::<ProbePanelMaterial>()
            .init_resource::<ProbeTarget>()
            .insert_resource(ProbeTrace::default())
            .add_observer(remove_window_clear_camera)
            .add_systems(Update, attach_window_content);
        app
    }

    fn spawn_content_windows(world: &mut World, monitor_entity: Entity) -> [Entity; 4] {
        let primary = world
            .spawn((
                Window::default(),
                PrimaryWindow,
                OnMonitor(monitor_entity),
                current_monitor(),
            ))
            .id();
        let automatic = world
            .spawn((
                Window::default(),
                ManagedWindow {
                    name: AUTOMATIC_WINDOW_KEY.into(),
                },
                OnMonitor(monitor_entity),
                current_monitor(),
            ))
            .id();
        let application = world
            .spawn((
                Window::default(),
                ManagedWindow {
                    name: APPLICATION_WINDOW_KEY.into(),
                },
                OnMonitor(monitor_entity),
                current_monitor(),
            ))
            .id();
        let control = world
            .spawn((
                Window::default(),
                UnregisteredControl,
                OnMonitor(monitor_entity),
                current_monitor(),
            ))
            .id();
        [primary, automatic, application, control]
    }

    fn owned_panels(world: &mut World, window: Entity) -> Vec<Entity> {
        let mut query =
            world.query_filtered::<(Entity, &ProbeContentOwner), With<ProbeWindowPanel>>();
        query
            .iter(world)
            .filter_map(|(entity, owner)| (owner.window == window).then_some(entity))
            .collect()
    }

    fn assert_window_panel(world: &mut World, window: Entity) {
        let panels = owned_panels(world, window);
        assert_eq!(panels.len(), 1);
        let panel = world
            .get::<DiegeticPanel>(panels[0])
            .expect("owned panel should carry DiegeticPanel");
        assert_eq!(panel.anchor(), Anchor::Center);
        assert!(matches!(
            panel.coordinate_space(),
            CoordinateSpace::Screen {
                window: WindowRef::Entity(target),
                ..
            } if *target == window
        ));
        assert!(world.get::<ChildOf>(panels[0]).is_none());
    }

    #[test]
    fn every_probe_window_receives_one_centered_window_targeted_panel() {
        let mut app = content_app();
        let monitor = app.world_mut().spawn_empty().id();
        let windows = spawn_content_windows(app.world_mut(), monitor);
        app.update();
        app.update();

        for window in windows {
            assert_window_panel(app.world_mut(), window);
        }
    }

    #[test]
    fn every_probe_window_receives_one_active_clear_only_camera() {
        let mut app = content_app();
        let monitor = app.world_mut().spawn_empty().id();
        let windows = spawn_content_windows(app.world_mut(), monitor);
        app.update();

        let mut cameras = app.world_mut().query_filtered::<(
            &ProbeContentOwner,
            &Camera,
            &RenderTarget,
            &RenderLayers,
            &Msaa,
            &Tonemapping,
        ), With<ProbeWindowClearCamera>>();
        let cameras: Vec<_> = cameras.iter(app.world()).collect();
        assert_eq!(cameras.len(), windows.len());
        for window in windows {
            let (_, camera, target, layers, msaa, tonemapping) = cameras
                .iter()
                .find(|(owner, ..)| owner.window == window)
                .expect("probe window should have a clear camera");
            assert!(camera.is_active);
            assert_eq!(camera.order, PANEL_CLEAR_CAMERA_ORDER);
            assert!(matches!(
                camera.clear_color,
                ClearColorConfig::Custom(color) if color == Color::BLACK
            ));
            assert!(matches!(
                target,
                RenderTarget::Window(WindowRef::Entity(target)) if *target == window
            ));
            assert_eq!(*layers, &RenderLayers::none());
            assert_eq!(**msaa, Msaa::Off);
            assert_eq!(**tonemapping, Tonemapping::None);
        }
    }

    #[test]
    fn one_persistent_camera_enables_stable_panel_transparency() {
        let mut app = App::new();
        app.add_systems(Startup, spawn_transparency_camera);
        app.update();

        let mut cameras = app.world_mut().query_filtered::<
            (&Camera, &StableTransparency, &Tonemapping),
            With<ProbeTransparencyCamera>,
        >();
        let cameras: Vec<_> = cameras.iter(app.world()).collect();
        assert_eq!(cameras.len(), 1);
        assert!(!cameras[0].0.is_active);
        assert_eq!(*cameras[0].2, Tonemapping::None);
    }

    #[test]
    fn removing_window_removes_its_clear_camera() {
        let mut app = content_app();
        let monitor = app.world_mut().spawn_empty().id();
        let windows = spawn_content_windows(app.world_mut(), monitor);
        app.update();

        app.world_mut().despawn(windows[1]);
        app.update();

        let mut owners = app
            .world_mut()
            .query_filtered::<&ProbeContentOwner, With<ProbeWindowClearCamera>>();
        let camera_windows: Vec<_> = owners.iter(app.world()).map(|owner| owner.window).collect();
        assert_eq!(camera_windows.len(), windows.len() - 1);
        assert!(!camera_windows.contains(&windows[1]));
    }

    #[test]
    fn role_removal_and_readdition_does_not_duplicate_panel() {
        let mut app = content_app();
        let monitor = app.world_mut().spawn_empty().id();
        let windows = spawn_content_windows(app.world_mut(), monitor);
        app.update();

        app.world_mut()
            .entity_mut(windows[0])
            .remove::<PrimaryWindow>();
        app.world_mut().entity_mut(windows[0]).insert(PrimaryWindow);
        app.world_mut()
            .entity_mut(windows[1])
            .remove::<ManagedWindow>();
        app.world_mut()
            .entity_mut(windows[1])
            .insert(ManagedWindow {
                name: AUTOMATIC_WINDOW_KEY.into(),
            });
        app.update();

        assert_window_panel(app.world_mut(), windows[0]);
        assert_window_panel(app.world_mut(), windows[1]);
    }

    #[test]
    fn panel_metadata_identifies_borderless_window_without_a_title_bar() {
        let mut app = content_app();
        let monitor = app.world_mut().spawn_empty().id();
        let automatic = app
            .world_mut()
            .spawn((
                Window {
                    mode: WindowMode::BorderlessFullscreen(MonitorSelection::Index(TARGET_INDEX)),
                    ..default()
                },
                ManagedWindow {
                    name: AUTOMATIC_WINDOW_KEY.into(),
                },
                OnMonitor(monitor),
                current_monitor(),
            ))
            .id();
        app.update();

        let panel = owned_panels(app.world_mut(), automatic)[0];
        let metadata = app
            .world()
            .get::<ProbeWindowMetadata>(panel)
            .expect("panel should retain displayed metadata");
        assert_eq!(metadata.role, ProbeWindowRole::ManagedAutomatic);
        assert!(
            metadata
                .fields
                .iter()
                .any(|(label, value)| *label == "recovery"
                    && value.contains("automatic fallback and return"))
        );
        assert!(
            metadata
                .fields
                .iter()
                .any(|(label, value)| *label == "mode" && value.contains("Borderless"))
        );
    }

    #[test]
    fn reconstructed_window_receives_content_without_recovery_or_placement() {
        let mut app = content_app();
        let reconstructed = app
            .world_mut()
            .spawn((
                probe_window(AUTOMATIC_WINDOW_TITLE, WindowPosition::Automatic),
                ManagedWindow {
                    name: AUTOMATIC_WINDOW_KEY.into(),
                },
            ))
            .id();
        app.update();
        app.update();

        assert_window_panel(app.world_mut(), reconstructed);
        assert!(
            app.world()
                .get::<bevy_clerestory::WindowRecovery>(reconstructed)
                .is_none()
        );
        assert!(
            app.world()
                .get::<crate::setup::ProbePlacementRequested>(reconstructed)
                .is_none()
        );
    }
}
