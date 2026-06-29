# Architecture

## Guiding Principle: Math and Rendering are Separate

Inspired by Clay's layout library and `bevy_diegetic`, this crate splits into two layers:

```
┌─────────────────────────────────────────────────────────┐
│  routing/  — Pure math (depends only on glam)           │
│                                                         │
│  Input:   RouteRequest (start, end, obstacles, params)  │
│  Output:  CableGeometry (points, tangents, arc lengths) │
│                                                         │
│  No Bevy. No ECS. No rendering. Fully testable.         │
└──────────────────────────┬──────────────────────────────┘
                           │
                     CableGeometry
                           │
┌──────────────────────────▼──────────────────────────────┐
│  plugin/  — Bevy integration (thin layer)               │
│                                                         │
│  Components:  Cable, CableEndpoint, AttachedTo          │
│  Observer:    on_geometry_computed                       │
│  Rendering:   tube mesh generation, debug gizmos        │
└─────────────────────────────────────────────────────────┘
```

The routing layer knows nothing about Bevy. The plugin layer knows nothing about how routes are computed. `CableGeometry` is the seam between them — a render-agnostic description of what was computed.

## Why This Matters

- **Testability**: Route math is tested with plain `#[test]` — no `App`, no `World`, no systems.
- **Multiple renderers**: Tube meshes, gizmo debug lines — all consume the same `CableGeometry`.
- **Multiple algorithms**: Catenary, A\*, orthogonal, composite — all produce the same `CableGeometry`.
- **Reusability**: The routing layer could be used outside Bevy (tools, headless validation, etc.).

## Module Structure

```
src/
├── lib.rs                     # pub use re-exports from both layers
│
├── routing/                   # Pure math — no Bevy dependency
│   ├── mod.rs                 # mod + pub use
│   ├── obstacle.rs            # Obstacle AABB + is_point_in_any_obstacle / is_segment_blocked
│   ├── geometry.rs            # Anchor, RouteRequest, CableSegment, CableGeometry
│   ├── constants.rs           # Named constants (no magic values)
│   ├── enums.rs               # Solver, Planner, Curve enum dispatch
│   ├── solver.rs              # RouteSolver/CurveSolver/PathPlanner traits, Router compositor
│   ├── catenary.rs            # Catenary math functions + CatenarySolver
│   ├── pathfinding.rs         # 3D A* + AStarPlanner
│   └── orthogonal.rs          # Orthogonal routing + OrthogonalPlanner
│
└── plugin/                    # Bevy integration
    ├── mod.rs                 # CatenaryPlugin
    ├── components.rs          # Cable, CableEndpoint, AttachedTo, ComputedCableGeometry
    ├── systems.rs             # compute_cable_routes, on_geometry_computed, debug gizmos
    └── mesh.rs                # CableGeometry → Mesh (tube generation, caps, elbows)
```

## Solver Dispatch

Users choose a solver via the `Solver` enum on the `Cable` component:

```
Solver::Linear          → LinearSolver (straight line)
Solver::Catenary(...)   → CatenarySolver (gravity sag)
Solver::Routed { planner, curve }
    planner: Planner::AStar       → AStarPlanner (obstacle avoidance)
             Planner::Orthogonal  → OrthogonalPlanner (Manhattan paths)
    curve:   Curve::Catenary(...) → CatenarySolver (sag between waypoints)
             Curve::Linear        → LinearSolver (straight segments)
```

The enums implement `RouteSolver` / `CurveSolver` / `PathPlanner` by delegating to the inner concrete type. This avoids trait objects while keeping the API ergonomic.

## Data Flow

```
1. User spawns Cable entity with child CableEndpoint entities:
   - Each endpoint has a CableEnd (Start/End) and optional AttachedTo(entity)
   - Cable holds solver choice, obstacles, and resolution

2. compute_cable_routes system (runs in Update):
   - Resolves endpoint positions from AttachedTo GlobalTransforms
   - Calls solver.solve(request) via Solver enum dispatch
   - Writes CableGeometry into ComputedCableGeometry

3. on_geometry_computed observer (fires on Insert<ComputedCableGeometry>):
   - Reads CableMeshConfig and endpoint CapStyle overrides
   - Calls generate_tube_mesh(geometry, config) → produces Mesh
   - First time: spawns mesh child entity with Mesh3d + optional material
   - Subsequent: mutates existing mesh asset in-place (no entity churn)
```

## Mesh Generation Pipeline

`generate_tube_mesh()` transforms `CableGeometry` + `CableMeshConfig` into a Bevy `Mesh`:

```
CableGeometry
    │
    ▼
flatten_geometry()          Merge all segments into one polyline
    │
    ▼
trim_path()                 Optional: remove start/end distance
    │
    ▼
insert_knee_rings()         Detect sharp bends, insert cubic Bezier
                            fillet points for smooth elbows
    │
    ▼
compute_rmf()               Rotation-minimizing frames: (normal, binormal)
                            pair at each path point — twist-free orientation
    │
    ▼
generate_tube_rings()       Sweep circular cross-section along path using
                            RMF frames. Produces positions, normals, UVs,
                            and quad indices connecting adjacent rings
    │
    ▼
apply_inside_normals()      For FaceSides::Both: duplicate vertices with
                            negated normals so interior faces receive light.
                            For FaceSides::Inside: negate all normals.
    │
    ▼
add_end_caps()              Round (hemisphere), Flat (disc), or None per end.
                            Inside caps get their own vertices with inward
                            normals for correct interior lighting.
    │
    ▼
Mesh                        TriangleList with positions, normals, UVs, indices
```

### CableMeshConfig

Controls mesh generation independently from path computation:

- `radius` / `sides` — cross-section circle dimensions
- `cap_start` / `cap_end` — `CapStyle::Round`, `Flat`, or `None` (overridden per-endpoint via `CableEndpoint::cap_style`)
- `face_sides` — `FaceSides::Outside`, `Inside`, or `Both`
- `trim_start` / `trim_end` — hide tube near endpoints (useful for junction overlap)
- `elbow_*` — bend radius, threshold angle, Bezier arm length for fillet smoothing
- `material` — optional `Handle<StandardMaterial>` applied to mesh child

## Dependency Strategy

The `routing/` module depends only on `glam` (which `bevy_math` re-exports, so types are compatible).

The `plugin/` module depends on `bevy`. It bridges glam types from `routing/` into the ECS world.

This means the routing module can be compiled and tested without pulling in the full Bevy dependency tree.
