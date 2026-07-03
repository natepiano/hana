//! 3D grid-based A* pathfinding for cable routing around obstacles.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::collections::HashMap;

use bevy::math::Vec3;
use bevy_kana::ToF32;
use bevy_kana::ToI32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::constants::ASTAR_CLEAR_CELL_SEARCH_RADIUS;
use super::constants::ASTAR_SEGMENT_SAMPLE_STEPS;
use super::constants::ASTAR_SHORTCUT_SAMPLES_PER_CELL;
use super::constants::COLLINEARITY_THRESHOLD;
use super::constants::DEFAULT_ASTAR_MAX_CELLS;
use super::constants::DEFAULT_GRID_SIZE;
use super::constants::DEFAULT_OBSTACLE_MARGIN;
use super::constants::MIN_CABLE_SAMPLE_POINTS;
use super::obstacle;
use super::obstacle::Blockage;
use super::obstacle::Obstacle;
use super::obstacle::PointContainment;
use super::solver::PathPlanner;

/// 3D grid cell coordinate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Cell {
    x: i32,
    y: i32,
    z: i32,
}

impl Cell {
    fn to_world(self, origin: Vec3, grid_size: f32) -> Vec3 {
        origin
            + Vec3::new(
                self.x.to_f32() * grid_size,
                self.y.to_f32() * grid_size,
                self.z.to_f32() * grid_size,
            )
    }
}

/// Entry in the A* priority queue (min-heap by `f_score`).
struct OpenEntry {
    cell:    Cell,
    f_score: f32,
}

impl PartialEq for OpenEntry {
    fn eq(&self, other: &Self) -> bool { self.f_score == other.f_score }
}

impl Eq for OpenEntry {}

impl PartialOrd for OpenEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

impl Ord for OpenEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse order for min-heap (`BinaryHeap` is max-heap)
        other
            .f_score
            .partial_cmp(&self.f_score)
            .unwrap_or(Ordering::Equal)
    }
}

/// 3D grid-based A* path planner that routes around obstacles.
#[derive(Clone, Debug)]
pub struct AStarPlanner {
    /// Voxel size for the search grid.
    pub grid_size: f32,
    /// Clearance margin around obstacles.
    pub margin:    f32,
    /// Maximum number of cells to explore before giving up.
    pub max_cells: usize,
}

impl Default for AStarPlanner {
    fn default() -> Self {
        Self {
            grid_size: DEFAULT_GRID_SIZE,
            margin:    DEFAULT_OBSTACLE_MARGIN,
            max_cells: DEFAULT_ASTAR_MAX_CELLS,
        }
    }
}

impl AStarPlanner {
    /// Create a planner with default settings.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            grid_size: DEFAULT_GRID_SIZE,
            margin:    DEFAULT_OBSTACLE_MARGIN,
            max_cells: DEFAULT_ASTAR_MAX_CELLS,
        }
    }

    /// Set the grid cell size.
    #[must_use]
    pub const fn with_grid_size(mut self, grid_size: f32) -> Self {
        self.grid_size = grid_size;
        self
    }

    /// Set the obstacle clearance margin.
    #[must_use]
    pub const fn with_margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// Convert a world position to the nearest grid cell.
    fn world_to_cell(&self, position: Vec3, origin: Vec3) -> Cell {
        let relative = position - origin;
        Cell {
            x: (relative.x / self.grid_size).round().to_i32(),
            y: (relative.y / self.grid_size).round().to_i32(),
            z: (relative.z / self.grid_size).round().to_i32(),
        }
    }

    /// Check if a world-space point is inside any obstacle (with margin).
    fn is_blocked(&self, position: Vec3, obstacles: &[Obstacle]) -> Blockage {
        match obstacle::is_point_in_any_obstacle(position, obstacles, self.margin) {
            PointContainment::Inside => Blockage::Blocked,
            PointContainment::Outside => Blockage::Clear,
        }
    }

    /// The clear cell nearest to `cell`, searched within
    /// [`ASTAR_CLEAR_CELL_SEARCH_RADIUS`]. A route endpoint can sit clear of an
    /// obstacle while its quantized cell center lands inside the inflated box
    /// (when `margin` or `grid_size` exceeds the endpoint's distance to the
    /// obstacle face); snapping keeps `find_path`'s goal reachable instead of
    /// silently falling back to a straight line through the obstacle.
    fn nearest_clear_cell(&self, cell: Cell, origin: Vec3, obstacles: &[Obstacle]) -> Option<Cell> {
        let target = cell.to_world(origin, self.grid_size);
        let radius = ASTAR_CLEAR_CELL_SEARCH_RADIUS;
        (-radius..=radius)
            .flat_map(|dx| {
                (-radius..=radius)
                    .flat_map(move |dy| (-radius..=radius).map(move |dz| (dx, dy, dz)))
            })
            .map(|(dx, dy, dz)| Cell {
                x: cell.x + dx,
                y: cell.y + dy,
                z: cell.z + dz,
            })
            .filter(|candidate| {
                match self.is_blocked(candidate.to_world(origin, self.grid_size), obstacles) {
                    Blockage::Clear => true,
                    Blockage::Blocked => false,
                }
            })
            .min_by(|a, b| {
                let a_distance = a.to_world(origin, self.grid_size).distance_squared(target);
                let b_distance = b.to_world(origin, self.grid_size).distance_squared(target);
                a_distance
                    .partial_cmp(&b_distance)
                    .unwrap_or(Ordering::Equal)
            })
    }

    /// 26-connected neighbors (all adjacent cells including diagonals).
    fn neighbors(cell: Cell) -> impl Iterator<Item = Cell> {
        (-1..=1)
            .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| (dx, dy, dz))))
            .filter(|&(dx, dy, dz)| dx != 0 || dy != 0 || dz != 0)
            .map(move |(dx, dy, dz)| Cell {
                x: cell.x + dx,
                y: cell.y + dy,
                z: cell.z + dz,
            })
    }

    /// Euclidean distance between two cells (heuristic).
    fn heuristic(a: Cell, b: Cell) -> f32 {
        let dx = (a.x - b.x).to_f32();
        let dy = (a.y - b.y).to_f32();
        let dz = (a.z - b.z).to_f32();
        dz.mul_add(dz, dx.mul_add(dx, dy * dy)).sqrt()
    }

    /// Run `A*` and return the path as grid cells.
    fn find_path(
        &self,
        start: Cell,
        goal: Cell,
        origin: Vec3,
        obstacles: &[Obstacle],
    ) -> Option<Vec<Cell>> {
        let mut open = BinaryHeap::new();
        let mut came_from: HashMap<Cell, Cell> = HashMap::new();
        let mut g_score: HashMap<Cell, f32> = HashMap::new();
        let mut explored = 0_usize;

        g_score.insert(start, 0.0);
        open.push(OpenEntry {
            cell:    start,
            f_score: Self::heuristic(start, goal),
        });

        while let Some(current_entry) = open.pop() {
            let current = current_entry.cell;

            if current == goal {
                // Follow `came_from` with the `node` cursor, then reverse `path`.
                let mut path = vec![current];
                let mut node = current;
                while let Some(&prev) = came_from.get(&node) {
                    path.push(prev);
                    node = prev;
                }
                path.reverse();
                return Some(path);
            }

            explored += 1;
            if explored > self.max_cells {
                return None;
            }

            let current_g = g_score[&current];

            for neighbor in Self::neighbors(current) {
                let neighbor_world = neighbor.to_world(origin, self.grid_size);

                match self.is_blocked(neighbor_world, obstacles) {
                    Blockage::Blocked => continue,
                    Blockage::Clear => {},
                }

                let move_cost = Self::heuristic(current, neighbor) * self.grid_size;
                let tentative_g = current_g + move_cost;

                let is_better = g_score
                    .get(&neighbor)
                    .is_none_or(|&existing| tentative_g < existing);

                if is_better {
                    g_score.insert(neighbor, tentative_g);
                    came_from.insert(neighbor, current);
                    let f = Self::heuristic(neighbor, goal).mul_add(self.grid_size, tentative_g);
                    open.push(OpenEntry {
                        cell:    neighbor,
                        f_score: f,
                    });
                }
            }
        }

        None
    }
}

impl PathPlanner for AStarPlanner {
    fn plan(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Vec<Vec3> {
        if obstacles.is_empty() {
            return vec![start, end];
        }

        match self.is_direct_path_blocked(start, end, obstacles) {
            Blockage::Clear => return vec![start, end],
            Blockage::Blocked => {},
        }

        let origin = start;
        // Snap each endpoint's cell to the nearest clear cell; the exact
        // start/end positions are restored on the waypoint list below.
        let snapped_cells = (
            self.nearest_clear_cell(self.world_to_cell(start, origin), origin, obstacles),
            self.nearest_clear_cell(self.world_to_cell(end, origin), origin, obstacles),
        );
        let (Some(start_cell), Some(goal_cell)) = snapped_cells else {
            return vec![start, end];
        };

        let Some(path_cells) = self.find_path(start_cell, goal_cell, origin, obstacles) else {
            return vec![start, end];
        };

        // Convert `path_cells` into `waypoints` with `Cell::to_world`.
        let mut waypoints: Vec<Vec3> = path_cells
            .iter()
            .map(|c| c.to_world(origin, self.grid_size))
            .collect();

        // Ensure exact start and end positions
        if let Some(first) = waypoints.first_mut() {
            *first = start;
        }
        if let Some(last) = waypoints.last_mut() {
            *last = end;
        }

        // Pull the path taut before dropping collinear points: A*'s
        // cell-by-cell moves leave staircase jogs that `shortcut_path`
        // replaces with the longest clear straight runs.
        self.shortcut_path(&mut waypoints, obstacles);

        // `simplify_path` removes collinear entries from `waypoints`.
        simplify_path(&mut waypoints);

        waypoints
    }
}

impl AStarPlanner {
    /// Check if any obstacle intersects the direct line from start to end.
    fn is_direct_path_blocked(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Blockage {
        obstacle::is_segment_blocked(
            start,
            end,
            obstacles,
            self.margin,
            ASTAR_SEGMENT_SAMPLE_STEPS,
        )
    }

    /// Pull the path taut: from each kept waypoint, jump straight to the
    /// farthest later waypoint whose connecting segment clears every obstacle,
    /// discarding the grid staircase in between.
    fn shortcut_path(&self, waypoints: &mut Vec<Vec3>, obstacles: &[Obstacle]) {
        let Some(&first) = waypoints.first() else {
            return;
        };
        let mut shortened = vec![first];
        let mut current = 0;
        while current + 1 < waypoints.len() {
            let next = (current + 1..waypoints.len())
                .rev()
                .find(|&candidate| {
                    match self.is_shortcut_blocked(
                        waypoints[current],
                        waypoints[candidate],
                        obstacles,
                    ) {
                        Blockage::Clear => true,
                        Blockage::Blocked => false,
                    }
                })
                .unwrap_or(current + 1);
            shortened.push(waypoints[next]);
            current = next;
        }
        *waypoints = shortened;
    }

    /// Segment blockage test whose sample count scales with segment length
    /// ([`ASTAR_SHORTCUT_SAMPLES_PER_CELL`] per grid cell), so a long shortcut
    /// cannot step over a thin obstacle between samples.
    fn is_shortcut_blocked(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Blockage {
        let steps = (start.distance(end) / self.grid_size * ASTAR_SHORTCUT_SAMPLES_PER_CELL)
            .ceil()
            .to_u32()
            .max(1);
        obstacle::is_segment_blocked(start, end, obstacles, self.margin, steps)
    }
}

/// Remove collinear waypoints from a path.
fn simplify_path(waypoints: &mut Vec<Vec3>) {
    if waypoints.len() <= MIN_CABLE_SAMPLE_POINTS.to_usize() {
        return;
    }

    let mut simplified = Vec::with_capacity(waypoints.len());
    simplified.push(waypoints[0]);

    for i in 1..waypoints.len() - 1 {
        let prev = simplified.last().copied().unwrap_or(waypoints[i]);
        let next = waypoints[i + 1];
        let current = waypoints[i];

        let incoming_direction = (current - prev).normalize_or_zero();
        let outgoing_direction = (next - current).normalize_or_zero();

        // Keep the waypoint if direction changes significantly
        if incoming_direction.dot(outgoing_direction) < COLLINEARITY_THRESHOLD {
            simplified.push(current);
        }
    }

    simplified.push(*waypoints.last().unwrap_or(&Vec3::ZERO));
    *waypoints = simplified;
}
