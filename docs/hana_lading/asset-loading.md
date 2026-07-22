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

### Phase 3 — Contract test suite · status: todo

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

### Phase 4 — Protective examples and documentation · status: todo

#### Work Order

**Goal:** Two runnable examples teach the normal loading flow and visibly prove
how applications can make safe, explicit decisions after partial or complete
failure. Documentation names which guarantees come from `hana_lading` and which
policy remains application-owned.

**Pending decision: configure a crate-local asset root before Fairy Dust installs Bevy plugins**

Actual problem:
`fairy_dust::sprinkle_example()` installs `DefaultPlugins` immediately, so
Phase 4 cannot replace `AssetPlugin::file_path` with
`CARGO_MANIFEST_DIR/assets` after receiving the builder.

What exists now:
- The examples must use the Fairy Dust chain, while their successful PNG is
  intentionally package-owned under `crates/hana_lading/assets/`.

What should change:
- Add a narrow Fairy Dust constructor that accepts an asset root before
  `DefaultPlugins` are installed, and allow that constructor in the documented
  takeover/error exception.

Recommendation:
Add `fairy_dust::sprinkle_example_with_asset_root(...)`, implement it beside
`sprinkle_example()` in `crates/fairy_dust/src/lib.rs`, and update the Phase 4
Spec, Files, canonical-example exception, and examples to use it.

**Spec:**

- Add `fairy_dust` and `hana_diegetic` as example-only dev-dependencies and
  explicit `[[example]]` entries. Extend the existing workspace `bevy`
  dev-dependency with `bevy_state` and preserve the Phase 3 image/PNG features.
- Add a real successful PNG fixture under `crates/hana_lading/assets/`. Configure
  the examples' `AssetPlugin` root from `CARGO_MANIFEST_DIR/assets` so both
  documented root-level run commands resolve that fixture independently of the
  invoking directory; keep the intentionally missing path absent.
- Amend `docs/fairy_dust/canonical-example.md` with a narrow takeover/error
  exception. Such examples may omit ground plane, studio lighting,
  camera-control panel, and title bar, but must still use `sprinkle_example()`,
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
- Both examples use only `hana_lading`'s public API, assert their missing fixture
  is absent, and terminate their builder chain with `.run()`.
- The examples and their source comments explicitly demonstrate these
  protections:
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
- `crates/hana_lading/assets/<successful-fixture>.png`
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
example, use BRP to inspect its application state, durable failure record, and
rendered panel content, then terminate it through `brp_extras/shutdown`; each
inspection must match the documented catastrophic or degraded outcome. The
example source and documentation identify all six protections above; the
canonical-example exception is present; `cargo build -p hana_lading
--all-features --examples` passes in isolation; the `clippy` skill passes.

## Handoff to Hana

After all four phases are complete, commit and push the worktree branch and
record the resulting Git SHA. The later Hana plan begins by pinning
`hana_lading` and the other bevy_hana dependencies to that same SHA. No Hana
application source belongs in this plan or its implementation worktree.
