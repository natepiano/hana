//! One parent-walking cascade with per-entity `Resolved<A>` caches.
//!
//! Several attributes in this crate honor an override cascade before a final
//! value is rendered — text alpha mode and font unit, on standalone world
//! text, panels, and panel labels. The rule is one sentence, applied by
//! following `ChildOf`: *my own override, else my parent's, else the global
//! default at the root.* A standalone text is depth-1 off the root, a panel is
//! depth-1, a panel label is depth-2; deeper nesting needs no new type.
//!
//! # The mechanism is attribute-agnostic
//!
//! [`CascadeAttr`], [`Override<A>`], [`Resolved<A>`], the parent-walk
//! ([`resolve_walk`]), and the propagation pass are all generic over the
//! attribute. Any value that should resolve *my override, else my parent's,
//! else a global default* plugs in as a new [`CascadeAttr`] impl plus a field
//! on [`CascadeDefaults`] — no new plugin, trait, or topology.
//!
//! | Attribute   | Global default                | Override source |
//! | ---         | ---                           | --- |
//! | [`TextAlpha`] | [`CascadeDefaults::text_alpha`] | `DiegeticPanel.text_alpha_mode` (panel labels inherit) |
//! | [`FontUnit`]  | [`CascadeDefaults::font_unit`]  | `WorldTextStyle.unit` (standalone); every panel carries a seeded `Override` (from `panel_font_unit`) its labels inherit |
//!
//! # Membership is a property of the tree, not of a shared component
//!
//! A node declares an override by carrying [`Override<A>`] — one generic
//! component per attribute. An entity holds at most one of any component, so
//! "two sources for one attribute on one node" has no representation and no
//! exclusion marker is needed. Node *kind* (standalone / panel / label) is
//! carried by the `WorldText` / `DiegeticPanel` / `PanelChild` markers and
//! selects which render system draws the entity — orthogonal to the cascade.
//!
//! # Write paths
//!
//! - **Spawn.** The node-kind authoring bridges seed each participant's initial `Resolved<A>`
//!   synchronously during command flush — the `WorldTextStyle` and `DiegeticPanel` bridges for
//!   standalones and panels, the panel-child alpha seed for labels. They are the only code that
//!   knows which entities participate and which `Resolved<A>` each one reads, so seeding lives with
//!   them; each calls [`resolve_walk`].
//! - **Change.** [`CascadePlugin`]'s propagation system, in [`CascadeSet::Propagate`], re-resolves
//!   a node when its own `Override<A>` changes or is removed, its `ChildOf` changes, or
//!   [`CascadeDefaults`] changes — fanning ancestor changes down through `Children`. It runs every
//!   frame so a frame's `RemovedComponents<Override<A>>` is never cleared unread, and is the single
//!   writer of each `Resolved<A>`.
//!
//! Readers query `&Resolved<A>` directly and filter on `Changed<Resolved<A>>`;
//! they never resolve inline. Users who need their systems to observe
//! freshly-propagated values schedule against [`CascadeSet::Propagate`].

mod cascade_set;
mod constants;
mod defaults;
mod plugin;
mod resolved;

pub use cascade_set::CascadeSet;
pub use defaults::CascadeDefaults;
pub(crate) use plugin::CascadePlugin;
pub(crate) use resolved::CascadeAttr;
pub(crate) use resolved::FontUnit;
pub(crate) use resolved::Override;
pub(crate) use resolved::Resolved;
pub(crate) use resolved::TextAlpha;
pub(crate) use resolved::resolve_walk;
