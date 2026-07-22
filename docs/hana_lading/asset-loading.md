# hana_lading — startup asset loading

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Builds the
> `hana_lading` startup disk-asset-loading crate in the bevy_hana workspace.
> Hana application integration is intentionally separate in
> `/Users/natemccoy/rust/hana/docs/hana/asset-loading.md`.

## Delegation Context
<!-- Shared across all phases. /plan:delegate prepends this to every dispatch. -->

- **Project:** bevy_hana (`/Users/natemccoy/rust/bevy_hana`), the Bevy library
  workspace published at `github.com/natepiano/hana.git`. `hana_lading` becomes
  a new `crates/*` member. This plan will be implemented in a worktree that has
  not yet been created.
- **Stack:** Rust edition 2024 and resolver `"3"`, inherited from the workspace.
  Bevy is pinned to 0.19.0. Use the workspace's granular Bevy crates; do not use
  `/Users/natemccoy/rust/bevy` (0.20.0-dev) to verify 0.19 APIs.
- **Layout:** Root `Cargo.toml` uses `members = ["crates/*"]` and excludes
  `crates/hana` and `vendor/clay-layout`. `[workspace.dependencies]` already has
  `bevy`, `bevy_app`, `bevy_ecs`, and `tracing`; Phase 1 adds `bevy_asset`.
  Follow `crates/hana_diegetic/Cargo.toml` for sibling manifest conventions:
  workspace-inherited authors/edition/license/repository/version, per-crate
  metadata, `[lints] workspace = true`, and explicit `[[example]]` entries.
- **Canonical examples:** `docs/fairy_dust/canonical-example.md` defines the
  normal Fairy Dust example shell and screen-panel conventions. Phase 4 adds a
  narrow takeover/error-example exception and builds two examples that visibly
  demonstrate both the loading workflow and its failure protections. Outside
  that exception, the canonical chain includes BRP extras, saved window
  position, studio lighting, ground plane, stable transparency, camera-control
  panel, title bar, and camera home. Custom panels use
  `DiegeticPanel::screen()`, `screen_panel_material()`, and
  `screen_panel_frame(...)`.
- **Key files:**
  - `Cargo.toml` — workspace dependencies and strict inherited lints.
  - `crates/hana_diegetic/Cargo.toml` — sibling manifest reference.
  - `docs/fairy_dust/canonical-example.md` — example-authoring contract.
  - `crates/fairy_dust/src/lib.rs` — `sprinkle_example()`.
  - `crates/fairy_dust/src/screen_panels/mod.rs` —
    `screen_panel_material()` and `screen_panel_frame(...)`.
  - `crates/hana_diegetic/src/panel/diegetic_panel.rs` —
    `DiegeticPanel::screen()`.
- **Build:** `cargo build --release --workspace --all-features --examples`.
- **Test:** `cargo nextest run --all-features --workspace --tests`.
- **Lint:** Use the `clippy` skill. Workspace lint inheritance includes
  `missing_docs = "deny"` and strict Clippy rules.
- **Style:** Load the project Rust style before writing Rust code. Because this
  repository's origin is owned by `natepiano`, use `cargo +nightly fmt`, never
  plain `cargo fmt`.

### Library boundary

`hana_lading` provides generic startup disk-asset loading, completion tracking,
and failure evidence. It does not decide whether an application should continue
after a failure. Applications own severity vocabulary, durable failure records,
state transitions, and degraded behavior.

The public loading path is `DiskAssetLoader`. It wraps Bevy's `AssetServer` and
records every returned handle, allowing `hana_lading` to wait for recursive
dependencies and report terminal success or failure exactly once. The public API
must not expose `AssetServer`, `LoadBuilder`, `Assets<T>`, `LoadState`, or
tracking internals because those would permit untracked loads.

Production dependencies are limited to `bevy_app`, `bevy_asset`, `bevy_ecs`,
and `tracing`. The umbrella `bevy`, `fairy_dust`, and `hana_diegetic` are allowed
only as test/example dev-dependencies.

Out of scope: hot reload or unload watching, dynamically registered runtime
asset sets, a derive macro, application startup policy, and runtime-generated
media such as camera feeds or screen capture.

All public event payloads have private fields and read-only accessors. Event
construction remains crate-private so one Bevy observer cannot mutate evidence
seen by later observers. The crate registers no application-policy observers;
its own systems only load, poll, notify, and tear down tracking state.

`LadingPlugin` may also be installed directly by an application that registers
zero asset sets but still needs the two global completion events.

Targeted developer-error panics require scoped lint allowances with reasons.
The new crate also requires `LICENSE-MIT`, `LICENSE-APACHE`, a README, crate-level
API documentation, and an overview under `docs/hana_lading/`.

## Phases

### Phase 1 — Crate scaffold and public API · status: done (`9a8ae890`)

#### Work Order

**Goal:** `hana_lading` exists as a workspace member whose complete public API
surface compiles and is documented; runtime behavior follows in Phase 2.

**Spec:**

Add `bevy_asset = { version = "0.19.0", default-features = false }` to the root
workspace dependencies.

Create `crates/hana_lading/` following the sibling manifest conventions.
Description: `"Startup disk-asset loading, completion tracking, and failure
reporting for Bevy."` Production dependencies are exactly `bevy_app`,
`bevy_asset`, `bevy_ecs`, and `tracing`, all workspace-inherited. Add a stub
README and both workspace license files.

Implement and publicly document this API; module layout is the implementer's
choice and `lib.rs` re-exports every public item:

```rust
pub trait DiskAssets: Resource + Sized {
    /// Starts every load through the tracked loader and returns the resource
    /// that owns the resulting handles.
    fn load(loader: &mut DiskAssetLoader<'_>) -> Self;
}

pub struct DiskAssetLoader<'a> { /* AssetServer reference + recorded handles */ }

impl DiskAssetLoader<'_> {
    pub fn load<'p, A: Asset>(&mut self, path: impl Into<AssetPath<'p>>) -> Handle<A>;

    pub fn load_with_settings<'p, A: Asset, S: Settings>(
        &mut self,
        path: impl Into<AssetPath<'p>>,
        settings: impl Fn(&mut S) + Send + Sync + 'static,
    ) -> Handle<A>;
}

pub struct LadingPlugin;
pub struct DiskAssetsPlugin<T: DiskAssets>(PhantomData<fn() -> T>);

pub struct Loaded<T: DiskAssets>;
pub struct LoadFailed<T: DiskAssets>;
pub struct AssetSetLoadFailed;
pub struct AllSetsLoaded;
pub struct AllSetsResolved;
pub struct LoadProgress;
```

- `DiskAssetLoader::load` and `load_with_settings` are the only tracked load
  paths. GLTF labels use an `AssetPath`, such as
  `GltfAssetLabel::Scene(0).from_asset(path)`.
- Recording retains one strong-handle reference per load while returning the
  typed handle to the asset-set resource.
- Convert input to `AssetPath` before loading. Reject a returned handle whose
  `path()` is absent with a targeted panic naming the attempted path; otherwise
  it would never resolve through path-based tracking.
- `DiskAssetsPlugin<T>` has a manual `Default` implementation without a
  `T: Default` bound. `LoadFailed<T>` uses `PhantomData<fn() -> T>`.
- `LoadFailed<T>` exposes `tracked_path()` and `error()`.
  `AssetSetLoadFailed` exposes `set_type_id()`, `set_name()`, `tracked_path()`,
  and `error()`. Errors are shared as `Arc<AssetLoadError>`.
- `AllSetsResolved::failures()` reports failed set count. `LoadProgress` keeps
  private `total`, `resolved`, and `failures` fields and exposes `loaded()` as
  `resolved - failures` plus `total()`.
- Do not add run-condition helpers. Consumers use `On<Loaded<T>>`,
  `On<AllSetsLoaded>`, `On<AllSetsResolved>`, and their own application state.
- Events derive or implement what Bevy 0.19 requires for `commands.trigger` and
  `On<E>`. Plugin bodies may remain minimal until Phase 2.

**Files:**

- `Cargo.toml`
- `crates/hana_lading/Cargo.toml`
- `crates/hana_lading/README.md`
- `crates/hana_lading/LICENSE-MIT`
- `crates/hana_lading/LICENSE-APACHE`
- `crates/hana_lading/src/lib.rs` and implementation modules

**Acceptance gate:** Workspace release build with all features/examples passes;
the `clippy` skill passes; the crate has exactly the production dependency set
above; all public API is documented.

#### Retrospective

**What worked:**

- `crates/hana_lading/` now exposes the planned loading, plugin, progress, and
  completion API with exactly four production dependencies.
- Release build, 1,221 nextest tests, lint, rustdoc, formatting, and a standalone
  plugin-registration smoke executable passed.

**What deviated from the plan:**

- `docs/hana_lading/overview.md` was scaffolded now so the new public API has an
  overview; Phase 4 will complete it alongside the examples.
- Review corrected the pathless-handle panic contract and made `LoadProgress` a
  resource rather than an event before checkpointing.

**Surprises:**

- A standalone granular-Bevy smoke executable must select a reflection
  auto-registration backend; the workspace's umbrella Bevy users already do.

**Implications for remaining phases:**

- Phase 2 starts from `disk_asset_loader.rs`, `events.rs`, and
  `lading_plugin.rs`; `LoadProgress` is already a defaultable resource and the
  plugin stubs already establish their registration relationship.
- Phase 4 should expand the existing `docs/hana_lading/overview.md` instead of
  creating a second overview file.

#### Phase 1 Review

- Phase 2 now carries the shipped module/API constraints, creates the required
  crate-private construction seams, and activates Bevy's reflection backend for
  its isolated zero-set test.
- Phase 3 now names its isolated asset-test configuration and replaces duplicate
  clean-order coverage with recursive-dependency coverage.
- Phase 4 now expands the existing overview and names its state, PNG, fixture,
  and isolated-example requirements.

### Phase 2 — Runtime machinery · status: done (`22edae32`)

#### Work Order

**Goal:** Registering `DiskAssetsPlugin<T>` loads, tracks, and finalizes asset
sets end-to-end with deterministic terminal-event ordering. A zero-set app also
completes.

**Spec:**

- Add `bevy` as a workspace-inherited test-only dev-dependency now so isolated
  `hana_lading` tests select the workspace's reflection auto-registration
  backend. Phase 3 extends this same dev-dependency with the image features its
  fixtures require.
- Add crate-private seams in the Phase 1 modules: `disk_asset_loader.rs` owns
  loader construction and extraction of its recorded `Vec<UntypedHandle>`;
  `events.rs` owns construction of private-field events plus mutation of
  `LoadProgress`. Keep those seams inaccessible to downstream crates.

- `LadingPlugin`, auto-added by every `DiskAssetsPlugin<T>` after checking
  `is_plugin_added`, initializes `LoadProgress`; configures `Update` ordering
  `AssetPoll` → deferred-command flush → `AssetFinalize` within an
  `AssetTracking` set gated by `resource_exists::<LoadProgress>`; and installs
  `finish_empty` in `PostStartup` plus `finalize_batch` in `AssetFinalize`. The
  parent set's condition is evaluated once per frame for all pollers.
- `DiskAssetsPlugin<T>` increments the total set count and installs
  `load_set::<T>` in `PreStartup` and `check_set::<T>` in `AssetPoll`. Normal
  plugin uniqueness rejects duplicate registration. `Plugin::finish` asserts
  that `AssetServer` exists; `load_set` also accepts `Option<Res<AssetServer>>`
  and emits a targeted diagnostic for update-only test harnesses.
- `load_set::<T>` constructs `DiskAssetLoader`, calls `T::load`, and inserts the
  returned resource plus internal `Tracked<T>`. Recording zero handles is a
  developer error and fails loudly with the concrete set type.
- Internal `Tracked<T>` owns `Vec<UntypedHandle>`, load-start `Instant`, and a
  one-shot slow-load-notice state. Strong handles already own their asset paths;
  create an owned path only when emitting a failure. The final set count is
  established during app construction, so global finalization cannot run early.
- `check_set::<T>` returns when `Tracked<T>` is absent. For every pending handle,
  call `AssetServer::get_load_states` once and consider root, direct dependency,
  and recursive dependency states. Recursive dependencies must finish before a
  set succeeds.
  - When every state is loaded, increment `resolved`, trigger `Loaded<T>`, and
    remove `Tracked<T>`.
  - On the first failed handle in declaration order, increment `resolved` and
    `failures`, log the error, trigger `AssetSetLoadFailed` and then
    `LoadFailed<T>`, and remove `Tracked<T>`. The reported tracked root is the
    best public evidence available because Bevy does not expose a public map
    from a requested root to the particular recursive dependency that failed.
  - After a named threshold of approximately ten seconds, log the concrete set
    type and root paths once. Valid load time remains unbounded.
- `finalize_batch` runs after deferred observer commands. When
  `resolved == total`, it triggers `AllSetsLoaded` only for zero failures, then
  always triggers `AllSetsResolved`, then removes `LoadProgress`.
  `finish_empty` emits the same clean sequence when `total == 0`.
- Document this event contract: the set resource exists before `Loaded<T>`;
  tracked roots and recursive dependencies are loaded, though GPU preparation
  is not promised; progress updates precede observers; final per-set observers
  complete before global completion; each set emits exactly one mutually
  exclusive terminal event; cross-set order is unspecified; only direct
  resource mutations are visible to the following completion observer.
- Add an in-crate zero-set smoke test proving `AllSetsLoaded` precedes
  `AllSetsResolved { failures: 0 }` and tracking state is removed.

**Files:**

- `crates/hana_lading/Cargo.toml`
- `crates/hana_lading/src/disk_asset_loader.rs`
- `crates/hana_lading/src/events.rs`
- `crates/hana_lading/src/lading_plugin.rs`
- an in-crate or integration zero-set smoke test

**Constraints from prior phases:**

- Phase 1 created the public API in `disk_asset_loader.rs`, `events.rs`, and
  `lading_plugin.rs`; keep those module paths instead of rediscovering a layout.
- `LoadProgress` is already a defaultable `Resource` with private `total`,
  `resolved`, and `failures` fields. Public completion-event fields are private
  and must remain constructible only inside the crate.
- `DiskAssetsPlugin<T>` already conditionally adds `LadingPlugin` after
  `is_plugin_added`; preserve that registration relationship while adding the
  runtime systems.
- `DiskAssetLoader` already retains strong untyped handle clones and rejects a
  pathless returned handle with the documented, reason-allowed targeted panic.

**Acceptance gate:** Workspace build passes; `cargo nextest run --all-features
--workspace --tests` passes including zero-set completion; the `clippy` skill
passes. `cargo nextest run -p hana_lading --all-features` also passes so
workspace feature unification cannot mask the test-only reflection backend.

#### Retrospective

**What worked:**

- `DiskAssetsPlugin<T>` now performs the complete startup load, recursive-state
  polling, typed and type-erased terminal notification, global finalization,
  slow-load notice, and tracking teardown described by the Work Order.
- The release examples build, all 1,222 workspace tests pass, the isolated
  `hana_lading` test passes, lint and rustdoc pass, and a standalone executable
  loaded a real file through Bevy before observing the promised completion
  order and `LoadProgress` lifetime.

**What deviated from the plan:**

- The final style pass replaced a Boolean loading-state accumulator with the
  existing `SetResolution` enum so the first failed handle remains typed and in
  declaration order throughout the fold.
- The missing-`AssetServer` and empty-set diagnostics use assertions where the
  Work Order calls for assertions; the remaining explicit `panic!` sites retain
  their scoped, reasoned allowances.

**Surprises:**

- The first delegated verification report exited while describing a workspace
  test as still running. The orchestrator therefore reran every acceptance gate
  itself instead of accepting that report as evidence.

**Implications for remaining phases:**

- Phase 3 can test the public runtime directly; the only private test access it
  still needs is the existing pathless-handle recording seam beside
  `DiskAssetLoader`.
- Recursive-dependency and multiple-failure tests should exercise the shipped
  `SetResolution` fold so declaration-order failure evidence remains covered.

#### Phase 2 Review

- Phase 3 now requires bounded asynchronous update helpers, a one-set
  declaration-order failure test, and successful `load_with_settings` coverage.
- Phase 4 now verifies each running example through BRP and terminates it through
  `brp_extras/shutdown` after collecting the observable result.
- Phase 4 carries a deferred decision about adding a Fairy Dust asset-root
  constructor because `sprinkle_example()` installs `AssetPlugin` before its
  builder is available.

### Phase 3 — Contract test suite · status: done (`b9a9d7a7`)

#### Work Order

**Goal:** Integration tests prove the library's success, failure, ordering, and
exit-on-failure protections.

**Spec:**

Extend the workspace-inherited `bevy` dev-dependency from Phase 2 with only the
`bevy_asset`, `bevy_image`, and `png` features required by the fixtures. Keep
fixtures under `crates/hana_lading/tests/assets/`; configure each test app's
`AssetPlugin::file_path` from `CARGO_MANIFEST_DIR/tests/assets` so tests do not
depend on the invoking directory. Missing-file cases must assert that their
chosen file is genuinely absent. Exercise the same Bevy 0.19 asynchronous asset
failure path used in production: with the image loader registered, the load
initially returns a `Loading` handle; the asynchronous read produces
`AssetReaderError::NotFound`; Bevy records failed root, dependency, and recursive
states before `hana_lading` polls in `Update`.

Share a bounded update helper across integration tests. It must stop on the
expected terminal observation or fail at a named deadline so asynchronous asset
work cannot hang nextest. The recursive-dependency loader must use an explicit
test-controlled release mechanism for its child rather than relying on timing.
The exit-pattern app uses `ScheduleRunnerPlugin::run_loop` plus an independent
deadline system that exits with a non-error result if the expected asset failure
does not arrive; the test then requires `app.run()` to return the error variant.

Required tests:

1. `success_two_sets`: each set emits `Loaded<T>` once; clean global events are
   ordered; progress is readable during global observers and removed afterward.
   At least one handle uses `load_with_settings`, and the loaded test asset proves
   that its custom loader setting was applied.
2. `failure_missing_file`: generic and typed failures agree on path and error;
   `AllSetsLoaded` is absent; global resolution reports one failed set.
3. `two_failures_one_frame`: direct `ResMut` recording sees both set failures
   before the global resolution observer.
4. `mixed_outcome`: the successful set remains usable while the other fails;
   only `AllSetsResolved` fires globally.
5. `recursive_dependencies_gate_and_fail`: a custom test asset loader requests a
   child through its `LoadContext`; the root cannot complete while that child is
   pending, and a failed child produces the set's terminal failure instead of a
   success event.
6. `empty_set_panics`: an empty `DiskAssets` declaration names its concrete type.
7. `pathless_rejection_panics`: an in-module unit test in
   `src/disk_asset_loader.rs` drives the private recording seam and proves a
   pathless handle fails at load declaration rather than hanging tracking.
8. `exit_on_failure_pattern`: an app using `ScheduleRunnerPlugin` converts a
   generic failure into `AppExit::error()` through `MessageWriter<AppExit>` and
   `app.run()` returns the error variant.
9. `first_failure_follows_declaration_order`: one set declares at least two
   missing roots that fail in the same polling pass; typed and generic evidence
   both name the first declared root.

**Files:**

- `crates/hana_lading/Cargo.toml`
- `crates/hana_lading/src/disk_asset_loader.rs`
- `crates/hana_lading/tests/`
- `crates/hana_lading/tests/assets/`

**Constraints from prior phases:**

- Phase 2 owns runtime construction and terminal-event ordering; tests use only
  public APIs except the pathless-handle unit test, which remains beside the
  private loader-recording seam.
- Phase 2 already adds the workspace `bevy` dev-dependency to select a reflection
  backend for isolated package tests; extend its features rather than creating a
  second test dependency.
- Public completion-event fields remain read-only, and `LoadProgress` is a
  resource that observers may read until global finalization removes it.
- `check_set` folds each handle into `SetResolution` and preserves the first
  failed root by declaration order; the multi-root test must exercise that
  shipped path rather than reimplementing failure selection in the harness.

**Acceptance gate:** All nine tests pass under `cargo nextest run --all-features
--workspace --tests`; `cargo nextest run -p hana_lading --all-features` passes in
isolation; the `clippy` skill passes.

#### Retrospective

**What worked:**

- Nine focused contract cases now prove successful multi-set completion,
  settings-aware loading, typed and generic failure evidence, same-frame writes,
  mixed outcomes, recursive-dependency gating, developer-error diagnostics,
  declaration-order selection, and an error exit pattern against real Bevy 0.19
  asset loading.
- All 1,231 workspace tests, the ten-test isolated package suite, release example
  build, lint, rustdoc, formatting, and the standalone file-backed runtime smoke
  pass.

**What deviated from the plan:**

- The initial recursive-child gate repeatedly woke itself and trapped one
  `App::update`; the final gate uses a mutex-protected `GateState` and stored
  `Waker`, so the test controls release without a busy future.
- The integration suite was split into one explicit test binary with success,
  recursive, support, and focused failure modules to satisfy the repository's
  module-ownership rules.
- At the user's direction, the main agent completed the final post-fix review
  after stopping an unusually slow second blind-review process.

**Surprises:**

- Static inspection predicted Bevy's bounded task-pool tick would prevent the
  self-waking gate from hanging; the isolated nextest run disproved that claim
  by remaining inside the recursive test until interrupted after 63 seconds.

**Implications for remaining phases:**

- Phase 4 examples can rely on generic failure observers running before global
  resolution, successful sets remaining usable after another set fails, and
  `LoadProgress` remaining readable through global observers.
- Phase 4 uses a first-step-only Fairy Dust builder method for its crate-local
  asset root. Typestate keeps that method unavailable after any ordinary
  builder operation has installed Bevy's baseline plugins.

#### Phase 3 Review

- Phase 4 now names the reflected state, failure record, rendered-text evidence,
  and successful-scene marker that BRP must inspect before shutdown.
- Phase 4 now cites the recursive-dependency contract test for that guarantee;
  its PNG examples directly exhibit failure reporting, generic recording,
  global ordering, and application-owned policy instead of simulating a child
  dependency they do not load.
- The Fairy Dust asset-root decision is resolved in favor of a typestate-gated
  `with_asset_root(...)` builder method rather than a second constructor.

### Phase 4 — Protective examples and documentation · status: done (`5bbf8999`)

#### Work Order

**Goal:** Two runnable examples teach the normal loading flow and visibly prove
how applications can make safe, explicit decisions after partial or complete
failure. Documentation names which guarantees come from `hana_lading` and which
policy remains application-owned.

**Resolved decision: configure a crate-local asset root through the first builder step**

Actual problem:
`fairy_dust::sprinkle_example()` installs `DefaultPlugins` immediately, so
Phase 4 cannot replace `AssetPlugin::file_path` with
`CARGO_MANIFEST_DIR/assets` after receiving the builder.

What exists now:
- The examples must use the Fairy Dust chain, while their successful PNG is
  intentionally package-owned under `crates/hana_lading/assets/`.

Decision:
- Keep `fairy_dust::sprinkle_example()` as the single constructor. It returns a
  pre-installation typestate whose `with_asset_root(...)` method consumes that
  builder, configures `AssetPlugin::file_path`, installs the Fairy Dust baseline
  plugins, and returns the normal builder typestate.
- The first ordinary builder operation performs the same transition with
  Bevy's default asset root, preserving existing fluent call sites. Once that
  transition occurs, `with_asset_root(...)` is absent from the returned type,
  so Rust rejects attempts to configure the root later in the chain.
- Keep the direct `app_mut()` escape hatch on the installed typestate only. Add
  an explicit default-root finalization method if a caller needs direct app
  access before selecting any ordinary capability.

**Spec:**

- Add `fairy_dust` and `hana_diegetic` as example-only dev-dependencies and
  explicit `[[example]]` entries. Extend the existing workspace `bevy`
  dev-dependency with `bevy_state` and preserve the Phase 3 image/PNG features.
- Add a real successful PNG fixture under `crates/hana_lading/assets/`. Configure
  the examples with
  `.with_asset_root(concat!(env!("CARGO_MANIFEST_DIR"), "/assets"))` as the first
  builder step so both documented root-level run commands resolve that fixture
  independently of the invoking directory; keep the intentionally missing path
  absent.
- Amend `docs/fairy_dust/canonical-example.md` with a narrow takeover/error
  exception. Such examples may omit ground plane, studio lighting,
  camera-control panel, and title bar, but must still use `sprinkle_example()`,
  the first-step-only `.with_asset_root(...)` when assets are package-owned,
  `.with_brp_extras()`, `.with_save_window_position()`, and `.run()`.
- `examples/catastrophic_failure.rs`, display name **Catastrophic Failure**,
  intentionally loads an absent PNG. An example-owned recorder writes generic
  failure evidence directly into a durable resource. The global resolution
  observer stays in an example-owned `Loading` state and replaces the normal
  scene with one opaque screen panel identifying the failed set, tracked path,
  error, and decision to remain loading.
- `examples/degraded_failure.rs`, display name **Degraded Failure**, uses the full
  canonical Fairy Dust chain. It declares one successful required set and one
  intentionally failing optional set. On global resolution it enters `Ready`,
  keeps the successful scene content, and displays a panel identifying the
  failed optional capability and the decision to continue in degraded mode.
- Add `examples/loading_evidence/mod.rs`, shared by both example binaries. It
  defines `ExampleState` (`Loading` and `Ready`), a reflected `FailureRecord`
  resource containing set name, tracked-path string, and error string, a
  reflected `FailurePanelContent` component containing the exact rendered panel
  text, and a reflected `RequiredSceneContent` marker for usable successful
  content. Its plugin initializes the state and failure resource and registers
  the concrete generic `State<ExampleState>` for BRP reflection. Attach
  `FailurePanelContent` to the entity that carries the panel's rendered text so
  the BRP observation and visible message cannot diverge.
- Both examples use only `hana_lading`'s public API, assert their missing fixture
  is absent, and terminate their builder chain with `.run()`.
- The examples directly exhibit protections 1 and 3–6 below. Their source
  comments and documentation cite Phase 3's
  `recursive_dependencies_gate_and_fail` contract test as the executable proof
  for protection 2, because the example PNG has no recursive child:
  1. every handle returned by `DiskAssetLoader` is tracked;
  2. an asset set emits one terminal outcome only after recursive dependencies
     resolve;
  3. failures are reported rather than leaving startup waiting forever;
  4. generic failure observers can record every set without knowing its type;
  5. global completion occurs after those direct failure-record writes;
  6. the library never chooses whether the app continues, degrades, or exits.
- Complete `README.md` with the workflow and both run commands. Add crate-level
  docs describing definition → plugin registration → loading → per-set outcome
  → global application decision. Document failure semantics and the headless
  exit recipe tested in Phase 3. Expand the existing
  `docs/hana_lading/overview.md` into the concise as-planned overview without
  duplicating this work order.

The documented run commands are:

```sh
cargo run -p hana_lading --example degraded_failure
cargo run -p hana_lading --example catastrophic_failure
```

**Files:**

- `crates/hana_lading/Cargo.toml`
- `crates/hana_lading/examples/catastrophic_failure.rs`
- `crates/hana_lading/examples/degraded_failure.rs`
- `crates/hana_lading/examples/loading_evidence/mod.rs`
- `crates/hana_lading/assets/<successful-fixture>.png`
- `crates/fairy_dust/src/lib.rs`
- `crates/fairy_dust/src/builder/sprinkle.rs`
- `crates/fairy_dust/README.md`
- `crates/fairy_dust/tests/trybuild.rs`
- `crates/fairy_dust/tests/trybuild/`
- `docs/fairy_dust/canonical-example.md`
- `crates/hana_lading/README.md`
- `crates/hana_lading/src/lib.rs`
- `docs/hana_lading/overview.md`

**Constraints from prior phases:**

- Phase 1 sealed event construction behind private fields and created
  `docs/hana_lading/overview.md`; examples must use only public accessors and
  expand that file instead of adding a second overview.
- Phase 2 supplies the complete loading and terminal-event runtime. Phase 3
  proves failure evidence, recursive-dependency completion, observer ordering,
  and the exit-on-failure recipe that these examples document.
- The package already has a workspace-inherited `bevy` dev-dependency with the
  reflection backend and image/PNG features; extend it with state support.

**Acceptance gate:** Workspace build including examples passes. Launch each
example, use BRP to read `State<ExampleState>` and `FailureRecord`, query the
entity carrying `FailurePanelContent`, and query `RequiredSceneContent` in the
degraded example; then terminate through `brp_extras/shutdown`. Catastrophic
inspection must show `Loading`, one recorded failure, the remain-loading panel
message, and no required-scene marker. Degraded inspection must show `Ready`,
one optional failure, the continue-degraded panel message, and retained required
scene content. The example source and documentation identify all six
protections above; the canonical-example exception is present; `cargo build -p
hana_lading --all-features --examples` passes in isolation; Fairy Dust has
opt-in nextest-executed trybuild pass and compile-fail cases proving that
`with_asset_root(...)` is available first and unavailable after another builder
operation; `cargo nextest run -p fairy_dust --test trybuild --run-ignored
ignored-only` passes without imposing that separate Bevy compile on routine
workspace test runs; the `clippy` skill passes.

#### Retrospective

**What worked:**

- Fairy Dust keeps one `sprinkle_example()` entry point while a second typestate
  dimension exposes `with_asset_root(...)` only before baseline installation.
  Existing fluent chains still install the default baseline on their first
  ordinary operation, and installed builders retain their previous one-parameter
  type spelling and `const fn` primitive/home entry points.
- The two examples use the same loading evidence to demonstrate opposite
  application policies. The catastrophic path stays in `Loading` behind an
  opaque failure panel; the degraded path enters `Ready`, applies the loaded PNG
  to its retained cube, and reports the optional failure through a translucent
  panel.
- Reflected state, failure records, panel content, and required-scene identity
  made both visual outcomes objectively inspectable through BRP before graceful
  shutdown.

**What changed during implementation:**

- The deferred constructor decision was resolved in favor of the user's
  first-step builder method with compile-time ordering. `app_mut()` remains on
  the installed typestate, with `with_default_asset_root()` as the explicit
  transition for callers that need direct app access before another capability.
- The API review preserved the installed builder's existing `const fn`
  capabilities, added positive and negative compile cases for both asset-root
  and direct-app transitions, and corrected documentation that still described
  baseline installation as constructor work.
- The slow trybuild case is ignored during routine suites and run explicitly for
  typestate API changes. Its first cold build populated a separate Bevy target;
  subsequent verification completed from that cache.
- The first degraded-example launch reached the expected loading events but hit
  a local WGPU queue stall. A fresh launch passed every BRP observation and shut
  down normally.

**Verification:**

- `cargo build --release --workspace --all-features --examples` passes; the only
  diagnostic is the pre-existing unused `DetectChanges` import in
  `hana_valence`.
- `cargo nextest run --all-features --workspace --tests` passes with 1,235 tests
  and five intentional skips. The explicit ignored-only Fairy Dust trybuild
  case also passes.
- The `clippy` workflow, rustdoc, Mend, style review, banned-word scan, nightly
  formatting check, and `git diff --check` pass.
- Live BRP inspection proves the catastrophic example remains `Loading`, records
  its required failure and exact panel text, and has no required-scene marker.
  The degraded example reaches `Ready`, records its optional failure and exact
  panel text, and retains the required textured cube. Both terminate through
  `brp_extras/shutdown`.

**Implications for remaining phases:**

- No implementation phases remain. The documented handoff can pin the final
  checkpoint SHA when Hana begins consuming `hana_lading`.

#### Phase 4 Review

- The independent behavior review approved the examples, reflected evidence,
  asset-root independence, visible policies, and documentation without fixes.
- The independent API review's compatibility and compile-coverage findings were
  repaired and rechecked. Its final custom-root pass assertion was restored
  alongside the default-root `app_mut()` assertion, and the ignored-only
  trybuild test passes with both late-call rejection cases.
- With no later phase to revise, the plan review records only the final Hana
  handoff boundary below.

## Handoff to Hana

After all four phases are complete, commit and push the worktree branch and
record the resulting Git SHA. The later Hana plan begins by pinning
`hana_lading` and the other bevy_hana dependencies to that same SHA. No Hana
application source belongs in this plan or its implementation worktree.
