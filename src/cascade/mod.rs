//! Cascading-attribute resolution with per-entity `Resolved<A>` caches.
//!
//! Several attributes in this crate honor an override cascade before a final
//! value is used to render — panel-text alpha mode, panel-text font unit,
//! standalone-world-text alpha mode, standalone-world-text font unit.
//! Historically each of these was resolved with an ad-hoc
//! `Option<T>.or(...).unwrap_or(...)` computed inline at read time. Nothing
//! cached the resolved value, so Bevy's `Changed<T>` could not observe a tier
//! change and propagate. Runtime mutation of a higher tier silently left
//! downstream state stale.
//!
//! This module supplies one reusable pattern: store the resolved value per
//! entity in a [`Resolved<A>`] component, maintain that cache with a small set
//! of generic observers and systems, and let Bevy's native `Changed<T>` fan
//! the invalidation out to readers. Readers always query `&Resolved<A>`
//! directly; they never resolve inline.
//!
//! # Three topologies
//!
//! An attribute picks exactly one topology depending on which tiers it has:
//!
//! | topology | tiers | plugin |
//! |---|---|---|
//! | child-of-panel | entity override → panel override → global default | [`CascadePanelChildPlugin`] |
//! | panel-targeted | panel override → global default | [`CascadePanelPlugin`] |
//! | entity-targeted | entity override → global default | [`CascadeEntityPlugin`] |
//!
//! 3-tier attributes implement [`CascadePanelChild`]. 2-tier attributes
//! implement [`CascadeTarget`] and pick the plugin that matches their
//! semantics. The two 2-tier plugins share machinery but name the topology at
//! the registration site so each attribute's intent is obvious.
//!
//! For 3-tier cascades, **both** the panel and its text children carry
//! `Resolved<A>`. The panel's `Resolved<A>` represents the "default my
//! children inherit" — computed as `panel.override.unwrap_or(global_default)`.
//! Each child's `Resolved<A>` is `child.override.unwrap_or(panel.Resolved<A>)`.
//! Native `Changed<Resolved<A>>` fires naturally at both levels.
//!
//! # Write paths
//!
//! For every registered cascade, the following fire in response to changes:
//!
//! - Spawn-time [`On<Add>`] observer(s) — populate the initial `Resolved<A>` synchronously during
//!   command flush, so readers never see an entity without `Resolved<A>`.
//! - `reconcile_panel_resolved` (3-tier only) — keeps each panel's own `Resolved<A>` in sync with
//!   `A::PanelOverride` mutations and global default mutations. Runs every frame, does real work
//!   only when a source changed.
//! - `propagate_panel_to_children` (3-tier only) — on `Changed<Resolved<A>>` on panels, writes each
//!   non-tier-1 child's `Resolved<A>` to match.
//! - `propagate_global_default_to_entity` (2-tier) — on a change to `CascadeDefaults`, recomputes
//!   `Resolved<A>` for every target entity without a tier-1 override.
//! - Tier-1 re-resolve — folded into the existing reader systems (see the invariants below).
//!
//! All non-observer propagation systems live in [`CascadeSet::Propagate`].
//! Users who need their systems to observe freshly-propagated values schedule
//! against that set.
//!
//! # Why change detection uses systems, not observers, past spawn
//!
//! Bevy 0.18 lifecycle observers (`On<Add>`, `On<Insert>`, `On<Remove>`, ...)
//! fire on component-level events. They do not fire on field-level mutations
//! of an existing component, and they do not fire on resource mutations.
//! Propagation therefore polls: `Changed<A::PanelOverride>` for panel-field
//! mutation, a `Local<Option<A>>` sentinel inside `should_propagate_defaults`
//! for [`CascadeDefaults`] mutations, and native `Changed<Resolved<A>>` for
//! the panel's resolved value transitioning. Spawn gets observers because a
//! polled first frame would be a frame too late.
//!
//! # Design note: merged 2-tier trait
//!
//! An earlier revision split 2-tier cascades into separate `CascadePanel` and
//! `CascadeEntity` traits with blanket
//! `impl<A: CascadePanel> CascadeTarget for A` / `impl<A: CascadeEntity> ...`.
//! That shape fails Rust's coherence check (E0119) — the compiler cannot
//! prove disjointness of user-facing supertrait bounds. The current module
//! uses a single [`CascadeTarget`] trait; topology intent is expressed at the
//! registration site via [`CascadePanelPlugin`] vs. [`CascadeEntityPlugin`].
//!
//! # Invariants
//!
//! Two rules contributors must honor:
//!
//! 1. A mutator for a panel component must not incidentally touch the tier-2 override field for any
//!    [`CascadePanelChild`] attribute. The `Changed<A::PanelOverride>` filter will flag the panel
//!    as dirty and cause a redundant recomputation, but more importantly, a setter that
//!    side-effects `text_alpha_mode` silently changes every child that has no tier-1 override.
//! 2. A reader's tier-1 re-resolve always calls the full `resolve_panel_child` / `resolve_target`
//!    helper, never shortcuts to the entity override alone. A `Some → None` tier-1 transition must
//!    fall through to the panel's `Resolved<A>` (3-tier) or [`CascadeDefaults`] (2-tier); otherwise
//!    the reader retains the stale tier-1-winning value.

mod defaults;
mod panel_child;
mod resolved;
mod set;
mod target;

pub use defaults::CascadeDefaults;
pub(crate) use panel_child::CascadePanelChildPlugin;
pub(crate) use resolved::CascadePanelChild;
pub(crate) use resolved::Resolved;
pub use set::CascadeSet;

// The 2-tier plugins (`CascadePanelPlugin`, `CascadeEntityPlugin`) and
// `CascadeTarget` are re-exported at `pub(crate)` when phases 3+ add
// consumers — keeping the facade honest about what's in use.
