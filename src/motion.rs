//! Shape motion. Movers travel across the screen in unfolded coordinates, so
//! they read as objects crossing the pattern rather than as more symmetry.

use rand::RngExt;

pub const MOVER_COUNT: usize = 4;

/// How a mover behaves when it reaches the edge of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Bounce,
    /// Leaves one edge and returns on the opposite one, its shape crossing the
    /// boundary in one piece.
    Wrap,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mover {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub edge: Edge,
}

/// The movers and the half extents of the visible area they travel in, in the
/// same units the field uses: the short screen axis runs from -1 to 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Motion {
    pub movers: Vec<Mover>,
    pub extent: [f32; 2],
}

impl Motion {
    pub fn random(rng: &mut impl RngExt) -> Motion {
        let movers = (0..MOVER_COUNT)
            .map(|_| {
                let speed = rng.random_range(0.013..0.07);
                let heading = rng.random_range(0.0..std::f32::consts::TAU);
                Mover {
                    position: [rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0)],
                    velocity: [speed * heading.cos(), speed * heading.sin()],
                    edge: if rng.random_bool(0.5) {
                        Edge::Bounce
                    } else {
                        Edge::Wrap
                    },
                }
            })
            .collect();
        Motion {
            movers,
            extent: [1.0, 1.0],
        }
    }

    /// Movers held still, for tests and for judging a genome without motion.
    pub fn still() -> Motion {
        Motion {
            movers: (0..MOVER_COUNT)
                .map(|index| Mover {
                    position: [index as f32 * 0.4 - 0.6, 0.2 - index as f32 * 0.3],
                    velocity: [0.0, 0.0],
                    edge: Edge::Bounce,
                })
                .collect(),
            extent: [1.0, 1.0],
        }
    }

    /// Match the travel area to the screen, where the short axis is -1 to 1.
    pub fn set_aspect(&mut self, width: f32, height: f32) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        self.extent = if width >= height {
            [width / height, 1.0]
        } else {
            [1.0, height / width]
        };
    }

    /// How far a point can sit from a wrapping mover before that mover's
    /// nearest copy changes.
    pub fn wrap_limit(&self) -> f32 {
        self.extent[0].min(self.extent[1])
    }

    pub fn update(&mut self, delta: f32) {
        let extent = self.extent;
        for mover in &mut self.movers {
            for (axis, limit) in extent.iter().copied().enumerate() {
                mover.position[axis] += mover.velocity[axis] * delta;
                match mover.edge {
                    Edge::Bounce => {
                        if mover.position[axis] > limit {
                            mover.position[axis] = 2.0 * limit - mover.position[axis];
                            mover.velocity[axis] = -mover.velocity[axis];
                        } else if mover.position[axis] < -limit {
                            mover.position[axis] = -2.0 * limit - mover.position[axis];
                            mover.velocity[axis] = -mover.velocity[axis];
                        }
                    }
                    Edge::Wrap => {
                        let span = 2.0 * limit;
                        if mover.position[axis] > limit {
                            mover.position[axis] -= span;
                        } else if mover.position[axis] < -limit {
                            mover.position[axis] += span;
                        }
                    }
                }
            }
        }
    }

    /// Offset from a mover to a point. A wrapping mover measures to whichever
    /// copy of itself is nearest, so its shape stays whole as it crosses an
    /// edge.
    pub fn delta(&self, index: usize, x: f32, y: f32) -> [f32; 2] {
        let Some(mover) = self.movers.get(index) else {
            return [0.0, 0.0];
        };
        let mut delta = [x - mover.position[0], y - mover.position[1]];
        if mover.edge == Edge::Wrap {
            for (axis, half_extent) in self.extent.iter().copied().enumerate() {
                let span = 2.0 * half_extent;
                delta[axis] -= span * (delta[axis] / span).round();
            }
        }
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::seeded_rng;

    #[test]
    fn a_bouncing_mover_turns_around_at_the_edge() {
        let mut motion = Motion::still();
        motion.movers[0] = Mover {
            position: [0.9, 0.0],
            velocity: [1.0, 0.0],
            edge: Edge::Bounce,
        };
        motion.update(0.2);
        assert!(motion.movers[0].position[0] < 1.0);
        assert_eq!(motion.movers[0].velocity[0], -1.0);
    }

    #[test]
    fn a_bouncing_mover_turns_around_at_the_far_edge_too() {
        let mut motion = Motion::still();
        motion.movers[0] = Mover {
            position: [-0.9, 0.0],
            velocity: [-1.0, 0.0],
            edge: Edge::Bounce,
        };
        motion.update(0.2);
        assert!(motion.movers[0].position[0] > -1.0);
        assert_eq!(motion.movers[0].velocity[0], 1.0);
    }

    #[test]
    fn a_wrapping_mover_returns_on_the_opposite_edge() {
        let mut motion = Motion::still();
        motion.movers[0] = Mover {
            position: [0.95, 0.95],
            velocity: [1.0, 1.0],
            edge: Edge::Wrap,
        };
        motion.update(0.1);
        assert!((motion.movers[0].position[0] + 0.95).abs() < 1e-5);
        assert!((motion.movers[0].position[1] + 0.95).abs() < 1e-5);
    }

    #[test]
    fn a_wrapping_mover_returns_from_the_low_edge_as_well() {
        let mut motion = Motion::still();
        motion.movers[0] = Mover {
            position: [-0.95, 0.0],
            velocity: [-1.0, 0.0],
            edge: Edge::Wrap,
        };
        motion.update(0.1);
        assert!((motion.movers[0].position[0] - 0.95).abs() < 1e-5);
    }

    #[test]
    fn the_travel_area_follows_the_screen_shape() {
        let mut motion = Motion::still();
        motion.set_aspect(1600.0, 1000.0);
        assert_eq!(motion.extent, [1.6, 1.0]);
        motion.set_aspect(1000.0, 2000.0);
        assert_eq!(motion.extent, [1.0, 2.0]);
    }

    #[test]
    fn a_screen_with_no_area_leaves_the_travel_area_alone() {
        let mut motion = Motion::still();
        motion.set_aspect(0.0, 100.0);
        assert_eq!(motion.extent, [1.0, 1.0]);
    }

    #[test]
    fn a_bouncing_mover_measures_distance_directly() {
        let motion = Motion::still();
        let delta = motion.delta(0, 0.4, 0.4);
        assert_eq!(delta, [0.4 - (-0.6), 0.4 - 0.2]);
    }

    #[test]
    fn a_wrapping_mover_measures_to_its_nearest_copy() {
        let mut motion = Motion::still();
        motion.movers[0] = Mover {
            position: [0.9, 0.0],
            velocity: [0.0, 0.0],
            edge: Edge::Wrap,
        };
        let delta = motion.delta(0, -0.9, 0.0);
        assert!((delta[0] - 0.2).abs() < 1e-5, "delta was {delta:?}");
    }

    #[test]
    fn the_wrap_limit_is_the_shorter_half_extent() {
        let mut motion = Motion::still();
        motion.set_aspect(1600.0, 1000.0);
        assert_eq!(motion.wrap_limit(), 1.0);
    }

    #[test]
    fn a_mover_index_that_does_not_exist_has_no_offset() {
        assert_eq!(Motion::still().delta(99, 0.5, 0.5), [0.0, 0.0]);
    }

    #[test]
    fn random_movers_all_start_inside_the_screen_and_moving() {
        let motion = Motion::random(&mut seeded_rng(3));
        assert_eq!(motion.movers.len(), MOVER_COUNT);
        for mover in &motion.movers {
            assert!(mover.position.iter().all(|axis| axis.abs() <= 1.0));
            assert!(mover.velocity[0].hypot(mover.velocity[1]) > 0.0);
        }
    }
}
