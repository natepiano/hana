//! Condition support for JSONC keymaps.

use std::collections::HashSet;
use std::marker::PhantomData;

use bevy::app::App;
use bevy::app::Plugin;
use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::prelude::PreUpdate;
use bevy::prelude::Reflect;
use bevy::prelude::ReflectResource;
use bevy::prelude::Res;
use bevy::prelude::ResMut;
use bevy::prelude::Resource;
use bevy::prelude::State;
use bevy::prelude::States;
use bevy::reflect::ReflectDeserialize;
use bevy::reflect::ReflectSerialize;
use serde::Deserialize;
use serde::Serialize;
use strum::EnumMessage;
use strum::IntoEnumIterator;

use crate::Diagnostic;
use crate::DiagnosticKind;
use crate::DiagnosticOrigin;
use crate::DiagnosticSeverity;
use crate::KeymapLoadFailures;
use crate::KeymapSystems;
use crate::keymap_plugin::RegistryValidationFailed;

/// A condition name declared by a [`KeymapContext`] variant.
///
/// Applications define these names by deriving `strum::AsRefStr` with
/// `#[strum(serialize_all = "snake_case")]` on their context enum. Keymap parsing resolves the
/// authored text once, so the input path carries only an opaque handle.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Reflect, Serialize)]
#[reflect(opaque, Serialize, Deserialize)]
pub struct ConditionName(String);

impl ConditionName {
    /// Creates a condition name from application-authored keymap text.
    #[must_use]
    pub(crate) fn new(name: impl Into<String>) -> Self { Self(name.into()) }

    /// Borrows the condition text declared by the application context enum.
    #[must_use]
    pub fn as_str(&self) -> &str { &self.0 }
}

impl AsRef<str> for ConditionName {
    fn as_ref(&self) -> &str { self.as_str() }
}

/// Context-enum requirements shared by resource and state-backed keymaps.
///
/// This is a downstream extension point: each application derives the required traits on its
/// own context enum instead of implementing `KeymapContext` manually. Derive
/// `strum::EnumIter`, `strum::AsRefStr`, and `strum::EnumMessage`; `AsRefStr` supplies the
/// `AsRef<str>` bound below.
pub trait KeymapContext:
    AsRef<str> + Copy + EnumMessage + Eq + IntoEnumIterator + Send + Sync + 'static
{
}

impl<T> KeymapContext for T where
    T: AsRef<str> + Copy + EnumMessage + Eq + IntoEnumIterator + Send + Sync + 'static
{
}

/// The registered condition currently selected by the application context.
///
/// Reflection reads this resource so a remote client can ask which context is routing input.
/// [`ActiveConditionState::ResolvedCondition`] carries the runtime handle the input path selects
/// a compiled keymap with alongside the [`ConditionName`] the application authored, because a
/// handle alone is an integer a reader outside the process cannot map back to a context.
#[derive(Debug, Default, Reflect, Resource)]
#[reflect(Resource)]
pub struct ActiveCondition {
    state: ActiveConditionState,
}

impl ActiveCondition {
    /// Returns the routing state the application's context source has reached.
    #[must_use]
    pub const fn state(&self) -> &ActiveConditionState { &self.state }

    /// Records the condition the application's context now selects.
    ///
    /// Returns without writing when the state already holds `condition_handle`: the context
    /// sources call this whenever `Res::is_changed` reports a `ResMut` deref, and cloning `name`
    /// on every such frame would allocate inside [`KeymapSystems::UpdateActiveCondition`], which
    /// runs before [`KeymapSystems::Route`].
    pub(crate) fn resolve(&mut self, condition_handle: ConditionHandle, name: &ConditionName) {
        if matches!(
            self.state,
            ActiveConditionState::ResolvedCondition { handle, .. } if handle == condition_handle
        ) {
            return;
        }

        self.state = ActiveConditionState::ResolvedCondition {
            handle: condition_handle,
            name:   name.clone(),
        };
    }

    pub(crate) fn await_context(&mut self) { self.state = ActiveConditionState::AwaitingContext; }

    pub(crate) fn enable_global(&mut self) { self.state = ActiveConditionState::GlobalRouting; }
}

/// Which keymap scope [`ActiveCondition`] currently routes input through.
#[derive(Debug, Default, Reflect)]
pub enum ActiveConditionState {
    /// A context source is registered but has not reported a context yet, so no binding routes.
    #[default]
    AwaitingContext,
    /// No context source is registered, so every binding routes through the global matcher.
    GlobalRouting,
    /// The application's context resolved to this registered condition.
    ResolvedCondition {
        /// The compact registry identity the input path selects a compiled matcher with.
        #[reflect(ignore)]
        handle: ConditionHandle,
        /// The condition text the application's context enum declared.
        name:   ConditionName,
    },
}

/// A condition's compact registry identity.
///
/// The value is meaningful only to the [`ConditionRegistry`] that issued it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConditionHandle(usize);

impl ConditionHandle {
    /// Whether [`ConditionRegistry`] issued this handle.
    ///
    /// A handle that it did not issue reaches [`ActiveCondition`] only through
    /// [`Default::default`], which [`FromReflect`] uses to fill the `#[reflect(ignore)]` handle
    /// field of [`ActiveConditionState::ResolvedCondition`] when a remote client reconstructs the
    /// resource from a name alone.
    ///
    /// [`FromReflect`]: bevy::reflect::FromReflect
    pub(crate) const fn is_registry_issued(self) -> bool { self.0 != usize::MAX }
}

impl Default for ConditionHandle {
    /// Produces an index [`ConditionRegistry`] can never issue, so a reconstructed handle fails
    /// [`Self::is_registry_issued`] instead of naming the first registered condition.
    fn default() -> Self { Self(usize::MAX) }
}

/// The result of resolving authored condition text against the registry.
pub(crate) enum ConditionLookup<'registry> {
    /// The registry declared this condition.
    Registered {
        handle: ConditionHandle,
        name:   &'registry ConditionName,
    },
    /// No registered condition carries the requested text.
    UnregisteredName,
}

/// Stable condition metadata for schema and companion-file generation.
pub(crate) struct ConditionInfo<'registry> {
    pub(crate) name:        &'registry ConditionName,
    pub(crate) description: &'registry str,
}

/// The application-declared condition names and their resolved runtime handles.
#[derive(Default, Resource)]
pub(crate) struct ConditionRegistry {
    entries: Vec<ConditionEntry>,
}

impl ConditionRegistry {
    /// Registers every variant of `C` in declaration order.
    ///
    /// # Errors
    ///
    /// Returns diagnostics when a variant has no description or its name duplicates a previously
    /// registered name. A failed registration leaves the registry unchanged.
    pub(crate) fn register<C: KeymapContext>(&mut self) -> Result<(), Vec<Diagnostic>> {
        let mut declarations = Vec::new();
        let mut diagnostics = Vec::new();
        let mut names = HashSet::new();

        for condition in C::iter() {
            let condition_name = ConditionName::new(condition.as_ref());
            let description = condition
                .get_message()
                .filter(|message| !message.is_empty());
            let is_duplicate = !names.insert(condition_name.clone())
                || self
                    .entries
                    .iter()
                    .any(|condition_entry| condition_entry.name == condition_name);

            if is_duplicate {
                diagnostics.push(condition_diagnostic(
                    condition_name.as_str(),
                    DiagnosticSeverity::Failure,
                    format!(
                        "Condition `{}` is registered more than once.",
                        condition_name.as_str()
                    ),
                ));
            }

            match description {
                Some(description) => declarations.push(ConditionEntry {
                    name: condition_name,
                    description,
                }),
                None => diagnostics.push(condition_diagnostic(
                    condition_name.as_str(),
                    DiagnosticSeverity::Failure,
                    format!(
                        "Condition `{}` has no description. Add #[strum(message = \"…\")].",
                        condition_name.as_str()
                    ),
                )),
            }
        }

        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }

        self.entries.extend(declarations);

        Ok(())
    }

    /// Resolves authored condition text to its handle and the name the registry stores for it.
    #[must_use]
    pub(crate) fn lookup(&self, condition_name: &str) -> ConditionLookup<'_> {
        self.entries
            .iter()
            .position(|condition_entry| condition_entry.name.as_str() == condition_name)
            .map_or(ConditionLookup::UnregisteredName, |index| {
                ConditionLookup::Registered {
                    handle: ConditionHandle(index),
                    name:   &self.entries[index].name,
                }
            })
    }

    /// Iterates over registered condition metadata in declaration order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = ConditionInfo<'_>> {
        self.entries.iter().map(|condition_entry| ConditionInfo {
            name:        &condition_entry.name,
            description: condition_entry.description,
        })
    }
}

struct ConditionEntry {
    name:        ConditionName,
    description: &'static str,
}

pub(crate) fn sync_resource_condition<C: KeymapContext + Resource>(
    context: Option<Res<C>>,
    condition_registry: Res<ConditionRegistry>,
    mut active_condition: ResMut<ActiveCondition>,
) {
    let Some(context) = context else {
        return;
    };

    if context.is_changed() {
        update_active_condition(*context, &condition_registry, &mut active_condition);
    }
}

pub(crate) fn sync_state_condition<C: KeymapContext + States>(
    state: Option<Res<State<C>>>,
    condition_registry: Res<ConditionRegistry>,
    mut active_condition: ResMut<ActiveCondition>,
) {
    let Some(state) = state else {
        return;
    };

    if state.is_changed() {
        update_active_condition(*state.get(), &condition_registry, &mut active_condition);
    }
}

fn update_active_condition<C: KeymapContext>(
    context: C,
    condition_registry: &ConditionRegistry,
    active_condition: &mut ActiveCondition,
) {
    if let ConditionLookup::Registered { handle, name } =
        condition_registry.lookup(context.as_ref())
    {
        active_condition.resolve(handle, name);
    }
}

pub(crate) fn condition_diagnostic(
    condition_name: &str,
    severity: DiagnosticSeverity,
    message: String,
) -> Diagnostic {
    Diagnostic {
        origin: DiagnosticOrigin::ContextRegistration,
        byte_range: 0..0,
        line: 0,
        column: 0,
        block_index: 0,
        context: condition_name.to_owned(),
        original_keystroke: String::new(),
        command_id: String::new(),
        kind: DiagnosticKind::Context,
        severity,
        message,
        suggestions: Vec::new(),
    }
}

pub(crate) struct ResourceContextPlugin<C> {
    keymap_plugin: crate::KeymapPlugin,
    marker:        PhantomData<fn() -> C>,
}

impl<C> ResourceContextPlugin<C> {
    pub(crate) fn new(keymap_plugin: crate::KeymapPlugin) -> Self {
        Self {
            keymap_plugin,
            marker: PhantomData,
        }
    }
}

impl<C> Plugin for ResourceContextPlugin<C>
where
    C: KeymapContext + Resource,
{
    fn build(&self, app: &mut App) {
        self.keymap_plugin.install(app);
        if register_context::<C>(app, ContextSource::Resource).is_ok() {
            app.add_systems(
                PreUpdate,
                sync_resource_condition::<C>.in_set(KeymapSystems::UpdateActiveCondition),
            );
        }
    }

    fn finish(&self, app: &mut App) { crate::KeymapPlugin::finish_assembly(app); }

    fn is_unique(&self) -> bool { false }
}

pub(crate) struct StateContextPlugin<C> {
    keymap_plugin: crate::KeymapPlugin,
    marker:        PhantomData<fn() -> C>,
}

impl<C> StateContextPlugin<C> {
    pub(crate) fn new(keymap_plugin: crate::KeymapPlugin) -> Self {
        Self {
            keymap_plugin,
            marker: PhantomData,
        }
    }
}

impl<C> Plugin for StateContextPlugin<C>
where
    C: KeymapContext + States,
{
    fn build(&self, app: &mut App) {
        self.keymap_plugin.install(app);
        if register_context::<C>(app, ContextSource::State).is_ok() {
            app.add_systems(
                PreUpdate,
                sync_state_condition::<C>.in_set(KeymapSystems::UpdateActiveCondition),
            );
        }
    }

    fn finish(&self, app: &mut App) { crate::KeymapPlugin::finish_assembly(app); }

    fn is_unique(&self) -> bool { false }
}

#[derive(Resource)]
struct ContextSourceInstalled(ContextSource);

#[derive(Clone, Copy)]
pub(crate) enum ContextSource {
    Resource,
    State,
    Derived,
}

impl ContextSource {
    const fn description(self) -> &'static str {
        match self {
            Self::Resource => "resource-backed",
            Self::State => "state-backed",
            Self::Derived => "derived",
        }
    }
}

pub(crate) fn register_context<C: KeymapContext>(
    app: &mut App,
    context_source: ContextSource,
) -> Result<(), Vec<Diagnostic>> {
    crate::KeymapPlugin::install_runtime(app);
    if let Some(previous_context_source) = app.world().get_resource::<ContextSourceInstalled>() {
        let diagnostics = vec![condition_diagnostic(
            "",
            DiagnosticSeverity::Failure,
            format!(
                "A {} keymap context is already registered; a keymap accepts exactly one context source.",
                previous_context_source.0.description()
            ),
        )];
        retain_context_diagnostics(app, &diagnostics);
        return Err(diagnostics);
    }
    app.world_mut()
        .insert_resource(ContextSourceInstalled(context_source));
    app.world_mut()
        .resource_mut::<ActiveCondition>()
        .await_context();
    app.init_resource::<ConditionRegistry>();

    let result = app
        .world_mut()
        .resource_mut::<ConditionRegistry>()
        .register::<C>();

    if let Err(diagnostics) = &result {
        app.world_mut().insert_resource(RegistryValidationFailed);
        retain_context_diagnostics(app, diagnostics);
    }

    result
}

pub(crate) fn retain_context_diagnostics(app: &mut App, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        match diagnostic.severity {
            DiagnosticSeverity::Failure => bevy::log::error!("{}", diagnostic.message),
            DiagnosticSeverity::Advisory => bevy::log::warn!("{}", diagnostic.message),
        }
    }
    app.init_resource::<KeymapLoadFailures>();
    app.world_mut()
        .resource_mut::<KeymapLoadFailures>()
        .retained_diagnostics
        .extend(diagnostics.iter().cloned());
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use bevy::ecs::system::IntoSystem;
    use bevy::ecs::system::System;
    use bevy::prelude::App;
    use bevy::prelude::NextState;
    use bevy::prelude::Resource;
    use bevy::prelude::States;
    use bevy::reflect::PartialReflect;
    use bevy::reflect::ReflectDeserialize;
    use bevy::reflect::ReflectRef;
    use bevy::reflect::ReflectSerialize;
    use bevy::reflect::TypeRegistry;
    use bevy::state::app::AppExtStates;
    use bevy::state::app::StatesPlugin;
    use strum::AsRefStr;
    use strum::EnumIter;
    use strum::EnumMessage;

    use super::ActiveCondition;
    use super::ActiveConditionState;
    use super::ConditionHandle;
    use super::ConditionLookup;
    use super::ConditionName;
    use super::ConditionRegistry;
    use super::KeymapContext;
    use super::sync_resource_condition;
    use crate::DiagnosticKind;
    use crate::DiagnosticOrigin;
    use crate::DiagnosticSeverity;
    use crate::KeymapPlugin;

    /// Frames the allocation test runs before measuring, so every routing structure the sync
    /// system touches is already populated.
    const SYNC_WARMUP_FRAMES: usize = 3;

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum ResourceContext {
        #[strum(message = "While flying the ship")]
        Flying,
        #[strum(message = "While the pause menu is open")]
        Paused,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, Hash, PartialEq, States)]
    #[strum(serialize_all = "snake_case")]
    enum StateContext {
        #[strum(message = "While flying the ship")]
        Flying,
        #[strum(message = "While the pause menu is open")]
        Paused,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum MissingDescriptionContext {
        Flying,
    }

    /// The three context names hana ships, spelled exactly as its keymap files author them.
    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum ShippedContext {
        #[strum(message = "While no modal interaction is running")]
        Resting,
        #[strum(message = "While a dimension lock is engaged")]
        DimensionLock,
        #[strum(message = "While a dimension lock is rotating")]
        DimensionLockRotation,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq)]
    #[strum(serialize_all = "snake_case")]
    enum FirstDuplicateContext {
        #[strum(message = "While flying the first context")]
        Flying,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq)]
    #[strum(serialize_all = "snake_case")]
    enum SecondDuplicateContext {
        #[strum(message = "While flying the second context")]
        Flying,
    }

    #[test]
    fn context_registration_failures_name_no_keymap_source() {
        let diagnostic = super::condition_diagnostic(
            "dimension_lock",
            DiagnosticSeverity::Failure,
            String::from("Duplicate keymap context name."),
        );

        assert_eq!(diagnostic.origin, DiagnosticOrigin::ContextRegistration);
    }

    #[test]
    fn derived_state_context_satisfies_keymap_context_without_a_resource_impl() {
        assert_keymap_context::<StateContext>();
    }

    #[test]
    fn registry_preserves_variant_names_descriptions_and_declaration_order() {
        let mut condition_registry = ConditionRegistry::default();

        assert!(condition_registry.register::<ResourceContext>().is_ok());

        let conditions = condition_registry
            .iter()
            .map(|condition_info| (condition_info.name.as_str(), condition_info.description))
            .collect::<Vec<_>>();
        assert_eq!(
            conditions,
            vec![
                ("flying", "While flying the ship"),
                ("paused", "While the pause menu is open"),
            ]
        );
    }

    #[test]
    fn missing_condition_description_produces_a_context_diagnostic() {
        let mut app = App::new();
        app.add_plugins(KeymapPlugin::new().for_context::<MissingDescriptionContext>());

        let diagnostics = &app
            .world()
            .resource::<crate::KeymapLoadFailures>()
            .retained_diagnostics;

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::Context
                && diagnostic.context == "flying"
                && diagnostic.message.contains("flying")
        }));
        assert_eq!(
            app.world().resource::<ConditionRegistry>().iter().count(),
            0
        );
    }

    #[test]
    fn duplicate_condition_names_are_rejected_without_replacing_the_first_registration() {
        let mut condition_registry = ConditionRegistry::default();

        assert!(
            condition_registry
                .register::<FirstDuplicateContext>()
                .is_ok()
        );
        let diagnostics = condition_registry
            .register::<SecondDuplicateContext>()
            .err();

        assert!(diagnostics.as_ref().is_some_and(|diagnostics| {
            diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == DiagnosticKind::Context
                    && diagnostic.context == "flying"
                    && diagnostic.message.contains("registered more than once")
            })
        }));
        assert_eq!(
            condition_registry
                .iter()
                .map(|condition_info| condition_info.description)
                .collect::<Vec<_>>(),
            vec!["While flying the first context"]
        );
    }

    #[test]
    fn resource_context_changes_update_the_active_condition_handle() -> Result<(), String> {
        let mut app = App::new();
        app.insert_resource(ResourceContext::Flying)
            .add_plugins(KeymapPlugin::new().for_context::<ResourceContext>());

        app.update();
        assert_active_condition(&app, "flying")?;

        *app.world_mut().resource_mut::<ResourceContext>() = ResourceContext::Paused;
        app.update();
        assert_active_condition(&app, "paused")
    }

    /// A context resource an application writes every frame reports `Res::is_changed` every frame,
    /// so the sync system must reach the routing state without allocating.
    #[test]
    fn an_unchanged_context_syncs_without_allocating() -> Result<(), String> {
        let mut app = App::new();
        app.insert_resource(ResourceContext::Flying)
            .add_plugins(KeymapPlugin::new().for_context::<ResourceContext>());
        app.update();

        let mut sync_condition =
            IntoSystem::into_system(sync_resource_condition::<ResourceContext>);
        sync_condition.initialize(app.world_mut());
        for _ in 0..SYNC_WARMUP_FRAMES {
            *app.world_mut().resource_mut::<ResourceContext>() = ResourceContext::Flying;
            sync_condition
                .run((), app.world_mut())
                .map_err(|error| format!("the condition sync system did not run: {error}"))?;
        }

        *app.world_mut().resource_mut::<ResourceContext>() = ResourceContext::Flying;
        let allocations_before = crate::TEST_ALLOCATOR.allocation_count();
        let sync_result = sync_condition.run((), app.world_mut());
        let allocations_after = crate::TEST_ALLOCATOR.allocation_count();

        sync_result.map_err(|error| format!("the condition sync system did not run: {error}"))?;
        assert_eq!(allocations_after - allocations_before, 0);
        Ok(())
    }

    #[test]
    fn state_context_changes_update_the_active_condition_handle() -> Result<(), String> {
        let mut app = App::new();
        app.add_plugins(StatesPlugin)
            .insert_state(StateContext::Flying)
            .add_plugins(KeymapPlugin::new().for_state_context::<StateContext>());

        app.update();
        assert_active_condition(&app, "flying")?;

        app.world_mut()
            .resource_mut::<NextState<StateContext>>()
            .set(StateContext::Paused);
        app.update();
        app.update();
        assert_active_condition(&app, "paused")
    }

    #[test]
    fn missing_state_resource_leaves_the_active_condition_unchanged() -> Result<(), String> {
        let mut app = App::new();
        app.add_plugins(KeymapPlugin::new().for_state_context::<StateContext>());
        let paused = registered_condition(&app, "paused")?;
        app.world_mut()
            .resource_mut::<ActiveCondition>()
            .resolve(paused, &ConditionName::new("paused"));

        app.update();

        assert!(matches!(
            app.world().resource::<ActiveCondition>().state(),
            ActiveConditionState::ResolvedCondition { handle, name }
                if *handle == paused && name.as_str() == "paused"
        ));
        Ok(())
    }

    #[test]
    fn every_shipped_context_name_reflects_as_its_authored_string() -> Result<(), String> {
        let mut app = App::new();
        app.insert_resource(ShippedContext::Resting)
            .add_plugins(KeymapPlugin::new().for_context::<ShippedContext>());

        for shipped_context in [
            ShippedContext::Resting,
            ShippedContext::DimensionLock,
            ShippedContext::DimensionLockRotation,
        ] {
            *app.world_mut().resource_mut::<ShippedContext>() = shipped_context;
            app.update();

            let active_condition = app.world().resource::<ActiveCondition>();
            let reflected_name = reflected_condition_name(active_condition)?;

            assert_eq!(reflected_name, shipped_context.as_ref());
        }
        Ok(())
    }

    #[test]
    fn a_condition_name_reflects_opaquely_and_carries_its_serde_type_data() -> Result<(), String> {
        let condition_name = ConditionName::new("dimension_lock_rotation");
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<ConditionName>();
        let registration = type_registry
            .get(TypeId::of::<ConditionName>())
            .ok_or_else(|| String::from("ConditionName is not registered"))?;

        assert!(matches!(
            condition_name.reflect_ref(),
            ReflectRef::Opaque(_)
        ));
        assert!(registration.data::<ReflectSerialize>().is_some());
        assert!(registration.data::<ReflectDeserialize>().is_some());
        assert_eq!(
            serde_json::to_string(&condition_name)
                .map_err(|error| format!("condition name did not serialize: {error}"))?,
            "\"dimension_lock_rotation\""
        );
        Ok(())
    }

    fn assert_keymap_context<C: KeymapContext>() {}

    fn registered_condition(app: &App, condition_name: &str) -> Result<ConditionHandle, String> {
        match app
            .world()
            .resource::<ConditionRegistry>()
            .lookup(condition_name)
        {
            ConditionLookup::Registered { handle, .. } => Ok(handle),
            ConditionLookup::UnregisteredName => Err(format!(
                "the test context did not register {condition_name}"
            )),
        }
    }

    fn reflected_condition_name(active_condition: &ActiveCondition) -> Result<String, String> {
        let ReflectRef::Struct(reflected_condition) = active_condition.reflect_ref() else {
            return Err(String::from("ActiveCondition does not reflect as a struct"));
        };
        let reflected_state = reflected_condition
            .field("state")
            .ok_or_else(|| String::from("ActiveCondition has no reflected state field"))?;
        let ReflectRef::Enum(reflected_state) = reflected_state.reflect_ref() else {
            return Err(String::from(
                "ActiveConditionState does not reflect as an enum",
            ));
        };

        if reflected_state.variant_name() != "ResolvedCondition" {
            return Err(format!(
                "the active condition reflected as {}",
                reflected_state.variant_name()
            ));
        }

        reflected_state
            .field("name")
            .ok_or_else(|| String::from("ResolvedCondition has no reflected name field"))?
            .try_downcast_ref::<ConditionName>()
            .map(|condition_name| condition_name.as_str().to_owned())
            .ok_or_else(|| String::from("the reflected name is not a ConditionName"))
    }

    fn assert_active_condition(app: &App, condition_name: &str) -> Result<(), String> {
        let expected_handle = registered_condition(app, condition_name)?;

        assert_ne!(
            registered_condition(app, "flying")?,
            registered_condition(app, "paused")?
        );
        assert!(matches!(
            app.world().resource::<ActiveCondition>().state(),
            ActiveConditionState::ResolvedCondition { handle, name }
                if *handle == expected_handle && name.as_str() == condition_name
        ));
        Ok(())
    }
}
