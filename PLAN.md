# Panel Rendering Plan

## Architecture Decision

**Hybrid approach: RTT + direct 3D geometry for physical interactive elements.**

- Panel content (backgrounds, text, borders, images) composited via render-to-texture (orthographic, no depth buffer, back-to-front alpha blending)
- Result displayed as a single textured quad in 3D space
- Interactive elements needing physical depth (buttons, switches) spawned as separate 3D meshes positioned by layout bounding boxes
- Press animations translate geometry along panel normal

### RTT at Scale (300+ panels — deferred until needed)

- Render at full resolution with mip chain; GPU handles distance-based quality automatically
- Only re-render on `Changed<DiegeticPanel>` (static panels cost zero after first frame)
- Frustum culling and texture deallocation if memory becomes an issue

### Alternative: Direct 3D Geometry (no RTT)

If RTT memory budget is unacceptable for a given use case:

- Batched vertex-colored meshes rendered directly in scene
- Depth writes disabled for panel internals
- Draw order enforced via custom render phase or sort key
- Tradeoff: zero texture memory, but ~4x draw calls per panel and no frame caching

## Implementation Phases

### Phase 1: Rectangle Backgrounds

- Extract `RenderCommand::Rectangle` from `ComputedDiegeticPanel`
- Batch all background quads into a single vertex-colored mesh per panel
- Render with `StandardMaterial` (vertex colors, `AlphaMode::Blend`, `double_sided: true`)
- Initial implementation uses direct 3D geometry (RTT infrastructure comes later)

### Phase 2: Borders

- Extract `RenderCommand::Border` from `ComputedDiegeticPanel`
- Each border produces 1-4 thin rectangles (top/right/bottom/left edges)
- Batch all border quads into a single vertex-colored mesh per panel
- Same material approach as backgrounds

### Phase 3: Shader-Based Clipping

- Handle `ScissorStart`/`ScissorEnd` render commands
- Pass `clip_min`/`clip_max` as uniforms, discard fragments outside
- One material instance per active clip region
- Covers the common case: overflow clipping on containers

### Phase 4: Image Support

- Extract `RenderCommand::Image` from `ComputedDiegeticPanel`
- Each image gets its own textured quad + `StandardMaterial` with the image handle
- Cannot batch across different textures (one draw call per image)

### Phase 5: RTT Compositing

- Render panel content to offscreen texture (orthographic camera, no depth buffer)
- Back-to-front alpha blending within the RTT pass
- Display result as a single textured quad in 3D space
- Eliminates all depth ordering concerns for flat panel content
- Only re-render when panel content changes

### Phase 6: RTT Scaling Infrastructure (deferred — only if memory becomes an issue)

- Render at full resolution with mip chain; GPU selects mip level by screen coverage automatically
- No custom LOD logic, no re-render on camera distance — only on `Changed<DiegeticPanel>`
- Frustum culling: deallocate textures for off-screen panels
- Build this only if panel count × resolution causes measurable memory pressure

### Phase 7: Physical Interactive Elements

- Layout tree flag to mark elements as "physical" (new `ElementContent` variant or flag on `El`)
- Panel renderer skips physical regions in RTT composition
- Spawns 3D geometry (extruded quads, beveled boxes) at bounding box positions
- Press animation: translate along panel normal
- Hit testing: ray-plane intersection → panel-local coordinates → bounding box walk

## Hit Testing (applies to all phases)

- Cast ray from camera through cursor
- Intersect with panel quad plane in world space
- Transform hit point into panel-local coordinates
- Walk render commands to find containing `BoundingBox`
- Works identically regardless of rendering approach (RTT or direct geometry)

## Material Strategy

- Phases 1-4: `StandardMaterial` with vertex colors for rectangles/borders, per-image materials for images
- Text: existing `MsdfTextMaterial` (unchanged)
- Phase 5+: RTT output displayed via `StandardMaterial` with the rendered texture
- Physical elements (Phase 7): `StandardMaterial` or custom material per element
