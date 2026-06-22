# Ingest `bevy_window_manager` into `bevy_hana` as `bevy_clerestory`

A one-off migration plan: move the standalone `bevy_window_manager` crate into
`crates/bevy_clerestory/` **and rename it** to `bevy_clerestory` in the same
operation. This is the generic [`ingest.md`](ingest.md) recipe plus a rename
layer plus a crates.io retirement that differs from a pure relocation. Read
`ingest.md` for the rationale behind each base step; this doc only restates a
base step where the rename changes it, and adds the migration-specific work.

- Incoming crate: `bevy_window_manager` → `bevy_clerestory`
- Source repo: `~/rust/bevy_window_manager` (GitHub `natepiano/bevy_window_manager`)
- Host workspace: `~/rust/bevy_hana` (GitHub `natepiano/hana`)
- Final crate dir: `crates/bevy_clerestory/`

## Decisions locked (do not relitigate)

- **Name:** `bevy_clerestory` (architecture: a clerestory is a row of windows set
  high in a wall — windows arranged across a surface). Bare `clerestory` and
  `bevy_clerestory` both free on crates.io.
- **Plugin type names unchanged:** keep `WindowManagerPlugin` and
  `WindowManagerPluginCustomPath`. Descriptive plugin names that don't match the
  crate name are the workspace majority (`PanelPlugin`, `TextPlugin`,
  `DiagnosticsPlugin`, …), so `WindowManagerPlugin` under `bevy_clerestory` is
  precedented. Only the crate *identity* renames, not the API symbols.
- **CHANGELOG:** reset to a single "Initial release" entry (see step R3). Old
  per-version history is preserved in git, not in the file.
- **crates.io retirement:** publish one final maintenance-note release of
  `bevy_window_manager`, **left un-yanked** (stays resolvable; reads as
  maintenance-frozen, not pulled), repo archived. Transfer later via
  `cargo owner` if anyone wants it. No yank.
- **Version:** `bevy_clerestory` resets to `0.1.0` — it is a new crate on
  crates.io, not a continuation of `bevy_window_manager`'s `0.22.x` line.

## Rename surface (survey, for verification)

Crate-identity occurrences to change (~84 `bevy_window_manager` / `window_manager`
hits). The `WindowManager*` *symbols* (24 + 6 refs) are **left alone**.

| Where | What changes |
|---|---|
| `Cargo.toml` | `name`, `[lib].name`, `repository`/`homepage` URLs |
| `src/lib.rs` + doc comments | crate-level docs naming the crate (not the plugin type) |
| `use bevy_window_manager::…` | every `use` path in `src`, `examples`, `tests` |
| `tests/scripts/run_test.py` (lines ~64–69) | fully-qualified component strings `bevy_window_manager::managed::ManagedWindow`, etc. — **string literals, compiler won't catch a miss** |
| `tests/scripts/*.{sh,ps1}` | Zed window-title match string `"bevy_window_manager"` |
| `tests/config/*.json` | `example_ron_path` dirs (`…/bevy_window_manager/restore_window.ron`) |
| `README.md` | full rebrand (title, badges, crates.io/docs.rs links, usage) |
| `.claude/` (plans, commands) | dev tooling pulled in by subtree; rename or drop |

After the rename, `rg -i 'window[_-]?manager' --glob '!target'` in the crate
should return **only** the intentional `WindowManager*` plugin-symbol lines —
nothing in snake_case, no `use` paths, no string literals.

## Behavioral consequence (not cosmetic)

`src/persistence/load.rs:35` derives the default state directory from
`env!("CARGO_PKG_NAME")`:

```rust
.join(env!("CARGO_PKG_NAME"))   // was …/bevy_window_manager/, becomes …/bevy_clerestory/
```

Renaming the crate silently moves the default persistence dir. Any user on the
default config loses saved window positions once (resets to first-launch
placement). Apps that override via `with_app_name(...)` are unaffected. Accepted
given `0.22.0-dev` + low downloads — but it is a real one-time reset, so it is a
conscious choice, not an accident.

---

## Steps

Order: **rename in the source repo first** (one clean commit), then run the base
ingest. This keeps the archived repo's final state coherent and `git log
--follow` clean through the move.

### R1. Rename in the standalone source repo

`cd ~/rust/bevy_window_manager`, confirm clean tree on `main` in sync with
origin, then apply the full rename surface above:

- All `bevy_window_manager` / `window_manager` crate-identity occurrences →
  `bevy_clerestory` / `clerestory`. **Leave `WindowManagerPlugin` /
  `WindowManagerPluginCustomPath` untouched.**
- `Cargo.toml`: `name = "bevy_clerestory"`, `[lib].name = "bevy_clerestory"`.
  (URLs get rewritten in step 4; setting them now is fine too.)
- Verify the rename is complete with the `rg` check above.
- `cargo build --workspace` + `cargo nextest run` green **in the source repo**
  before committing (the source repo still builds standalone here).
- Commit (`rename: bevy_window_manager → bevy_clerestory`) and push. This is the
  last functional commit to the standalone repo before its final
  maintenance-note release (step 10).

### R2. (folded into 10) crates.io retirement is handled at archive time

No action here — listed so the order is explicit. See step 10.

### R3. Reset the CHANGELOG

Replace `CHANGELOG.md` with a single initial-release entry under the new name.
Do not carry forward the `bevy_window_manager` per-version history (it stays in
git). List the shipped feature set:

```markdown
# Changelog

## 0.1.0 — Initial release

`bevy_clerestory` (formerly published as `bevy_window_manager`).

- Primary-window position/size persistence across launches.
- Multi-monitor support with scale-factor-correct positioning (handles mixed
  Retina / non-Retina monitor setups).
- Correct window placement when dragging across monitors with different scale
  factors.
- Platform workarounds: macOS, Windows, Linux X11 and Wayland.
- `Monitors` resource, `MonitorInfo`, `CurrentMonitor`, `ManagedWindow`,
  `WindowKey`, `WindowRestored` / `WindowRestoreMismatch` events.
- `WindowManagerPlugin` with `with_app_name` / `with_path` / `with_persistence`
  builders.
```

### 1. Pre-flight (base `ingest.md` §1)

Source repo clean on `main`, in sync with origin — already true after R1's push.

### 2. Ingest with history preserved (base §2)

From `~/rust/bevy_hana`:

```bash
git subtree add --prefix=crates/bevy_clerestory ~/rust/bevy_window_manager main
```

No `--squash`. `git log --follow crates/bevy_clerestory/src/lib.rs` then shows
full original history including the R1 rename commit. The source repo dir name
(`bevy_window_manager`) is irrelevant — the prefix sets the new location.

### 3. Post-subtree cleanup (base §3)

Delete from `crates/bevy_clerestory/`: `Cargo.lock`, `taplo.toml`,
`rustfmt.toml`, `.github/`.

**Migration note — LICENSE files (create them):** the source repo has **no**
`LICENSE-APACHE` / `LICENSE-MIT` / `NOTICE` files (the manifest declares
`MIT OR Apache-2.0` but the files are absent), so `subtree` brings none. Base §3
says "keep" them — there is nothing to keep, so **create** them to match every
other published member. There is no root-level license to inherit; each member
(`bevy_kana`, `bevy_lagrange`, `bevy_liminal`, `bevy_valence`, `bevy_diegetic`)
ships its own per-crate `LICENSE-APACHE` + `LICENSE-MIT` as real files. Copy from
a sibling with the same license:

```bash
cp crates/bevy_kana/LICENSE-APACHE crates/bevy_kana/LICENSE-MIT crates/bevy_clerestory/
```

(`NOTICE` is optional — only `bevy_diegetic` has one; the source had none, skip
unless wanted.) The `license` field still resolves via `workspace.package`
(step 4); the files are what crates.io requires at publish.

Decide on `crates/bevy_clerestory/.claude/` — either rename its
name-bearing contents or delete it (dev tooling, not shipped).

### 4. Restructure `Cargo.toml` to the workspace pattern (base §4)

Match `bevy_diegetic`'s thin-manifest pattern:

- `edition.workspace = true`, `license.workspace = true`,
  `authors.workspace = true`, `repository.workspace = true`,
  `[lints] workspace = true`.
- `readme = "README.md"` per-crate.
- `name = "bevy_clerestory"`, `version = "0.1.0"`, plus per-crate `description`,
  `keywords`, `categories`.
- **`homepage`** → `https://github.com/natepiano/hana/tree/main/crates/bevy_clerestory`
  (the GitHub repo is `natepiano/hana`, not `bevy_hana`).

### 4b. Switch in-tree deps to workspace/path (migration-specific)

The source manifest pins **registry** versions of crates that already live in,
or are declared by, this workspace. These must be repointed or they resolve to
published copies instead of the in-tree ones:

- `bevy_kana = "0.1.0"` → `bevy_kana = { workspace = true }`
  (workspace declares `bevy_kana = { path = "crates/bevy_kana" }` — this is the
  one that silently pulls the published 0.1.0 if left as-is).
- `bevy = { version = "0.19.0", default-features = false, features = [...] }` →
  `bevy = { workspace = true, features = ["bevy_log", "bevy_window",
  "bevy_winit", "wayland", "x11"] }` (keep the exact feature list).
- dev-dep `bevy_brp_extras = "0.20.0"` → `{ workspace = true }`
  (workspace has `0.20.1`).
- `bevy_diagnostic = "0.19.0"` — not in `[workspace.dependencies]`; either add a
  workspace entry and reference it, or leave the pin and align the version to
  `bevy`'s. Prefer adding to `[workspace.dependencies]` for consistency.
- Leave **platform-specific** target deps per-crate — `windows`, `objc2`,
  `objc2-app-kit`, `x11rb`, `raw-window-handle`, `dirs`, `ron`, `tempfile` are
  unique to this crate, not shared workspace deps.

### 5. Minimize the Bevy feature set (base §5)

Source already uses a tight set (`bevy_log`, `bevy_window`, `bevy_winit`,
`wayland`, `x11`; code touches only `bevy::{window, winit, prelude, ecs}` plus
`bevy_diagnostic`). Keep it; compare against `bevy_diegetic` / `bevy_lagrange`
and trim only if something is unused.

### 6. Workspace membership (base §6 — adapted)

`[workspace] members = ["crates/*"]` auto-includes `crates/bevy_clerestory` —
**no `members` edit needed.** There is currently **no** `bevy_window_manager`
entry in `[workspace.dependencies]` and **no** in-tree consumer, so base §6's
"switch registry entry to path dep" is N/A. Add a
`bevy_clerestory = { path = "crates/bevy_clerestory" }` entry **only if** another
member starts depending on it later.

### 7. Verify (base §7)

- `cargo build --workspace` green.
- `cargo build --workspace --examples` green (includes `restore_window`,
  `custom_path`, `custom_app_name`).
- `cargo nextest run --workspace` green.
- Run the `restore_window` example end-to-end; confirm it writes to
  `…/bevy_clerestory/` (the renamed default dir) and restores correctly.
- Re-run the rename `rg` check inside `crates/bevy_clerestory` — only
  `WindowManager*` symbols should remain.
- If the BRP test runner (`tests/scripts/run_test.py`) is exercised, confirm the
  renamed fully-qualified component strings resolve against the running app.

Regenerate `Cargo.lock` and commit.

### 8. Update nightly clean-fix config (base §8)

`~/.claude/scripts/clean-fix/clean-fix.conf`:

- `[build]` — ensure `bevy_hana` (workspace root) is present.
- `[targets]` — add `bevy_hana/crates/bevy_clerestory`.

Do **not** remove the standalone `[build] bevy_window_manager` entry yet (still a
live checkout until step 11).

### 9. Checkpoint — confirm before retiring the standalone repo (base §9)

Stop. Everything through step 8 is in-tree and reversible. Summarize what landed
(crate at `crates/bevy_clerestory/`, history preserved incl. R1 rename, manifest
on workspace pattern, in-tree deps repointed, build/examples/tests/smoke
results, clean-fix enrollment) and get explicit go-ahead before steps 10–11
(outward-facing + destructive).

### 10. Retire the standalone repo + crates.io (base §10, modified)

**crates.io (migration-specific — replaces base §10's pure-pointer-only step):**

1. On the source repo (still pushable, pre-archive), prepare a **final
   `bevy_window_manager` release**: bump version (e.g. `0.22.0`), keep the code
   functional, and set its `README.md` to a maintenance note — not a hard "do
   not use", since it stays resolvable:

   ```markdown
   # bevy_window_manager

   This crate is no longer actively developed. Development continues as
   **`bevy_clerestory`** in the hana monorepo:

   ➡️ **https://github.com/natepiano/hana/tree/main/crates/bevy_clerestory**

   This version remains available and functional. Open to transfer — contact via
   the repository above if you'd like to take it over.
   ```

   `cargo publish` this final release. **Do not `cargo yank`.** (Yank wouldn't
   hide the page anyway and isn't needed for a future `cargo owner` transfer;
   leaving it resolvable is the friendlier handoff.)

**GitHub archive (base §10):**

2. Replace the source repo's `README.md` with the same pointer note, commit,
   push.
3. `gh repo archive natepiano/bevy_window_manager --yes` (run unsandboxed per the
   `gh` rule). Confirm with
   `gh repo view natepiano/bevy_window_manager --json isArchived`.
4. Stop pushing to `~/rust/bevy_window_manager`.

### 11. Remove the local standalone checkout (base §11)

Only after: workspace builds/tests green, `git log --follow` resolves full
history in-tree, final crates.io release published, GitHub repo archived.

```bash
rm -rf ~/rust/bevy_window_manager
```

Then prune dead standalone entries from `clean-fix.conf`: remove
`[build] bevy_window_manager` and any `bevy_window_manager_bevy_update` rows.
Nightly coverage now comes from the `bevy_hana` root `[build]` entry +
`bevy_hana/crates/bevy_clerestory` `[targets]` entry.

## Definition of done

- `crates/bevy_clerestory/` exists with full git history (`git log --follow`
  works; `git blame` resolves through the move and the R1 rename).
- No crate-identity `window_manager` strings remain — only `WindowManager*`
  plugin symbols (intentional).
- CHANGELOG reset to `0.1.0` initial release; `Cargo.toml` on workspace pattern;
  `bevy_kana` / `bevy` / `bevy_brp_extras` repointed to workspace.
- Inner `Cargo.lock`/`taplo.toml`/`rustfmt.toml`/`.github/` removed;
  `LICENSE-APACHE` + `LICENSE-MIT` created (copied from a sibling member).
- Workspace builds + examples + tests green; `restore_window` writes to the
  renamed `…/bevy_clerestory/` dir.
- `clean-fix.conf`: `bevy_hana` in `[build]`, `bevy_hana/crates/bevy_clerestory`
  in `[targets]`; standalone entries pruned.
- Final `bevy_window_manager` maintenance-note release published, **un-yanked**;
  GitHub repo archived with pointer README.
- Local standalone checkout removed.
