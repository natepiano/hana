//! One parent-walking cascade with per-entity `Resolved<A>` caches.
//!
//! Some text attributes inherit through the entity tree: text alpha mode and
//! font unit today. The rule is one sentence, applied by following `ChildOf`:
//! *my own override, else my parent's, else the global default at the root.*
//! A standalone text is depth-1 off the root, a panel is depth-1, and a panel
//! label is depth-2; deeper nesting needs no new type.
//!
//! # Using the cascade
//!
//! Entity-local authoring goes through typed
//! [`EntityCommands`](bevy::ecs::system::EntityCommands) extension methods from
//! [`CascadeEntityCommandsExt`]:
//!
//! ```ignore
//! commands
//!     .spawn(TextContent::new("hi"))
//!     .override_text_alpha(AlphaMode::Add)
//!     .override_font_unit(Unit::Millimeters);
//!
//! commands.entity(text).inherit_text_alpha();
//! let alpha = resolved_text_alpha(world, text);
//! ```
//!
//! `override_*` and `inherit_*` are command methods, not `TextProps` setters.
//! Use the same `override_*` verb at spawn and at runtime. A write scheduled
//! before [`CascadeSet::Propagate`] is visible to readers scheduled after that
//! set in the same `Update`; readers before it see the prior frame. If a write
//! runs after [`CascadeSet::Propagate`], descendants update on the next frame.
//! The directly overridden entity is self-healed when the command flushes so a
//! same-frame spawn override has the authored value available immediately.
//!
//! Global cascade defaults are per attribute:
//!
//! ```ignore
//! app.insert_resource(CascadeDefault(TextAlpha(AlphaMode::Add)));
//! ```
//!
//! # The mechanism is attribute-agnostic
//!
//! [`CascadeProperty`], the internal [`CascadeAttr`] reflection contract,
//! [`Override<A>`], [`Resolved<A>`], the parent-walk ([`resolve_walk`]), and
//! the propagation pass are all generic over the attribute. Any value that
//! should resolve *my override, else my parent's, else a global default* plugs
//! in as a `cascade_attr!` declaration plus one
//! [`CascadeDefault<A>`](CascadeDefault) resource, one plugin line, and typed
//! `override_*` / `inherit_*` / `resolved_*` wrappers.
//!
//! | Attribute | Global default | Public verbs |
//! | --- | --- | --- |
//! | [`TextAlpha`] | `CascadeDefault<TextAlpha>` | `override_text_alpha`, `inherit_text_alpha`, [`resolved_text_alpha`] |
//! | [`FontUnit`] | `CascadeDefault<FontUnit>` | `override_font_unit`, `inherit_font_unit`, [`resolved_font_unit`] |
//! | [`TextLighting`] | `CascadeDefault<TextLighting>` | `override_text_lighting`, `inherit_text_lighting`, [`resolved_text_lighting`] |
//! | [`TextSidedness`] | `CascadeDefault<TextSidedness>` | `override_text_sidedness`, `inherit_text_sidedness`, [`resolved_text_sidedness`] |
//!
//! # Membership is a property of the tree, not of a shared component
//!
//! A node declares an override by carrying [`Override<A>`] — one generic
//! component per attribute. An entity holds at most one of any component, so
//! "two sources for one attribute on one node" has no representation and no
//! exclusion marker is needed. Node *kind* (standalone / panel / label) is
//! carried by the `TextContent` / `DiegeticPanel` / `PanelTextChild` markers and
//! selects which render system draws the entity — orthogonal to the cascade.
//!
//! # Write paths
//!
//! - **Spawn.** The node-kind authoring bridges seed each participant's initial `Resolved<A>`
//!   during command flush. Standalone text and panels seed their own participating attributes;
//!   panel labels seed text alpha. Bridges that author a per-node override use the same helper as
//!   the public verbs; standalone text only seeds global defaults and relies on explicit
//!   `override_*` commands for non-default entity values.
//! - **Change.** [`CascadePlugin`]'s propagation system, in [`CascadeSet::Propagate`], re-resolves
//!   a node when its own `Override<A>` changes or is removed, its `ChildOf` changes, or
//!   `CascadeDefault<A>` changes — fanning ancestor changes down through `Children`. It runs every
//!   frame so a frame's `RemovedComponents<Override<A>>` is never cleared unread.
//!
//! Internal render systems query `&Resolved<A>` directly and filter on
//! `Changed<Resolved<A>>`; public callers should use the typed `resolved_*`
//! readers when they need the computed value.
//!
//! # Adding a cascade attribute
//!
//! For a new attribute with no existing plain-field history, add one
//! `cascade_attr!(Name(Ty), default = value)` declaration in `cascade/resolved.rs`,
//! three typed wrappers in `cascade/attributes.rs`
//! (`override_name`, `inherit_name`, `resolved_name`), one
//! `.add_plugins(CascadePlugin::<Name>::default())` line, and a render read site
//! that uses `Resolved<Name>` or `resolved_name`. The value type only needs to be
//! `Copy + PartialEq`.
//!
//! Promoting an existing plain field is a migration, not just a declaration.
//! Add the attribute, then inventory every consumer before editing: standalone
//! render reads, panel-label shaping/render reads, spawn-seed bridges, label
//! authoring capture before `for_shaping()`, examples, README/docs, and the
//! first-frame tests that prove `Resolved<A>` and rendered materials agree. Each
//! promoted attribute may participate in a different subset of consumers: text
//! alpha affects standalone text and panel labels, while font unit is seeded for
//! standalone text and panels but panel labels inherit it from the panel. Decide
//! whether the old plain accessors are deleted or kept as typed-verb sugar, and
//! migrate callers. Text alpha is the worked example: standalone entities use
//! `override_text_alpha`, while panel labels capture
//! `TextStyle::with_alpha_mode` and insert `Override<TextAlpha>`.

mod attributes;
mod cascade_set;
mod constants;
mod defaults;
mod plugin;
mod resolved;

pub use attributes::CascadeEntityCommandsExt;
pub use attributes::FontUnit;
pub use attributes::TextAlpha;
pub use attributes::TextLighting;
pub use attributes::TextSidedness;
pub(crate) use attributes::apply_cascade_override;
pub(crate) use attributes::remove_cascade_override;
pub use attributes::resolved_font_unit;
pub use attributes::resolved_text_alpha;
pub use attributes::resolved_text_lighting;
pub use attributes::resolved_text_sidedness;
pub use cascade_set::CascadeSet;
pub use defaults::CascadeDefault;
pub use defaults::CascadeDefaults;
pub(crate) use plugin::CascadePlugin;
pub use resolved::CascadeProperty;
pub(crate) use resolved::Override;
pub(crate) use resolved::Resolved;
pub(crate) use resolved::resolve_walk;
