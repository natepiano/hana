# bevy_diegetic Text System -- Effects Design

How per-glyph effects work, what the system provides out of the box, and what
becomes possible once the foundation is in place.

**Related documents:**
- **VISION.md** -- Philosophy: "effects from composition, not configuration."
- **API_DESIGN.md** -- Sections 4-5: per-glyph access API and text effects API.
- **IMPLEMENTATION.md** -- Phase 5: per-glyph effects system implementation.
- **CONSIDERATIONS.md** -- Section 10.9: open question on effect organizing principle.

---

## Core Principle

Text effects are not special. They are Bevy systems that read glyph positions and
write glyph overrides. The text system provides measurement and placement; effects
are just math. This is the Processing model: `textWidth()` + a for-loop, not a
framework.

---

## The Two-Layer Architecture

### Layer 1: Indexed Arrays (default path)

One `Text3d` entity holds all the text. Two components provide per-glyph access:

- **`ComputedGlyphLayout`** (read-only) -- populated by the text layout system.
  Contains a `Vec<GlyphInfo>` with each glyph's position, size, advance, line
  number, and normalized progress (0.0-1.0).

- **`GlyphTransforms`** (writable) -- parallel arrays of per-glyph overrides:
  offsets, scales, rotations, colors, opacities. Indexed by glyph position in the
  text. The rendering system applies these on top of the computed layout.

```
Entity: Text3d("CRITICAL HIT")
  ├── ComputedGlyphLayout  →  [{char:'C', pos:(0,0), ...}, {char:'R', ...}, ...]
  └── GlyphTransforms      →  {offsets: [Vec2; 12], scales: [f32; 12], ...}
```

An effect system queries `(&ComputedGlyphLayout, &mut GlyphTransforms)` and writes
per-glyph overrides by index:

```rust
fn wave_text(
    time: Res<Time>,
    mut query: Query<(&ComputedGlyphLayout, &mut GlyphTransforms)>,
) {
    let t = time.elapsed_secs();
    for (layout, mut transforms) in &mut query {
        for (i, glyph) in layout.glyphs.iter().enumerate() {
            transforms.offsets[i].y = (t * 4.0 + glyph.total_advance * 0.5).sin() * 3.0;
        }
    }
}
```

This handles the vast majority of effects: wave, shake, typewriter, rainbow, fade,
pop, gradient, dissolve, and any custom math a user can dream up.

### Layer 2: Per-Glyph Entities (for physics and advanced use cases)

When you need individual glyph entities -- physics colliders, attaching child meshes
to a letter, querying a specific glyph from unrelated systems -- the system spawns
lightweight child entities:

```
Entity: Text3d("HI")
  ├── GlyphEntity { index: 0, char: 'H' }  ← attach RigidBody, Collider, etc.
  └── GlyphEntity { index: 1, char: 'I' }
```

Each child has metadata (glyph index, character) but no mesh of its own. A sync
system maps entity transforms back into `GlyphTransforms` on the parent, so physics
results flow into the rendering pipeline automatically.

### When to use which

| Use case | Layer |
|---|---|
| Visual effects (wave, shake, color) | Indexed arrays |
| Typewriter reveal | Indexed arrays |
| Per-character color from data | Indexed arrays |
| Custom math effects | Indexed arrays |
| Physics colliders on individual letters | Per-glyph entities |
| Attaching a particle emitter to a specific letter | Per-glyph entities |
| Querying a single glyph from an unrelated system | Per-glyph entities |

---

## `GlyphInfo` -- What the Layout System Provides

Each glyph in `ComputedGlyphLayout` carries:

```rust
pub struct GlyphInfo {
    /// Character this glyph represents.
    pub character: char,
    /// Index of this glyph in the glyph array.
    pub glyph_index: usize,
    /// Index of this glyph in the original string (byte offset).
    pub byte_index: usize,
    /// Position relative to the text entity's anchor.
    pub position: Vec2,
    /// Size of the glyph's bounding box.
    pub size: Vec2,
    /// Advance width (distance to next glyph origin).
    pub advance: f32,
    /// Line number (0-based).
    pub line: u32,
    /// Cumulative advance from the start of the text.
    pub total_advance: f32,
    /// Normalized progress (0.0 = first glyph, 1.0 = last glyph).
    pub progress: f32,
}
```

`total_advance` and `progress` are particularly useful for effects: they give you
spatial position along the text without needing to measure anything.

---

## `GlyphTransforms` -- What Effects Write

```rust
#[derive(Component, Default)]
pub struct GlyphTransforms {
    /// Position offset per glyph.
    pub offsets: Vec<Vec2>,
    /// Scale per glyph (1.0 = normal).
    pub scales: Vec<f32>,
    /// Rotation per glyph in radians.
    pub rotations: Vec<f32>,
    /// Color override per glyph. `None` = use style color.
    pub colors: Vec<Option<Color>>,
    /// Opacity override per glyph. `None` = fully opaque.
    pub opacities: Vec<Option<f32>>,
}
```

Vecs are auto-resized to match `ComputedGlyphLayout::glyphs.len()`. The rendering
system applies these overrides when building the final mesh.

---

## Built-In Effects

For users who do not want to write systems, we provide effect components. Each is a
Bevy system that reads the component's parameters and writes to `GlyphTransforms`.
Each uses `#[require(GlyphTransforms)]` so users never need to add it manually.

| Component | Writes to | Parameters |
|---|---|---|
| `WaveEffect` | `offsets[i].y` | amplitude, frequency, speed |
| `ShakeEffect` | `offsets[i]` | intensity, rate |
| `TypewriterEffect` | `opacities[i]` | speed, start_time |
| `RainbowEffect` | `colors[i]` | speed, spatial_frequency, saturation, lightness |
| `FadeEffect` | `opacities[i]` | target, duration, start_time |
| `PopEffect` | `scales[i]` | stagger, pop_duration, start_time |

### Stacking effects

Effects compose by writing to different fields. Add multiple effect components to
the same entity:

```rust
commands.spawn((
    Text3d::new("BOSS DEFEATED"),
    WaveEffect::new(5.0, 0.3, 3.0),
    RainbowEffect::new(),
    ShakeEffect::new(1.5, 0.0),
));
```

### Effect conflicts

Effects that write to the same field conflict -- the last system to run wins:

- `ShakeEffect` + `WaveEffect` both write `offsets` → shake overwrites wave's Y
- `TypewriterEffect` + `FadeEffect` both write `opacities` → last one wins

When conflicts matter, write a custom system that composes the effects yourself.

### Custom effects

No trait needed. Write a system, query `ComputedGlyphLayout` and `GlyphTransforms`,
do math:

```rust
fn scatter_effect(
    time: Res<Time>,
    mut query: Query<(&ComputedGlyphLayout, &mut GlyphTransforms, &ScatterEffect)>,
) {
    let t = time.elapsed_secs();
    for (layout, mut transforms, scatter) in &mut query {
        let progress = ((t - scatter.start_time) / scatter.duration).clamp(0.0, 1.0);
        for (i, glyph) in layout.glyphs.iter().enumerate() {
            let angle = glyph.total_advance * 1.5 + (i as f32) * 0.7;
            let radius = progress * scatter.radius;
            transforms.offsets[i] = Vec2::new(angle.cos() * radius, angle.sin() * radius);
            transforms.opacities[i] = Some(1.0 - progress);
        }
    }
}
```

---

## Shader-Level Effects (Materials)

Some effects are better done in the fragment shader: hologram scan lines, CRT
distortion, glitch, chromatic aberration. These are not per-glyph -- they affect
the entire text mesh. They use Bevy's `ExtendedMaterial` system to compose with
`StandardMaterial` lighting.

| Material | Visual |
|---|---|
| `HologramMaterial` | Scan lines + opacity noise + chromatic aberration |
| `CrtMaterial` | Barrel distortion + phosphor glow + flicker |
| `GlitchMaterial` | Random block displacement + color channel separation |

Shader effects and per-glyph effects compose: a hologram material on a text entity
with a `WaveEffect` produces text that waves AND has scan lines.

---

## The Processing Comparison

```text
Processing                          bevy_diegetic
─────────                           ─────────────
textFont(mono)                      TextStyle::new().font(MONOSPACE)
textSize(32)                        .size(32.0)
fill(255, 0, 0)                     .color(Color::srgb(1.0, 0.0, 0.0))
textWidth("Hello")                  fonts.measure("Hello", &style).x
for c in str.chars() {              for glyph in layout.glyphs.iter() {
  float w = textWidth(c);             let offset = (t + glyph.total_advance).sin();
  text(c, x, y + sin(x));             transforms.offsets[i].y = offset;
  x += w;                           }
}
```

Same pattern, same simplicity. The difference: `ComputedGlyphLayout` already has the
positions, so you skip the measurement loop entirely.

---

## Open Question: Effect Organizing Principle

The built-in effects are currently individual components. This works but lacks
discoverability -- users find them by reading docs, not by autocomplete. We need
a unifying concept, similar to how `EasingFunction` organizes easing curves.

**Option A: `GlyphEffect` enum + single `TextEffects` component.**

A `GlyphEffect` enum with variants (`Wave`, `Shake`, `Typewriter`, etc.) and a
`TextEffects` component holding a `SmallVec<[GlyphEffect; 2]>`. Discoverable via
`GlyphEffect::` autocomplete. One system processes all effects. Can't
`Query<&WaveEffect>` individually.

**Option B: Keep individual components, add enum as convenience constructor.**

Preserve separate components for ECS queryability. Add a `GlyphEffect` enum that
spawns the right component. Best of both worlds but more surface area.

**Option C: Something else entirely.**

Maybe effects are better modeled as animation curves applied to glyph properties,
a mini-DSL, or a trait with a blanket system. Needs prototyping.

This question should be resolved before Phase 5 implementation. The core mechanism
(`ComputedGlyphLayout` + `GlyphTransforms` + Bevy systems) is decided. The question
is how to organize the built-in effects for discoverability.

---

## Future Directions

These are effects that the architecture enables but that are not in the initial
implementation. Each is a standalone Bevy system -- no changes to the core text
pipeline required.

### Processing-Inspired Effects

- **Bounce/drop** -- staggered gravity sim per character (text "drops in" letter by letter)
- **Scatter/explode** -- glyphs fly outward from center on trigger
- **Gradient sweep** -- color lerp or highlight band that moves across text over time
- **Dissolve** -- random per-glyph opacity fade to simulate text evaporating
- **Text on curves** -- override glyph positions to follow a bezier, circle, or spiral

### Diegetic / Shader Effects

- **Neon glow** -- MSDF outline with emissive bloom
- **Terminal boot** -- typewriter reveal + jitter as each character "locks in"

### Physics-Driven Text

Per-glyph entities enable integration with physics engines like Avian:

- Attach `RigidBody` + `Collider` to individual glyph entities
- Avian simulates physics; the sync system maps entity transforms back to
  `GlyphTransforms`
- **Squish on impact** -- characters drop from height, compress on collision via
  scale, bounce back with a spring animation
- **Shatter** -- a word explodes into tumbling individual letters when hit
- **Dangling chain** -- characters connected by `DistanceJoint` constraints swing
  when nudged

### Kinetic Typography

Choreographed multi-effect sequences timed to audio or events:

- Scale + position + opacity + color animated per-glyph on a timeline
- Stagger delays for cascading reveals
- Integration with Bevy's animation system for keyframed glyph properties
- Potential for a declarative timeline DSL or asset format

### Data-Driven Text

Per-glyph properties driven by external data:

- Color-code characters based on real-time values (red for errors, green for OK)
- Size individual characters based on data magnitude (text sparklines)
- Opacity based on relevance scores (dim stale data, brighten fresh data)
