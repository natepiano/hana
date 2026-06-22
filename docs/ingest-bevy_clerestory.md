# Ingest `bevy_window_manager` into `bevy_hana` as `bevy_clerestory`

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Move the standalone
> `bevy_window_manager` crate into `crates/bevy_clerestory/`, rename it in-tree,
> repoint the host workspace, then retire the standalone crate/repo.

Base recipe: [`ingest.md`](ingest.md) (generic relocation rationale). This plan is
the rename-and-retire variant; the load-bearing facts are baked into the Work
Orders below.

## Delegation Context
<!-- Shared across all phases. /plan:delegate prepends this to every dispatch. -->

- **Project:** `bevy_clerestory` — Bevy plugin for primary-window position
  restoration + multi-monitor (scale-factor-correct) placement, moving into the
  `bevy_hana` workspace (Bevy 0.19.0 engine/dev-framework monorepo).
- **Stack:** Rust edition 2024; Bevy 0.19.0; serde, ron, dirs; platform deps
  `windows` 0.62.2 / `objc2`+`objc2-app-kit` 0.6.4/0.3.2 / `x11rb` 0.13 /
  `raw-window-handle` 0.6.
- **Layout:**
  - Source `~/rust/bevy_window_manager/`: `Cargo.toml`, `src/` (lib.rs,
    persistence/load.rs, restore/winit_info.rs, …), `examples/`
    (restore_window, custom_path, custom_app_name), `tests/`
    (scripts/, config/), `README.md`, `CHANGELOG.md`, `.claude/`.
  - Host `~/rust/bevy_hana/`: root `Cargo.toml` (`[workspace.dependencies]`,
    `members = ["crates/*"]`), `crates/{bevy_clerestory(new), bevy_diegetic,
    bevy_liminal, fairy_dust, bevy_lagrange, bevy_kana}/Cargo.toml`.
- **Key files:**
  - source `~/rust/bevy_window_manager/Cargo.toml` — `name` (7), `[lib].name`
    (12), `version = "0.22.0-dev"` (9), `bevy` (16–22), `bevy_diagnostic = "0.19.0"`
    (23), `bevy_kana = "0.1.0"` (24), dev `bevy_brp_extras = "0.20.0"` (32),
    platform target-deps (35–55).
  - source `src/persistence/load.rs:35` — `env!("CARGO_PKG_NAME")` default state dir.
  - source `src/restore/winit_info.rs:161` — debug-log string `"… No saved
    bevy_window_manager state …"`.
  - source `tests/scripts/run_test.py:64–69` — fully-qualified component strings
    `bevy_window_manager::…` (COMP_MANAGED, COMP_CURRENT_MONITOR, COMP_LAUNCH_INFO,
    COMP_MONITORS, COMP_PERSISTENCE).
  - source `tests/scripts/macos_detect_zed_monitor.sh` (11,13),
    `macos_move_zed_to_monitor.sh` (5,81), `windows_detect_zed_monitor.ps1`
    (5,86,134), `windows_move_zed_to_monitor.ps1` (4,105,186) — window-title
    match strings `"bevy_window_manager"` (11 total, compiler-invisible).
  - source `tests/config/{macos,windows,linux}.json:3` — `example_ron_path` dir.
  - source `README.md`, `CHANGELOG.md` — rebrand / reset targets.
  - host `~/rust/bevy_hana/Cargo.toml` — `bevy_window_manager = "0.21.0"` (35);
    `bevy_kana = { path = "crates/bevy_kana" }` (18, lacks `version`).
  - host `crates/bevy_diegetic/Cargo.toml:47`, `crates/bevy_liminal/Cargo.toml:52`,
    `crates/fairy_dust/Cargo.toml:40`, `crates/bevy_lagrange/Cargo.toml:45` —
    `bevy_window_manager = { workspace = true }`.
  - host `crates/bevy_diegetic/Cargo.toml` — manifest pattern to mirror
    (`*.workspace = true`, dev `bevy = { version = …, features = ["default_font"] }`).
- **Build:** `cargo build --workspace`; `cargo build --workspace --examples`;
  format with `cargo +nightly fmt`.
- **Test:** `cargo nextest run --workspace` (never `cargo test`).
- **Lint:** workspace `[workspace.lints]`; per-crate `[lints] workspace = true`.
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_hana` (load before editing Rust).
- **Invariants:**
  1. **Rename in-tree only** — the standalone source repo is *never* renamed; it
     keeps `bevy_window_manager` through its final pointer release and archive.
  2. **Keep `WindowManagerPlugin` / `WindowManagerPluginCustomPath` symbol names**
     un-renamed; only crate identity (`bevy_window_manager` → `bevy_clerestory`,
     `window_manager` → `clerestory`) changes.
  3. The `env!("CARGO_PKG_NAME")` default-dir shift (`…/bevy_window_manager/` →
     `…/bevy_clerestory/`) is an accepted one-time per-user reset.
  4. The rename surface spans the crate **and** 5 host references (root dep entry
     + 4 consumer dev-deps).
  5. `bevy_kana` workspace dep must carry `path` **and** `version` to keep members
     publishable.
  6. Never commit or run a destructive/outward-facing step (publish, archive,
     `rm`) without explicit user go-ahead.

## Phases

### Phase 1 — Ingest & cleanup  · status: todo

#### Work Order

**Execution:** manual (orchestrator) — `git subtree` spans two repos; not a codex delegate.

**Goal:** `crates/bevy_clerestory/` exists with full history and cruft removed; crate is internally still named `bevy_window_manager`.

**Spec:**
- Pre-flight: `~/rust/bevy_window_manager` clean on `main`, in sync with origin.
- Re-verify the target name is still unclaimed before going public:
  `curl -s -o /dev/null -w "%{http_code}" https://index.crates.io/be/vy/bevy_clerestory`
  must return `404`. Abort if `200`.
- From `~/rust/bevy_hana`: `git subtree add --prefix=crates/bevy_clerestory ~/rust/bevy_window_manager main` (no `--squash`). Verify `git log --follow crates/bevy_clerestory/src/lib.rs` shows full history. **No `git mv`** — `--prefix` already places files at `crates/bevy_clerestory/`.
- Delete: `crates/bevy_clerestory/{Cargo.lock, taplo.toml, rustfmt.toml, .github/, .claude/}` (`.claude/` carries name-bearing dev tooling — delete, don't keep).
- Create LICENSE files (source has none): `cp crates/bevy_kana/LICENSE-APACHE crates/bevy_kana/LICENSE-MIT crates/bevy_clerestory/` (`NOTICE` optional — skip).

**Files:**
- `crates/bevy_clerestory/` — created by subtree; listed deletions; LICENSE files copied in.

**Constraints from prior phases:** none (Phase 1).

**Acceptance gate:** `crates/bevy_clerestory/` exists; `git log --follow crates/bevy_clerestory/src/lib.rs` resolves full history; inner `Cargo.lock`/`taplo.toml`/`rustfmt.toml`/`.github/`/`.claude/` absent; `LICENSE-APACHE` + `LICENSE-MIT` present. (Workspace does **not** build yet — the crate manifest still names `bevy_window_manager`, colliding with the root registry dep. Green lands at Phase 3.)

### Phase 2 — Rename crate identity in-tree  · status: todo

#### Work Order

**Execution:** the bulk find-replace is performed by the user via the editor's global rename (project convention); the `rg` gate verifies completeness.

**Goal:** every crate-identity `bevy_window_manager`/`window_manager` occurrence inside `crates/bevy_clerestory/` is `bevy_clerestory`/`clerestory`; `WindowManager*` symbols untouched; CHANGELOG reset; README rebranded.

**Spec:**
- Global replace within `crates/bevy_clerestory/`: `bevy_window_manager` → `bevy_clerestory`, `window_manager` → `clerestory`, across `Cargo.toml` (`name`, `[lib].name`), `src/` (crate docs, `use` paths), `examples/`, `tests/`. **Leave `WindowManagerPlugin` and `WindowManagerPluginCustomPath` unchanged (Invariant 2).**
- Hand-verify these compiler-invisible string literals:
  - `src/restore/winit_info.rs:161` — debug string `"… No saved bevy_window_manager state …"` → `bevy_clerestory`.
  - `tests/scripts/run_test.py:64–69` — `COMP_MANAGED`, `COMP_CURRENT_MONITOR`, `COMP_LAUNCH_INFO`, `COMP_MONITORS`, `COMP_PERSISTENCE` strings `bevy_window_manager::…` → `bevy_clerestory::…`.
  - window-title strings (11): `macos_detect_zed_monitor.sh` (11,13), `macos_move_zed_to_monitor.sh` (5,81), `windows_detect_zed_monitor.ps1` (5,86 `TargetTitle`,134), `windows_move_zed_to_monitor.ps1` (4,105 `TargetTitle`,186) → `bevy_clerestory`.
  - `tests/config/{macos,windows,linux}.json:3` — `example_ron_path` dir → `bevy_clerestory` (must match the renamed `CARGO_PKG_NAME` default dir).
- `README.md`: full rebrand — title, crates.io/docs.rs badge URLs, homepage URL, body links; keep `cargo run --example restore_window`.
- `CHANGELOG.md`: replace with a single initial release (includes `ManagedWindowPersistence` + `Platform`):

  ```markdown
  # Changelog

  ## 0.1.0 — Initial release

  `bevy_clerestory` (formerly published as `bevy_window_manager`).

  - Primary-window position/size persistence across launches.
  - Multi-monitor support with scale-factor-correct positioning (mixed
    Retina / non-Retina setups).
  - Correct placement when dragging across monitors with different scale factors.
  - Platform workarounds: macOS, Windows, Linux X11 and Wayland.
  - `Monitors` resource, `MonitorInfo`, `CurrentMonitor`, `ManagedWindow`,
    `ManagedWindowPersistence`, `WindowKey`, `Platform`,
    `WindowRestored` / `WindowRestoreMismatch` events.
  - `WindowManagerPlugin` with `with_app_name` / `with_path` / `with_persistence`
    builders.
  ```

**Files:**
- `crates/bevy_clerestory/Cargo.toml` — `name`, `[lib].name`.
- `crates/bevy_clerestory/src/**` — crate docs + `use` paths + `winit_info.rs:161`.
- `crates/bevy_clerestory/examples/**` — `use` paths, window titles.
- `crates/bevy_clerestory/tests/scripts/*`, `tests/config/*.json` — string literals.
- `crates/bevy_clerestory/README.md` — rebrand.
- `crates/bevy_clerestory/CHANGELOG.md` — reset (block above).

**Constraints from prior phases:** Phase 1 placed the crate at `crates/bevy_clerestory/`, still internally named `bevy_window_manager`, LICENSE files present.

**Acceptance gate:** `rg -i 'window[_-]?manager' crates/bevy_clerestory --glob '!target'` returns **only** `WindowManagerPlugin` / `WindowManagerPluginCustomPath` symbol lines — no snake_case, no `use` paths, no string literals. (Workspace still not green — host references + manifest deps pending Phase 3.)

### Phase 3 — Manifest plumbing: crate reshape + host repoint  · status: todo

#### Work Order

**Goal:** the workspace resolves and builds the renamed member; `cargo build --workspace` green.

**Spec:**
- `crates/bevy_clerestory/Cargo.toml` → mirror `bevy_diegetic`'s thin manifest:
  - `edition.workspace = true`, `license.workspace = true`, `authors.workspace = true`, `repository.workspace = true`, `[lints] workspace = true`.
  - `readme = "README.md"` (per-crate). **Do not** set a per-crate `repository` (inherited; setting it is silently overridden).
  - `name = "bevy_clerestory"`, `version = "0.1.0"`, per-crate `description`/`keywords`/`categories`.
  - `homepage = "https://github.com/natepiano/hana/tree/main/crates/bevy_clerestory"`.
  - `bevy = { workspace = true, features = ["bevy_log", "bevy_window", "bevy_winit", "wayland", "x11"] }`.
  - `bevy_kana = { workspace = true }`; dev `bevy_brp_extras = { workspace = true }`; `bevy_diagnostic = { workspace = true }`.
  - dev `bevy = { workspace = true, features = ["default_font"] }` (examples need `default_font`); keep `tempfile` per-crate.
  - Remove any self dev-dep path reference if present.
  - Leave platform target-deps per-crate (`windows`, `objc2`, `objc2-app-kit`, `x11rb`, `raw-window-handle`, `dirs`, `ron`).
- Host root `~/rust/bevy_hana/Cargo.toml`:
  - line 35: `bevy_window_manager = "0.21.0"` → `bevy_clerestory = { path = "crates/bevy_clerestory", version = "0.1.0" }`.
  - line 18: `bevy_kana = { path = "crates/bevy_kana" }` → `bevy_kana = { path = "crates/bevy_kana", version = "0.1.0" }` (publishability).
  - add `bevy_diagnostic = "0.19.0"` to `[workspace.dependencies]`.
- Host consumer dep lines → `bevy_clerestory = { workspace = true }`:
  `crates/bevy_diegetic/Cargo.toml:47`, `crates/bevy_liminal/Cargo.toml:52`, `crates/fairy_dust/Cargo.toml:40`, `crates/bevy_lagrange/Cargo.toml:45`.

**Files:**
- `crates/bevy_clerestory/Cargo.toml` — workspace-pattern reshape + dep repoint.
- `~/rust/bevy_hana/Cargo.toml` — root dep entry rename, `bevy_kana` version, `bevy_diagnostic` entry.
- `crates/{bevy_diegetic,bevy_liminal,fairy_dust,bevy_lagrange}/Cargo.toml` — consumer dep rename.

**Constraints from prior phases:** Phase 2 set the crate `name = "bevy_clerestory"` and renamed all internal identity; the crate compiles standalone. The host root + 4 consumers still reference `bevy_window_manager` until this phase.

**Acceptance gate:** `cargo build --workspace` green; `cargo build --workspace --examples` green.

### Phase 4 — Verify, smoke-test, lock + clean-fix enrollment  · status: done

#### Work Order

**Goal:** full green verification, isolated smoke test, and nightly coverage enrolled.

**Spec:**
- `cargo nextest run --workspace` green.
- Workspace-wide rename gate: `rg -l 'bevy_window_manager' crates ~/rust/bevy_hana/Cargo.toml` returns nothing.
- Isolated smoke test (do **not** touch the user's real config): `HOME=$(mktemp -d) cargo run --example restore_window`; confirm it writes the temp `…/bevy_clerestory/restore_window.ron`. The user's real `…/bevy_window_manager/` dir stays untouched and recoverable.
- `cargo tree -p bevy_clerestory --duplicates` — no unexpected version conflicts for `windows`/`objc2`/`x11rb`/`raw-window-handle`. Confirm `Cargo.lock` carries these platform entries. Regenerate `Cargo.lock`.
- If the BRP runner (`run_test.py`) is exercised, confirm the renamed component strings resolve against the running app (mandatory, not optional).
- `~/.claude/scripts/clean-fix/clean-fix.conf`: ensure `bevy_hana` in `[build]`; add `bevy_hana/crates/bevy_clerestory` to `[targets]`. **Do not** remove the standalone `bevy_window_manager` entries yet (live until Phase 7).

**Files:**
- `~/rust/bevy_hana/Cargo.lock` — regenerated.
- `~/.claude/scripts/clean-fix/clean-fix.conf` — `[build]`/`[targets]` entries (external).

**Constraints from prior phases:** Phase 3 made the workspace build green; the renamed member is an in-tree path dep carrying `version = "0.1.0"`.

**Acceptance gate:** build + examples + `nextest` green; workspace-wide `rg` gate clean; smoke test writes to the isolated `bevy_clerestory` dir; `cargo tree --duplicates` clean.

### Phase 5 — Checkpoint  · status: todo

#### Work Order

**Execution:** manual gate — stop for explicit user go-ahead; not a codex delegate.

**Goal:** confirm readiness before any outward-facing/destructive step.

**Spec:** Everything through Phase 4 is in-tree and reversible (`git reset`). Summarize what landed (crate at `crates/bevy_clerestory/` with history, manifest on workspace pattern, host repointed, build/examples/tests/smoke green, clean-fix enrolled). Proceed to Phase 6+ only on explicit user go-ahead. Do not commit without being asked.

**Constraints from prior phases:** Phases 1–4 are complete and green.

**Acceptance gate:** user explicitly approves retiring the standalone repo.

### Phase 6 — Retire source repo + crates.io  · status: done

#### Work Order

**Execution:** manual (orchestrator/user) — outward-facing `cargo publish` + `gh archive`; not a codex delegate. `gh` runs unsandboxed.

**Goal:** final `bevy_window_manager` pointer release published (un-yanked); GitHub repo archived.

**Spec:**
- In `~/rust/bevy_window_manager` (still named `bevy_window_manager` — never renamed): bump `version` to `0.22.0` (latest published `0.21.0`; `0.22.0` is free — confirm with `cargo publish --dry-run`). Keep code functional. Replace `README.md` with a maintenance note pointing to `https://github.com/natepiano/hana/tree/main/crates/bevy_clerestory` (no-longer-developed / open-to-transfer wording). `cargo publish`. **Do not `cargo yank`** (stays resolvable; transferable later via `cargo owner`).
- Commit + push the README pointer.
- `gh repo archive natepiano/bevy_window_manager --yes`; confirm `gh repo view natepiano/bevy_window_manager --json isArchived`.
- Stop pushing to `~/rust/bevy_window_manager`.
- (Independently, when ready, publish the new crate: `cargo publish -p bevy_clerestory` — members carry no `publish` field, so it is publishable by default.)

**Files:**
- `~/rust/bevy_window_manager/Cargo.toml` — version bump.
- `~/rust/bevy_window_manager/README.md` — maintenance/pointer note.

**Constraints from prior phases:** the source repo was never renamed (Phase 2 renamed only the in-tree copy); the workspace is green and no longer depends on the published `bevy_window_manager` (Phase 3 repointed it to the path dep).

**Acceptance gate:** `bevy_window_manager` final release on crates.io, **un-yanked**; `gh … isArchived` is `true`.

### Phase 7 — Remove local checkout + prune clean-fix.conf  · status: done

#### Work Order

**Execution:** manual (orchestrator/user) — destructive (`rm -rf`); not a codex delegate.

**Goal:** standalone checkout deleted; nightly config pruned.

**Spec:** Only after Phase 6 (final release published, repo archived, history confirmed in-tree). `rm -rf ~/rust/bevy_window_manager` (subtree did not transfer the source reflog/tags, so this checkout is their only copy until the repo is archived). Then prune `clean-fix.conf`: remove `[build] bevy_window_manager` and any `bevy_window_manager_bevy_update` rows in `[build]`/`[targets]`. Nightly coverage now comes from the `bevy_hana` root `[build]` entry + `bevy_hana/crates/bevy_clerestory` `[targets]` entry.

**Files:**
- `~/rust/bevy_window_manager/` — removed.
- `~/.claude/scripts/clean-fix/clean-fix.conf` — pruned (external).

**Constraints from prior phases:** Phase 6 archived the GitHub repo; until then the local checkout is the only copy of the source reflog/tags.

**Acceptance gate:** `~/rust/bevy_window_manager` gone; `clean-fix.conf` has no `bevy_window_manager` rows.
