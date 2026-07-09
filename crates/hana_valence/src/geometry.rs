use bevy_ecs::prelude::Component;
use bevy_ecs::prelude::ReflectComponent;
use bevy_math::Dir3;
use bevy_math::Quat;
use bevy_math::Vec3;
use bevy_platform::collections::HashMap;
use bevy_reflect::Reflect;

/// Stable identifier for an anchor point emitted by an authoring provider.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Reflect)]
#[non_exhaustive]
pub enum AnchorId {
    /// Numbered vertex anchor.
    Vertex(u32),
    /// Numbered edge midpoint anchor.
    EdgeMid(u32),
    /// Central fallback anchor.
    #[default]
    Center,
}

/// Local anchor point authored in the provider's entity frame.
#[derive(Clone, Copy, Debug, Default, Reflect)]
pub struct AnchorPoint {
    /// Local-frame position in authored units.
    pub position: Vec3,
    /// Optional local tangent frame for non-flat providers.
    pub frame:    Option<Quat>,
}

impl AnchorPoint {
    /// Returns `frame` or [`Quat::IDENTITY`] when the point uses the entity frame.
    #[must_use]
    pub fn rotation(&self) -> Quat { self.frame.unwrap_or(Quat::IDENTITY) }
}

/// Ordered edge between two anchor points.
///
/// Endpoint order is significant: swapping `start` and `end` reverses the
/// returned edge axis.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Reflect)]
pub struct Edge {
    /// Edge start anchor.
    pub start: AnchorId,
    /// Edge end anchor.
    pub end:   AnchorId,
}

impl Edge {
    /// Minimum endpoint separation, in authored units, for an axis to be
    /// meaningful; separations below this are float noise, not direction.
    const MIN_AXIS_LENGTH: f32 = 1e-4;

    /// Returns the unit direction from `start` to `end`.
    ///
    /// Missing anchors, near-coincident positions, and mismatched endpoint
    /// frames are reported as [`EdgeAxisError`] values instead of creating
    /// invalid axes.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeAxisError::MissingAnchor`] when either endpoint is absent,
    /// [`EdgeAxisError::Degenerate`] when the endpoints are separated by less
    /// than `MIN_AXIS_LENGTH`, and [`EdgeAxisError::FrameDivergent`] when
    /// endpoint frames differ.
    pub fn axis(&self, geometry: &ResolvedAnchorGeometry) -> Result<Dir3, EdgeAxisError> {
        let start = geometry
            .points
            .get(&self.start)
            .ok_or(EdgeAxisError::MissingAnchor(self.start))?;
        let end = geometry
            .points
            .get(&self.end)
            .ok_or(EdgeAxisError::MissingAnchor(self.end))?;

        if frames_diverge(start.frame, end.frame) {
            return Err(EdgeAxisError::FrameDivergent);
        }

        let separation = end.position - start.position;
        if separation.length_squared() < Self::MIN_AXIS_LENGTH * Self::MIN_AXIS_LENGTH {
            return Err(EdgeAxisError::Degenerate);
        }

        Dir3::new(separation).map_err(|_| EdgeAxisError::Degenerate)
    }
}

/// Edge-axis resolution failure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EdgeAxisError {
    /// The edge references an anchor missing from [`ResolvedAnchorGeometry::points`].
    MissingAnchor(AnchorId),
    /// The edge endpoints are separated by less than the minimum axis length.
    Degenerate,
    /// The edge endpoints use different optional tangent frames.
    FrameDivergent,
}

/// Provider-filled anchor geometry for one entity.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct ResolvedAnchorGeometry {
    /// Anchor points keyed by provider-authored ids.
    pub points: HashMap<AnchorId, AnchorPoint>,
    /// Ordered edges between entries in `points`.
    pub edges:  Vec<Edge>,
}

impl ResolvedAnchorGeometry {
    /// Validates provider output before resolver systems read it.
    ///
    /// Validation checks finite positions, existing edge endpoints, distinct
    /// edge points, and matching optional endpoint frames. In debug builds,
    /// provider quaternions are also asserted to be unit quaternions.
    ///
    /// # Errors
    ///
    /// Returns a [`GeometryError`] describing the first invalid anchor point or
    /// edge found in provider output.
    pub fn validate(&self) -> Result<(), GeometryError> {
        for (anchor_id, point) in &self.points {
            if !point.position.is_finite() {
                return Err(GeometryError::NonFinitePoint(*anchor_id));
            }
            if let Some(frame) = point.frame {
                debug_assert!(frame.is_normalized());
            }
        }

        for edge in &self.edges {
            self.validate_edge(*edge)?;
        }

        Ok(())
    }

    fn validate_edge(&self, edge: Edge) -> Result<(), GeometryError> {
        let start = self
            .points
            .get(&edge.start)
            .ok_or(GeometryError::MissingAnchor {
                edge,
                anchor: edge.start,
            })?;
        let end = self
            .points
            .get(&edge.end)
            .ok_or(GeometryError::MissingAnchor {
                edge,
                anchor: edge.end,
            })?;

        if edge.start == edge.end {
            return Err(GeometryError::DegenerateEdge(edge));
        }
        if frames_diverge(start.frame, end.frame) {
            return Err(GeometryError::FrameDivergentEdge(edge));
        }

        edge.axis(self).map_err(|axis_error| match axis_error {
            EdgeAxisError::MissingAnchor(anchor) => GeometryError::MissingAnchor { edge, anchor },
            EdgeAxisError::Degenerate => GeometryError::DegenerateEdge(edge),
            EdgeAxisError::FrameDivergent => GeometryError::FrameDivergentEdge(edge),
        })?;

        Ok(())
    }
}

/// Geometry validation failure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GeometryError {
    /// An edge references an anchor missing from [`ResolvedAnchorGeometry::points`].
    MissingAnchor {
        /// Edge that failed validation.
        edge:   Edge,
        /// Missing anchor referenced by `edge`.
        anchor: AnchorId,
    },
    /// An edge has identical endpoints or near-coincident endpoint positions.
    DegenerateEdge(Edge),
    /// An anchor point has a non-finite position.
    NonFinitePoint(AnchorId),
    /// An edge connects endpoint frames that do not match.
    FrameDivergentEdge(Edge),
}

fn frames_diverge(start: Option<Quat>, end: Option<Quat>) -> bool { start != end }

#[cfg(test)]
mod tests {
    use bevy_math::Quat;
    use bevy_math::Vec3;
    use bevy_platform::collections::HashMap;

    use super::AnchorId;
    use super::AnchorPoint;
    use super::Edge;
    use super::EdgeAxisError;
    use super::GeometryError;
    use super::ResolvedAnchorGeometry;

    fn flat_quad_geometry() -> ResolvedAnchorGeometry {
        ResolvedAnchorGeometry {
            points: HashMap::from_iter([
                (
                    AnchorId::Vertex(0),
                    AnchorPoint {
                        position: Vec3::new(-1.0, -1.0, 0.0),
                        frame:    None,
                    },
                ),
                (
                    AnchorId::Vertex(1),
                    AnchorPoint {
                        position: Vec3::new(1.0, -1.0, 0.0),
                        frame:    None,
                    },
                ),
                (
                    AnchorId::Vertex(2),
                    AnchorPoint {
                        position: Vec3::new(1.0, 1.0, 0.0),
                        frame:    None,
                    },
                ),
                (
                    AnchorId::Vertex(3),
                    AnchorPoint {
                        position: Vec3::new(-1.0, 1.0, 0.0),
                        frame:    None,
                    },
                ),
            ]),
            edges:  vec![
                Edge {
                    start: AnchorId::Vertex(0),
                    end:   AnchorId::Vertex(1),
                },
                Edge {
                    start: AnchorId::Vertex(1),
                    end:   AnchorId::Vertex(2),
                },
                Edge {
                    start: AnchorId::Vertex(2),
                    end:   AnchorId::Vertex(3),
                },
                Edge {
                    start: AnchorId::Vertex(3),
                    end:   AnchorId::Vertex(0),
                },
            ],
        }
    }

    #[test]
    fn valid_flat_quad_geometry_passes_validation() {
        assert_eq!(flat_quad_geometry().validate(), Ok(()));
    }

    #[test]
    fn missing_anchor_edge_reports_missing_anchor() {
        let edge = Edge {
            start: AnchorId::Vertex(0),
            end:   AnchorId::Vertex(1),
        };
        let geometry = ResolvedAnchorGeometry {
            points: HashMap::from_iter([(
                AnchorId::Vertex(0),
                AnchorPoint {
                    position: Vec3::ZERO,
                    frame:    None,
                },
            )]),
            edges:  vec![edge],
        };

        assert_eq!(
            geometry.validate(),
            Err(GeometryError::MissingAnchor {
                edge,
                anchor: AnchorId::Vertex(1),
            }),
        );
        assert_eq!(
            edge.axis(&geometry),
            Err(EdgeAxisError::MissingAnchor(AnchorId::Vertex(1))),
        );
    }

    #[test]
    fn degenerate_edge_reports_degenerate() {
        let edge = Edge {
            start: AnchorId::Vertex(0),
            end:   AnchorId::Vertex(1),
        };
        let geometry = ResolvedAnchorGeometry {
            points: HashMap::from_iter([
                (
                    AnchorId::Vertex(0),
                    AnchorPoint {
                        position: Vec3::ZERO,
                        frame:    None,
                    },
                ),
                (
                    AnchorId::Vertex(1),
                    AnchorPoint {
                        position: Vec3::ZERO,
                        frame:    None,
                    },
                ),
            ]),
            edges:  vec![edge],
        };

        assert_eq!(
            geometry.validate(),
            Err(GeometryError::DegenerateEdge(edge))
        );
        assert_eq!(edge.axis(&geometry), Err(EdgeAxisError::Degenerate));
    }

    #[test]
    fn sub_minimum_length_edge_reports_degenerate() {
        let edge = Edge {
            start: AnchorId::Vertex(0),
            end:   AnchorId::Vertex(1),
        };
        let geometry = ResolvedAnchorGeometry {
            points: HashMap::from_iter([
                (
                    AnchorId::Vertex(0),
                    AnchorPoint {
                        position: Vec3::ZERO,
                        frame:    None,
                    },
                ),
                (
                    AnchorId::Vertex(1),
                    AnchorPoint {
                        position: Vec3::X * (Edge::MIN_AXIS_LENGTH * 0.5),
                        frame:    None,
                    },
                ),
            ]),
            edges:  vec![edge],
        };

        assert_eq!(
            geometry.validate(),
            Err(GeometryError::DegenerateEdge(edge))
        );
        assert_eq!(edge.axis(&geometry), Err(EdgeAxisError::Degenerate));
    }

    #[test]
    fn non_finite_point_reports_anchor_id() {
        let mut geometry = flat_quad_geometry();
        geometry.points.insert(
            AnchorId::Center,
            AnchorPoint {
                position: Vec3::new(f32::NAN, 0.0, 0.0),
                frame:    None,
            },
        );

        assert_eq!(
            geometry.validate(),
            Err(GeometryError::NonFinitePoint(AnchorId::Center)),
        );
    }

    #[test]
    fn frame_divergent_edge_reports_frame_divergence() {
        let edge = Edge {
            start: AnchorId::Vertex(0),
            end:   AnchorId::Vertex(1),
        };
        let geometry = ResolvedAnchorGeometry {
            points: HashMap::from_iter([
                (
                    AnchorId::Vertex(0),
                    AnchorPoint {
                        position: Vec3::ZERO,
                        frame:    Some(Quat::IDENTITY),
                    },
                ),
                (
                    AnchorId::Vertex(1),
                    AnchorPoint {
                        position: Vec3::X,
                        frame:    Some(Quat::from_rotation_z(1.0)),
                    },
                ),
            ]),
            edges:  vec![edge],
        };

        assert_eq!(
            geometry.validate(),
            Err(GeometryError::FrameDivergentEdge(edge)),
        );
        assert_eq!(edge.axis(&geometry), Err(EdgeAxisError::FrameDivergent));
    }

    #[test]
    fn anchor_point_rotation_uses_identity_for_missing_frame() {
        let point = AnchorPoint {
            position: Vec3::ZERO,
            frame:    None,
        };

        assert_eq!(point.rotation(), Quat::IDENTITY);
    }

    #[test]
    fn anchor_point_rotation_uses_authored_frame() {
        let rotation = Quat::from_rotation_z(1.0);
        let point = AnchorPoint {
            position: Vec3::ZERO,
            frame:    Some(rotation),
        };

        assert_eq!(point.rotation(), rotation);
    }
}
