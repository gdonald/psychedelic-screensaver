//! CPU evaluation of a genome's field. The GPU path in `msl` renders the same
//! expression; this one exists so patterns can be rendered and tested headless.

use crate::genome::{Expr, Genome, Layer, TIME_SCALE, Warp, fold_spun};
use crate::motion::Motion;

pub struct Context<'a> {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub theta: f32,
    pub time: f32,
    pub params: &'a [f32],
    /// Unfolded position, which is what movers are measured against.
    pub raw_x: f32,
    pub raw_y: f32,
    pub motion: &'a Motion,
}

impl Context<'_> {
    fn mover_delta(&self, index: usize) -> [f32; 2] {
        self.motion.delta(index, self.raw_x, self.raw_y)
    }
}

pub fn eval(expr: &Expr, ctx: &Context) -> f32 {
    match expr {
        Expr::X => ctx.x,
        Expr::Y => ctx.y,
        Expr::Radius => ctx.radius,
        Expr::Theta => ctx.theta,
        Expr::Time => ctx.time * TIME_SCALE,
        Expr::Param(index) => ctx.params.get(*index).copied().unwrap_or(0.0),
        Expr::MoverDistance(index) => {
            let delta = ctx.mover_delta(*index);
            delta[0].hypot(delta[1])
        }
        Expr::MoverWindow(index) => {
            let delta = ctx.mover_delta(*index);
            ripple_window(delta[0].hypot(delta[1]), ctx.motion.wrap_limit())
        }
        Expr::Sin(a) => eval(a, ctx).sin(),
        Expr::Cos(a) => eval(a, ctx).cos(),
        Expr::Neg(a) => -eval(a, ctx),
        Expr::Add(a, b) => eval(a, ctx) + eval(b, ctx),
        Expr::Sub(a, b) => eval(a, ctx) - eval(b, ctx),
        Expr::Mul(a, b) => eval(a, ctx) * eval(b, ctx),
        Expr::Hypot(a, b) => eval(a, ctx).hypot(eval(b, ctx)),
    }
}

/// Field value at a point in normalized coordinates, where the short screen
/// axis runs from -1 to 1. The result is bounded to [-1, 1] so that unbounded
/// subtrees still land somewhere in the palette.
pub fn field_at(
    genome: &Genome,
    params: &[f32],
    motion: &Motion,
    x: f32,
    y: f32,
    time: f32,
) -> f32 {
    let (rippled_x, rippled_y) = ripple_point(&genome.warp, motion, x, y, time);

    let mut value = layer_value(
        genome.base_layer(),
        genome,
        params,
        motion,
        (rippled_x, rippled_y),
        (x, y),
        time,
    );

    // Layers paint in depth order, each showing through wherever its mask is
    // open and however opaque it is at this moment.
    for layer in genome.layers_above_base() {
        let alpha = layer.alpha_at(rippled_x, rippled_y, time);
        if alpha <= 0.0 {
            continue;
        }
        let above = layer_value(
            layer,
            genome,
            params,
            motion,
            (rippled_x, rippled_y),
            (x, y),
            time,
        );
        value += (above - value) * alpha;
    }
    value
}

/// One layer's field, folded by its own symmetry, turned by its own spin and
/// shifted to its own part of the palette.
pub fn layer_value(
    layer: &Layer,
    genome: &Genome,
    params: &[f32],
    motion: &Motion,
    rippled: (f32, f32),
    raw: (f32, f32),
    time: f32,
) -> f32 {
    let (folded_x, folded_y, _, _) =
        fold_spun(rippled.0, rippled.1, layer.symmetry, layer.spin * time);
    let (warped_x, warped_y) = wave_point(&genome.warp, folded_x, folded_y, time);
    let (warped_x, warped_y) = (warped_x * layer.detail, warped_y * layer.detail);
    let ctx = Context {
        x: warped_x,
        y: warped_y,
        radius: warped_x.hypot(warped_y),
        theta: warped_y.atan2(warped_x),
        time,
        params,
        raw_x: raw.0,
        raw_y: raw.1,
        motion,
    };
    bounded(eval(&layer.field, &ctx)) + layer.tone_offset
}

/// Rings spreading from every mover, applied before the symmetry fold so they
/// stay tied to where the movers are on screen.
pub fn ripple_point(warp: &Warp, motion: &Motion, x: f32, y: f32, time: f32) -> (f32, f32) {
    if warp.ripple_amplitude == 0.0 {
        return (x, y);
    }
    let strength = warp.strength_at(time);
    let wrap_limit = motion.wrap_limit();
    let mut rippled = (x, y);
    for index in 0..motion.movers.len() {
        let delta = motion.delta(index, x, y);
        let distance = delta[0].hypot(delta[1]);
        if distance <= 1e-4 {
            continue;
        }
        let push = warp.ripple_amplitude
            * strength
            * ripple_window(distance, wrap_limit)
            * (warp.ripple_frequency * distance - warp.ripple_speed * time).sin()
            / (1.0 + RIPPLE_DECAY * distance * distance);
        rippled.0 += push * delta[0] / distance;
        rippled.1 += push * delta[1] / distance;
    }
    rippled
}

/// Traveling waves along both axes, applied after the fold so the symmetry
/// survives and the whole surface moves as one sheet of water.
pub fn wave_point(warp: &Warp, x: f32, y: f32, time: f32) -> (f32, f32) {
    let amplitude = warp.amplitude * warp.strength_at(time);
    (
        x + amplitude * (warp.frequency * y + warp.speed * time).sin(),
        y + amplitude * (warp.frequency * x + warp.speed * time * 1.13).sin(),
    )
}

/// Rings reach zero before the distance at which a wrapping mover's nearest
/// copy changes. Any push left at that distance would draw a line there.
pub fn ripple_window(distance: f32, wrap_limit: f32) -> f32 {
    let ratio = (distance / wrap_limit.max(1e-4)).min(1.0);
    let falloff = 1.0 - ratio * ratio;
    falloff * falloff
}

/// How quickly a mover's rings die away with distance.
pub const RIPPLE_DECAY: f32 = 6.0;

/// Wrap an unbounded expression result into the range the palette covers.
fn bounded(value: f32) -> f32 {
    if value.is_finite() { value.sin() } else { 0.0 }
}

/// One layer's own values on a `steps` by `steps` grid, so a layer can be
/// judged without whatever is stacked over it.
pub fn sample_layer(
    genome: &Genome,
    layer: &Layer,
    motion: &Motion,
    time: f32,
    steps: usize,
) -> Vec<f32> {
    let params = genome.param_values(time);
    let mut values = Vec::with_capacity(steps * steps);
    for row in 0..steps {
        for column in 0..steps {
            let x = (column as f32 / (steps - 1) as f32) * 2.0 - 1.0;
            let y = (row as f32 / (steps - 1) as f32) * 2.0 - 1.0;
            let rippled = ripple_point(&genome.warp, motion, x, y, time);
            values.push(layer_value(
                layer,
                genome,
                &params,
                motion,
                rippled,
                (x, y),
                time,
            ));
        }
    }
    values
}

/// Field values on a `steps` by `steps` grid, used to judge whether a genome
/// produces a varied image.
pub fn sample_field(genome: &Genome, motion: &Motion, time: f32, steps: usize) -> Vec<f32> {
    let params = genome.param_values(time);
    let mut values = Vec::with_capacity(steps * steps);
    for row in 0..steps {
        for column in 0..steps {
            let x = (column as f32 / (steps - 1) as f32) * 2.0 - 1.0;
            let y = (row as f32 / (steps - 1) as f32) * 2.0 - 1.0;
            values.push(field_at(genome, &params, motion, x, y, time));
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Expr, Genome, PARAM_COUNT, TIME_SCALE, Warp, fallback_genome, seeded_rng};
    use std::sync::OnceLock;

    static MOTION: OnceLock<Motion> = OnceLock::new();

    fn context(params: &[f32]) -> Context<'_> {
        Context {
            x: 0.5,
            y: -0.25,
            radius: 2.0,
            theta: 1.0,
            time: 3.0,
            params,
            raw_x: 0.5,
            raw_y: -0.25,
            motion: MOTION.get_or_init(Motion::still),
        }
    }

    #[test]
    fn every_node_kind_evaluates() {
        let params = [7.0f32; PARAM_COUNT];
        let ctx = context(&params);
        assert_eq!(eval(&Expr::X, &ctx), 0.5);
        assert_eq!(eval(&Expr::Y, &ctx), -0.25);
        assert_eq!(eval(&Expr::Radius, &ctx), 2.0);
        assert_eq!(eval(&Expr::Theta, &ctx), 1.0);
        assert_eq!(eval(&Expr::Time, &ctx), 3.0 * TIME_SCALE);
        assert_eq!(eval(&Expr::Param(2), &ctx), 7.0);
        assert_eq!(eval(&Expr::Sin(Box::new(Expr::X)), &ctx), 0.5f32.sin());
        assert_eq!(eval(&Expr::Cos(Box::new(Expr::X)), &ctx), 0.5f32.cos());
        assert_eq!(eval(&Expr::Neg(Box::new(Expr::X)), &ctx), -0.5);
        let x = Box::new(Expr::X);
        let y = Box::new(Expr::Y);
        assert_eq!(eval(&Expr::Add(x.clone(), y.clone()), &ctx), 0.25);
        assert_eq!(eval(&Expr::Sub(x.clone(), y.clone()), &ctx), 0.75);
        assert_eq!(eval(&Expr::Mul(x.clone(), y.clone()), &ctx), -0.125);
        assert_eq!(eval(&Expr::Hypot(x, y), &ctx), 0.5f32.hypot(-0.25));
    }

    #[test]
    fn a_parameter_index_beyond_the_slots_reads_as_zero() {
        let params = [1.0f32; 2];
        assert_eq!(eval(&Expr::Param(9), &context(&params)), 0.0);
    }

    #[test]
    fn the_field_is_bounded_even_when_the_expression_is_not() {
        let mut genome = fallback_genome();
        genome.layers[0].field = Expr::Mul(Box::new(Expr::Param(0)), Box::new(Expr::Radius));
        let params = genome.param_values(0.0);
        let value = field_at(&genome, &params, &Motion::still(), 0.9, 0.9, 0.0);
        assert!((-1.0..=1.0).contains(&value));
    }

    #[test]
    fn a_field_that_evaluates_to_infinity_reads_as_zero() {
        let mut genome = fallback_genome();
        genome.layers[0].field = Expr::Param(0);
        genome.params[0].base = f32::INFINITY;
        genome.params[0].amplitude = 0.0;
        let params = genome.param_values(0.0);
        assert_eq!(
            field_at(&genome, &params, &Motion::still(), 0.1, 0.1, 0.0),
            0.0
        );
    }

    #[test]
    fn a_point_sitting_on_a_mover_takes_no_push_from_its_rings() {
        let warp = Warp {
            ripple_amplitude: 0.1,
            ripple_frequency: 10.0,
            ripple_speed: 1.0,
            ..Warp::none()
        };
        let motion = Motion::still();
        let center = motion.movers[0].position;
        let (x, y) = ripple_point(&warp, &motion, center[0], center[1], 2.0);
        assert!((x - center[0]).abs() < 0.2 && (y - center[1]).abs() < 0.2);
    }

    #[test]
    fn sampling_returns_one_value_per_grid_cell() {
        let genome = Genome::random(&mut seeded_rng(4));
        assert_eq!(sample_field(&genome, &Motion::still(), 0.0, 8).len(), 64);
    }
}
