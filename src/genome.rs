//! The genome: a randomly generated scalar field expression plus the drifting
//! parameters, symmetry and palette settings that make it evolve over time.

use crate::motion::Motion;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

/// Every genome carries the same number of parameter slots so that trees can be
/// crossed between two genomes without renumbering `Expr::Param` indices.
pub const PARAM_COUNT: usize = 24;

pub const MAX_SYMMETRY: u32 = 12;

/// The share of its strength a warp keeps when its swell is at its lowest.
pub const WARP_FLOOR: f32 = 0.25;

/// The clock the expressions see, in turns of the real one. A frequency
/// parameter multiplies whatever it is given, so an unscaled clock inside a
/// wave term sweeps several radians a second and the pattern shakes.
pub const TIME_SCALE: f32 = 0.03;

/// A scalar constant that oscillates slowly, which is what makes a still
/// pattern breathe and mutate rather than sit frozen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Param {
    pub base: f32,
    pub amplitude: f32,
    pub rate: f32,
    pub phase: f32,
}

impl Param {
    pub fn value_at(&self, time: f32) -> f32 {
        self.base + self.amplitude * (self.rate * time + self.phase).sin()
    }

    /// Slots have roles by index so that generated expressions get real
    /// frequency content instead of a screenful of one slow gradient.
    /// Low slots are spatial frequencies, middle slots are general constants,
    /// high slots are phases.
    pub fn random(rng: &mut impl RngExt, slot: usize) -> Self {
        let (base, amplitude, rate) = match slot {
            0..=7 => (
                rng.random_range(1.2..8.0) * if rng.random_bool(0.5) { -1.0 } else { 1.0 },
                rng.random_range(0.4..2.0),
                rng.random_range(0.001..0.0065),
            ),
            8..=15 => (
                rng.random_range(-3.0..3.0),
                rng.random_range(0.4..2.4),
                rng.random_range(0.0016..0.008),
            ),
            _ => (
                rng.random_range(0.0..std::f32::consts::TAU),
                rng.random_range(0.5..4.0),
                rng.random_range(0.0024..0.011),
            ),
        };
        Param {
            base,
            amplitude,
            rate,
            phase: rng.random_range(0.0..std::f32::consts::TAU),
        }
    }

    pub fn frequency_slot(rng: &mut impl RngExt) -> usize {
        rng.random_range(0..8)
    }

    pub fn phase_slot(rng: &mut impl RngExt) -> usize {
        rng.random_range(16..PARAM_COUNT)
    }

    /// A slot holding a general constant of moderate size.
    pub fn constant_slot(rng: &mut impl RngExt) -> usize {
        rng.random_range(8..16)
    }

    pub fn lobe_slot(rng: &mut impl RngExt) -> usize {
        rng.random_range(8..12)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    X,
    Y,
    Radius,
    Theta,
    Time,
    Param(usize),
    /// Distance from a mover, measured in unfolded screen coordinates.
    MoverDistance(usize),
    /// Falls to zero at the distance where a wrapping mover's nearest copy
    /// changes. Anything measured from a mover is multiplied by this, so the
    /// change of copy never shows as an edge.
    MoverWindow(usize),
    Sin(Box<Expr>),
    Cos(Box<Expr>),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Hypot(Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Number of nodes in the tree, counting this one.
    pub fn node_count(&self) -> usize {
        match self {
            Expr::X
            | Expr::Y
            | Expr::Radius
            | Expr::Theta
            | Expr::Time
            | Expr::Param(_)
            | Expr::MoverDistance(_)
            | Expr::MoverWindow(_) => 1,
            Expr::Sin(a) | Expr::Cos(a) | Expr::Neg(a) => 1 + a.node_count(),
            Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Hypot(a, b) => {
                1 + a.node_count() + b.node_count()
            }
        }
    }

    /// Depth-first lookup of the `index`th node, used by mutation and crossover
    /// to pick a subtree to replace.
    pub fn subtree_at_mut(&mut self, index: usize) -> Option<&mut Expr> {
        let mut remaining = index;
        self.walk_mut(&mut remaining)
    }

    fn walk_mut(&mut self, remaining: &mut usize) -> Option<&mut Expr> {
        if *remaining == 0 {
            return Some(self);
        }
        *remaining -= 1;
        let (left, right) = self.children_mut();
        if let Some(child) = left
            && let Some(found) = child.walk_mut(remaining)
        {
            return Some(found);
        }
        if let Some(child) = right
            && let Some(found) = child.walk_mut(remaining)
        {
            return Some(found);
        }
        None
    }

    fn children_mut(&mut self) -> (Option<&mut Expr>, Option<&mut Expr>) {
        match self {
            Expr::X
            | Expr::Y
            | Expr::Radius
            | Expr::Theta
            | Expr::Time
            | Expr::Param(_)
            | Expr::MoverDistance(_)
            | Expr::MoverWindow(_) => (None, None),
            Expr::Sin(a) | Expr::Cos(a) | Expr::Neg(a) => (Some(a), None),
            Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Hypot(a, b) => {
                (Some(a), Some(b))
            }
        }
    }

    /// Build a random tree. `depth` is the remaining budget; at zero only
    /// leaves are produced. Every node kind here is smooth, since a crease or
    /// a jump anywhere in the tree draws a visible line across the screen.
    pub fn random(rng: &mut impl RngExt, depth: u32) -> Expr {
        if depth == 0 {
            return Expr::random_leaf(rng);
        }
        match rng.random_range(0..12) {
            0 | 1 => Expr::random_leaf(rng),
            2..=4 => Expr::Sin(Box::new(Expr::random(rng, depth - 1))),
            5..=7 => Expr::Cos(Box::new(Expr::random(rng, depth - 1))),
            8 => Expr::Neg(Box::new(Expr::random(rng, depth - 1))),
            9 => Expr::Add(
                Box::new(Expr::random(rng, depth - 1)),
                Box::new(Expr::random(rng, depth - 1)),
            ),
            10 => Expr::Mul(
                Box::new(Expr::random(rng, depth - 1)),
                Box::new(Expr::random(rng, depth - 1)),
            ),
            _ => Expr::Hypot(
                Box::new(Expr::random(rng, depth - 1)),
                Box::new(Expr::random(rng, depth - 1)),
            ),
        }
    }

    fn random_leaf(rng: &mut impl RngExt) -> Expr {
        match rng.random_range(0..10) {
            0 | 1 => Expr::X,
            2 | 3 => Expr::Y,
            4 | 5 => Expr::Radius,
            6 | 7 => Expr::Theta,
            8 => Expr::Time,
            _ => Expr::Param(rng.random_range(8..16)),
        }
    }

    /// One oscillating term: a random subtree scaled by a frequency parameter
    /// and offset by a phase. Building the field out of these guarantees the
    /// fine banding the pattern needs, which a purely random tree rarely finds.
    pub fn random_wave(rng: &mut impl RngExt, depth: u32) -> Expr {
        let inner = Expr::random(rng, depth);
        let scaled = Expr::Mul(
            Box::new(Expr::Param(Param::frequency_slot(rng))),
            Box::new(inner),
        );
        let offset = Expr::Add(
            Box::new(scaled),
            Box::new(Expr::Param(Param::phase_slot(rng))),
        );
        if rng.random_bool(0.5) {
            Expr::Sin(Box::new(offset))
        } else {
            Expr::Cos(Box::new(offset))
        }
    }

    /// A field is a combination of two to four wave terms, some of which are
    /// tied to a mover so they travel across the screen.
    pub fn random_field(rng: &mut impl RngExt) -> Expr {
        let term_count = rng.random_range(2..=4);
        let mut field = Expr::random_wave(rng, 3);
        for _ in 1..term_count {
            let term = if rng.random_bool(0.3) {
                Expr::random_mover_wave(rng)
            } else {
                Expr::random_wave(rng, 3)
            };
            field = if rng.random_bool(0.6) {
                Expr::Add(Box::new(field), Box::new(term))
            } else {
                Expr::Mul(Box::new(field), Box::new(term))
            };
        }
        // A mid range multiplier: enough to fold the waves into each other,
        // not so much that the result aliases into noise.
        Expr::Mul(
            Box::new(Expr::Param(Param::constant_slot(rng))),
            Box::new(field),
        )
    }

    /// Rings spreading from one mover, optionally with arms. A term like this
    /// puts a traveling wave source in a layer, since the mover it is measured
    /// from crosses the screen.
    pub fn random_mover_wave(rng: &mut impl RngExt) -> Expr {
        let mover = rng.random_range(0..crate::motion::MOVER_COUNT);
        let rings = Expr::Mul(
            Box::new(Expr::Param(Param::frequency_slot(rng))),
            Box::new(Expr::MoverDistance(mover)),
        );
        let body = rings;
        let travelling = Expr::Sin(Box::new(Expr::Add(
            Box::new(body),
            Box::new(Expr::Mul(
                Box::new(Expr::Param(Param::phase_slot(rng))),
                Box::new(Expr::Time),
            )),
        )));
        Expr::Mul(Box::new(Expr::MoverWindow(mover)), Box::new(travelling))
    }
}

/// A slow oscillation between two states, used wherever something morphs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cycle {
    pub period: f32,
    pub phase: f32,
}

impl Cycle {
    pub fn random(rng: &mut impl RngExt, shortest: f32, longest: f32) -> Cycle {
        Cycle {
            period: rng.random_range(shortest..longest),
            phase: rng.random_range(0.0..std::f32::consts::TAU),
        }
    }

    /// Position in the cycle, 0 to 1, easing at both ends.
    pub fn value_at(&self, time: f32) -> f32 {
        0.5 + 0.5 * (std::f32::consts::TAU * time / self.period + self.phase).sin()
    }
}

/// Where a layer above the base shows through. The patches drift across the
/// screen, so one layer keeps opening onto the one under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mask {
    pub frequency: [f32; 2],
    pub drift: [f32; 2],
    /// How sharply the mask favors its open patches. One is a plain swell,
    /// higher values keep the layer hidden over more of the screen. The curve
    /// has no threshold and no clamp, so it never creases.
    pub bias: f32,
}

impl Mask {
    pub fn random(rng: &mut impl RngExt) -> Mask {
        Mask {
            frequency: [rng.random_range(0.5..2.5), rng.random_range(0.5..2.5)],
            drift: [rng.random_range(-0.12..0.12), rng.random_range(-0.12..0.12)],
            bias: rng.random_range(1.0..4.0),
        }
    }

    /// Covers everything, for the base layer and for tests.
    pub fn open() -> Mask {
        Mask {
            frequency: [0.0, 0.0],
            drift: [0.0, 0.0],
            bias: 0.0,
        }
    }

    pub fn coverage_at(&self, x: f32, y: f32, time: f32) -> f32 {
        let wave = 0.5
            * ((self.frequency[0] * x + self.drift[0] * time).sin()
                + (self.frequency[1] * y + self.drift[1] * time).sin());
        (0.5 + 0.5 * wave).powf(self.bias)
    }
}

/// Displacement applied to the sampling point before the field is read:
/// traveling waves along both axes, plus rings spreading from every mover.
/// Together they give the pattern its water surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Warp {
    /// Both displacements swell and subside on this cycle, so the water is
    /// never moving at one fixed strength.
    pub swell: Cycle,
    pub amplitude: f32,
    pub frequency: f32,
    pub speed: f32,
    pub ripple_amplitude: f32,
    pub ripple_frequency: f32,
    pub ripple_speed: f32,
}

impl Warp {
    pub fn random(rng: &mut impl RngExt) -> Warp {
        Warp {
            swell: Cycle::random(rng, 30.0, 130.0),
            amplitude: if rng.random_bool(0.75) {
                rng.random_range(0.01..0.12)
            } else {
                0.0
            },
            frequency: rng.random_range(1.0..6.0),
            speed: rng.random_range(0.05..0.22) * if rng.random_bool(0.5) { -1.0 } else { 1.0 },
            ripple_amplitude: if rng.random_bool(0.7) {
                rng.random_range(0.01..0.07)
            } else {
                0.0
            },
            ripple_frequency: rng.random_range(3.0..10.0),
            ripple_speed: rng.random_range(0.08..0.45),
        }
    }

    /// How much of its strength the warp keeps at the bottom of its swell.
    pub fn strength_at(&self, time: f32) -> f32 {
        WARP_FLOOR + (1.0 - WARP_FLOOR) * self.swell.value_at(time)
    }

    pub fn none() -> Warp {
        Warp {
            swell: Cycle {
                period: 20.0,
                phase: 0.0,
            },
            amplitude: 0.0,
            frequency: 1.0,
            speed: 0.0,
            ripple_amplitude: 0.0,
            ripple_frequency: 1.0,
            ripple_speed: 0.0,
        }
    }
}

/// One full screen field with its own symmetry, its own spin and an opacity
/// that swells and fades. Layers stack by depth, so which one dominates keeps
/// changing as their opacities cross.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub field: Expr,
    pub symmetry: u32,
    /// How fast this layer's fold turns, in radians per second.
    pub spin: f32,
    /// Scale applied to this layer's coordinates. Layers with different detail
    /// read as separate sheets rather than one pattern, since one carries fine
    /// structure where the other carries broad shapes.
    pub detail: f32,
    pub opacity: Cycle,
    /// How much of the layer still shows when its opacity is at its lowest.
    pub opacity_floor: f32,
    /// Where on screen the layer shows through at all.
    pub mask: Mask,
    /// Shift applied to this layer's value, which moves it to its own part of
    /// the palette so it reads as a separate sheet rather than a blend.
    pub tone_offset: f32,
    /// Paint order within the scene, low to high.
    pub depth: f32,
}

impl Layer {
    pub fn random(rng: &mut impl RngExt) -> Layer {
        Layer {
            field: Expr::random_field(rng),
            symmetry: rng.random_range(1..=MAX_SYMMETRY),
            spin: rng.random_range(-0.032..0.032),
            detail: rng.random_range(0.5..2.6),
            opacity: Cycle::random(rng, 30.0, 140.0),
            opacity_floor: rng.random_range(0.0..0.25),
            mask: Mask::random(rng),
            tone_offset: rng.random_range(-1.2..1.2),
            depth: rng.random_range(0.0..1.0),
        }
    }

    /// How much of this layer shows at a point: its opacity now, narrowed to
    /// wherever its mask is open.
    pub fn alpha_at(&self, x: f32, y: f32, time: f32) -> f32 {
        self.opacity_at(time) * self.mask.coverage_at(x, y, time)
    }

    pub fn opacity_at(&self, time: f32) -> f32 {
        self.opacity_floor + (1.0 - self.opacity_floor) * self.opacity.value_at(time)
    }
}

/// Most layers a genome can stack.
pub const MAX_LAYERS: usize = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct Genome {
    /// Stacked fields, lowest depth first. The first one is the base and is
    /// always fully opaque, since there is nothing under it to show through.
    pub layers: Vec<Layer>,
    pub warp: Warp,
    pub params: Vec<Param>,
    /// How many palette cycles one unit of field value spans.
    pub palette_scale: f32,
    /// How far the palette scale swings either side of that, which stretches
    /// and compresses the color banding as it runs.
    pub palette_swing: f32,
    pub palette_swing_cycle: Cycle,
    /// Palette rotation speed, in cycles per second.
    pub palette_rate: f32,
}

impl Genome {
    pub fn random(rng: &mut impl RngExt) -> Genome {
        let layer_count = rng.random_range(2..=MAX_LAYERS);
        let mut layers: Vec<Layer> = (0..layer_count).map(|_| Layer::random(rng)).collect();
        give_layers_distinct_symmetry(&mut layers, rng);
        sort_layers_by_depth(&mut layers);
        Genome {
            layers,
            warp: Warp::random(rng),
            params: (0..PARAM_COUNT)
                .map(|slot| Param::random(rng, slot))
                .collect(),
            palette_scale: rng.random_range(0.7..2.2),
            palette_swing: rng.random_range(0.0..0.35),
            palette_swing_cycle: Cycle::random(rng, 90.0, 240.0),
            palette_rate: rng.random_range(-0.013..0.013),
        }
    }

    /// The layers that paint over the base, in depth order.
    pub fn layers_above_base(&self) -> &[Layer] {
        &self.layers[1..]
    }

    /// The layer everything else paints over.
    pub fn base_layer(&self) -> &Layer {
        self.layers.first().expect("a genome always has a layer")
    }

    /// Palette scale at a moment, swinging around the genome's base scale.
    pub fn palette_scale_at(&self, time: f32) -> f32 {
        let swing = self.palette_swing * (self.palette_swing_cycle.value_at(time) * 2.0 - 1.0);
        (self.palette_scale + swing).max(0.2)
    }

    /// Current value of every parameter, ready to hand to the evaluator or to
    /// upload as a shader uniform.
    pub fn param_values(&self, time: f32) -> Vec<f32> {
        self.params.iter().map(|p| p.value_at(time)).collect()
    }

    pub fn mutate(&self, rng: &mut impl RngExt, strength: f32) -> Genome {
        let mut child = self.clone();
        for layer in &mut child.layers {
            if rng.random_bool(f64::from(strength).min(1.0)) {
                let index = rng.random_range(0..layer.field.node_count());
                if let Some(node) = layer.field.subtree_at_mut(index) {
                    *node = Expr::random_wave(rng, 2);
                }
            }
            if rng.random_bool(0.25) {
                layer.symmetry = rng.random_range(1..=MAX_SYMMETRY);
            }
            if rng.random_bool(0.35) {
                layer.spin = rng.random_range(-0.032..0.032);
            }
            if rng.random_bool(0.35) {
                layer.detail = rng.random_range(0.5..2.6);
            }
            if rng.random_bool(0.35) {
                layer.opacity = Cycle::random(rng, 30.0, 140.0);
                layer.opacity_floor = rng.random_range(0.0..0.25);
            }
            if rng.random_bool(0.35) {
                layer.mask = Mask::random(rng);
            }
            if rng.random_bool(0.35) {
                layer.tone_offset = rng.random_range(-1.2..1.2);
            }
            if rng.random_bool(0.3) {
                layer.depth = rng.random_range(0.0..1.0);
            }
        }
        if rng.random_bool(0.3) && child.layers.len() < MAX_LAYERS {
            child.layers.push(Layer::random(rng));
        } else if rng.random_bool(0.25) && child.layers.len() > 2 {
            child.layers.pop();
        }
        sort_layers_by_depth(&mut child.layers);
        for param in &mut child.params {
            if rng.random_bool(0.3) {
                param.base += rng.random_range(-1.0..1.0) * strength;
                param.rate = (param.rate + rng.random_range(-0.008..0.008) * strength).abs();
            }
        }
        child
    }

    /// Splice a subtree from `other` into a copy of `self`, and take the other
    /// genome's symmetry settings half the time.
    pub fn crossover(&self, other: &Genome, rng: &mut impl RngExt) -> Genome {
        let mut child = self.clone();
        for (index, layer) in child.layers.iter_mut().enumerate() {
            let Some(partner) = other.layers.get(index) else {
                continue;
            };
            if rng.random_bool(0.5) {
                *layer = partner.clone();
                continue;
            }
            let mut donor = partner.field.clone();
            let graft = donor
                .subtree_at_mut(rng.random_range(0..partner.field.node_count()))
                .expect("index is within the donor tree")
                .clone();
            let target = rng.random_range(0..layer.field.node_count());
            if let Some(node) = layer.field.subtree_at_mut(target) {
                *node = graft;
            }
        }
        sort_layers_by_depth(&mut child.layers);
        for (slot, source) in child.params.iter_mut().zip(other.params.iter()) {
            if rng.random_bool(0.5) {
                *slot = *source;
            }
        }
        if rng.random_bool(0.5) {
            child.warp = other.warp;
        }
        child
    }
}

/// Two layers folded the same way read as one pattern, so every layer is given
/// a symmetry no other layer in the stack has.
pub fn give_layers_distinct_symmetry(layers: &mut [Layer], rng: &mut impl RngExt) {
    let mut taken: Vec<u32> = Vec::with_capacity(layers.len());
    for layer in layers.iter_mut() {
        for _ in 0..16 {
            if !taken.contains(&layer.symmetry) {
                break;
            }
            layer.symmetry = rng.random_range(1..=MAX_SYMMETRY);
        }
        taken.push(layer.symmetry);
    }
}

/// Paint order runs low depth to high, so a layer with greater depth paints
/// over one with less.
pub fn sort_layers_by_depth(layers: &mut [Layer]) {
    layers.sort_by(|left, right| {
        left.depth
            .partial_cmp(&right.depth)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Fold a point into one wedge of an `n`-fold rotationally symmetric plane.
pub fn fold(x: f32, y: f32, symmetry: u32) -> (f32, f32, f32, f32) {
    fold_spun(x, y, symmetry, 0.0)
}

/// Fold with the whole pattern turned by `spin` radians, which is what makes a
/// mandala rotate rather than sit still.
pub fn fold_spun(x: f32, y: f32, symmetry: u32, spin: f32) -> (f32, f32, f32, f32) {
    let radius = x.hypot(y);
    let segment = std::f32::consts::TAU / symmetry.max(1) as f32;
    let angle = y.atan2(x) + spin;
    // The wedge is always mirrored. Wrapping the angle without reflecting it
    // leaves a seam at every wedge edge, where the folded angle jumps from one
    // end of its range to the other.
    let wrapped = angle.rem_euclid(segment);
    let folded = if wrapped > segment * 0.5 {
        segment - wrapped
    } else {
        wrapped
    };
    (radius * folded.cos(), radius * folded.sin(), radius, folded)
}

/// Reject three failure modes of random generation: a field that barely varies
/// across the screen, one whose frequency is so high that neighboring pixels
/// are unrelated, and one that runs through the palette so many times across
/// the screen that it reads as stripes.
pub fn is_degenerate(genome: &Genome, motion: &Motion, time: f32) -> bool {
    if stats_are_poor(field_stats(genome, motion, time)) {
        return true;
    }
    // A layer is judged on its own as well: a flat sheet stacked over a good
    // pattern leaves a dead area of screen wherever its mask is open.
    genome
        .layers
        .iter()
        .any(|layer| stats_are_poor(layer_stats(genome, layer, motion, time)))
}

fn stats_are_poor(stats: FieldStats) -> bool {
    stats.range < 0.5 || stats.jump_fraction > 0.12 || !(0.025..=0.14).contains(&stats.stripe_index)
}

#[derive(Clone, Copy, Debug)]
pub struct FieldStats {
    /// Spread of field values across the sampled grid.
    pub range: f32,
    /// Share of neighboring samples that differ by more than half the range,
    /// which is high when the pattern is finer than the screen can resolve.
    pub jump_fraction: f32,
    /// Average step between neighboring samples as a share of the range. High
    /// values mean the field crosses the whole palette over and over, which
    /// renders as dense striping. Low values mean there is barely a pattern to
    /// look at.
    pub stripe_index: f32,
}

pub fn field_stats(genome: &Genome, motion: &Motion, time: f32) -> FieldStats {
    stats_of(&crate::eval::sample_field(genome, motion, time, STATS_GRID))
}

/// The same measures taken over one layer on its own.
pub fn layer_stats(genome: &Genome, layer: &Layer, motion: &Motion, time: f32) -> FieldStats {
    stats_of(&crate::eval::sample_layer(
        genome, layer, motion, time, STATS_GRID,
    ))
}

/// How finely a field is sampled when it is being judged.
const STATS_GRID: usize = 64;

fn stats_of(values: &[f32]) -> FieldStats {
    const GRID: usize = STATS_GRID;
    let low = values.iter().cloned().fold(f32::INFINITY, f32::min);
    let high = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let range = high - low;

    let jump_threshold = range * 0.5;
    let mut jumps = 0usize;
    let mut pairs = 0usize;
    let mut total_step = 0.0f32;
    for row in 0..GRID {
        for column in 1..GRID {
            let delta = (values[row * GRID + column] - values[row * GRID + column - 1]).abs();
            if delta > jump_threshold {
                jumps += 1;
            }
            total_step += delta;
            pairs += 1;
        }
    }
    FieldStats {
        range,
        jump_fraction: jumps as f32 / pairs as f32,
        stripe_index: total_step / pairs as f32 / range.max(1e-4),
    }
}

/// Draw random genomes until one of them varies across the screen.
pub fn random_interesting(rng: &mut impl RngExt, motion: &Motion) -> Genome {
    random_interesting_at(rng, motion, 0.0)
}

/// Judge candidates at the time they will first appear, since a genome's
/// parameters drift and a pattern that reads well at zero may not later.
pub fn random_interesting_at(rng: &mut impl RngExt, motion: &Motion, time: f32) -> Genome {
    random_interesting_within(rng, motion, time, 64)
}

/// Give up after `attempts` draws and fall back, so generation cannot spin.
pub fn random_interesting_within(
    rng: &mut impl RngExt,
    motion: &Motion,
    time: f32,
    attempts: u32,
) -> Genome {
    for _ in 0..attempts {
        let genome = Genome::random(rng);
        if !is_degenerate(&genome, motion, time) {
            return genome;
        }
    }
    fallback_genome()
}

/// Used when random generation fails to find a varied field, which keeps the
/// screen showing something rather than a flat color.
pub fn fallback_genome() -> Genome {
    Genome {
        layers: vec![Layer {
            field: Expr::Sin(Box::new(Expr::Mul(
                Box::new(Expr::Radius),
                Box::new(Expr::Param(0)),
            ))),
            symmetry: 6,
            spin: 0.0,
            detail: 1.0,
            opacity: Cycle {
                period: 20.0,
                phase: 0.0,
            },
            opacity_floor: 1.0,
            mask: Mask::open(),
            tone_offset: 0.0,
            depth: 0.0,
        }],
        params: (0..PARAM_COUNT)
            .map(|index| Param {
                base: 6.0 + index as f32,
                amplitude: 0.5,
                rate: 0.05,
                phase: index as f32,
            })
            .collect(),
        warp: Warp::none(),
        palette_scale: 1.0,
        palette_swing: 0.0,
        palette_swing_cycle: Cycle {
            period: 20.0,
            phase: 0.0,
        },
        palette_rate: 0.05,
    }
}

/// Seedable generator so a genome can be reproduced from a seed alone.
pub fn seeded_rng(seed: u64) -> StdRng {
    StdRng::seed_from_u64(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parameter_oscillates_around_its_base_value() {
        let param = Param {
            base: 2.0,
            amplitude: 0.5,
            rate: 1.0,
            phase: 0.0,
        };
        assert_eq!(param.value_at(0.0), 2.0);
        assert!((param.value_at(std::f32::consts::FRAC_PI_2) - 2.5).abs() < 1e-5);
    }

    #[test]
    fn random_parameters_take_their_range_from_their_slot() {
        let mut rng = seeded_rng(7);
        let frequency = Param::random(&mut rng, 0);
        let phase = Param::random(&mut rng, 20);
        assert!(frequency.base.abs() >= 2.0);
        assert!((0.0..=std::f32::consts::TAU).contains(&phase.base));
    }

    #[test]
    fn slot_pickers_stay_inside_their_ranges() {
        let mut rng = seeded_rng(11);
        for _ in 0..50 {
            assert!(Param::frequency_slot(&mut rng) < 8);
            assert!((16..PARAM_COUNT).contains(&Param::phase_slot(&mut rng)));
        }
    }

    #[test]
    fn node_count_covers_every_node_in_the_tree() {
        let tree = Expr::Add(
            Box::new(Expr::Sin(Box::new(Expr::X))),
            Box::new(Expr::Param(3)),
        );
        assert_eq!(tree.node_count(), 4);
    }

    #[test]
    fn every_node_is_reachable_by_index() {
        let mut rng = seeded_rng(3);
        let mut tree = Expr::random_field(&mut rng);
        let count = tree.node_count();
        for index in 0..count {
            assert!(tree.subtree_at_mut(index).is_some(), "node {index} missing");
        }
        assert!(tree.subtree_at_mut(count).is_none());
    }

    #[test]
    fn a_random_field_uses_at_least_one_frequency_parameter() {
        let mut rng = seeded_rng(5);
        let field = Expr::random_field(&mut rng);
        assert!(msl_uses_low_parameter(&field));
    }

    fn msl_uses_low_parameter(field: &Expr) -> bool {
        let source = crate::msl::emit(field);
        (0..8).any(|slot| source.contains(&format!("p[{slot}]")))
    }

    #[test]
    fn a_cycle_runs_from_one_end_to_the_other_and_back() {
        let cycle = Cycle {
            period: 8.0,
            phase: 0.0,
        };
        assert!((cycle.value_at(0.0) - 0.5).abs() < 1e-6);
        assert!((cycle.value_at(2.0) - 1.0).abs() < 1e-6);
        assert!((cycle.value_at(6.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn a_random_cycle_stays_inside_the_period_it_was_given() {
        let mut rng = seeded_rng(47);
        for _ in 0..30 {
            let cycle = Cycle::random(&mut rng, 5.0, 12.0);
            assert!((5.0..12.0).contains(&cycle.period));
        }
    }

    #[test]
    fn the_palette_scale_swings_around_the_genome_setting() {
        let mut genome = fallback_genome();
        genome.palette_scale = 2.0;
        genome.palette_swing = 0.5;
        genome.palette_swing_cycle = Cycle {
            period: 8.0,
            phase: 0.0,
        };
        assert!((genome.palette_scale_at(2.0) - 2.5).abs() < 1e-5);
        assert!((genome.palette_scale_at(6.0) - 1.5).abs() < 1e-5);
    }

    #[test]
    fn the_palette_scale_never_reaches_zero() {
        let mut genome = fallback_genome();
        genome.palette_scale = 0.3;
        genome.palette_swing = 5.0;
        assert!(genome.palette_scale_at(1.0) >= 0.2);
    }

    #[test]
    fn a_warp_swells_between_its_floor_and_its_full_strength() {
        let mut warp = Warp::none();
        warp.swell = Cycle {
            period: 8.0,
            phase: 0.0,
        };
        assert!((warp.strength_at(2.0) - 1.0).abs() < 1e-6);
        assert!((warp.strength_at(6.0) - WARP_FLOOR).abs() < 1e-6);
    }

    #[test]
    fn spinning_turns_the_whole_fold() {
        let quarter = std::f32::consts::FRAC_PI_2;
        let (_, _, _, still) = fold_spun(1.0, 0.0, 4, 0.0);
        let (_, _, _, turned) = fold_spun(1.0, 0.0, 4, quarter * 0.5);
        assert!((turned - still - quarter * 0.5).abs() < 1e-5);
    }

    #[test]
    fn spinning_a_pattern_with_no_symmetry_keeps_the_angle_in_one_turn() {
        let (_, _, _, angle) = fold_spun(1.0, 0.0, 1, -std::f32::consts::PI);
        assert!((0.0..std::f32::consts::TAU).contains(&angle));
    }

    #[test]
    fn a_warp_that_is_turned_off_moves_nothing() {
        let warp = Warp::none();
        assert_eq!(crate::eval::wave_point(&warp, 0.3, -0.2, 5.0), (0.3, -0.2));
        assert_eq!(
            crate::eval::ripple_point(&warp, &Motion::still(), 0.3, -0.2, 5.0),
            (0.3, -0.2)
        );
    }

    #[test]
    fn random_warps_sometimes_carry_waves_and_sometimes_ripples() {
        let mut rng = seeded_rng(37);
        let warps: Vec<Warp> = (0..40).map(|_| Warp::random(&mut rng)).collect();
        assert!(warps.iter().any(|warp| warp.amplitude > 0.0));
        assert!(warps.iter().any(|warp| warp.amplitude == 0.0));
        assert!(warps.iter().any(|warp| warp.ripple_amplitude > 0.0));
        assert!(warps.iter().any(|warp| warp.ripple_amplitude == 0.0));
    }

    #[test]
    fn a_mask_opens_where_its_wave_swells() {
        let mask = Mask {
            frequency: [1.0, 1.0],
            drift: [0.0, 0.0],
            bias: 1.0,
        };
        let quarter = std::f32::consts::FRAC_PI_2;
        let closed = mask.coverage_at(-quarter, -quarter, 0.0);
        let open = mask.coverage_at(quarter, quarter, 0.0);
        assert!(closed < 1e-6, "closed was {closed}");
        assert!((open - 1.0).abs() < 1e-6, "open was {open}");
    }

    #[test]
    fn a_biased_mask_keeps_more_of_the_screen_closed() {
        let plain = Mask {
            frequency: [1.0, 1.0],
            drift: [0.0, 0.0],
            bias: 1.0,
        };
        let biased = Mask { bias: 3.0, ..plain };
        assert!(biased.coverage_at(0.4, 0.2, 0.0) < plain.coverage_at(0.4, 0.2, 0.0));
    }

    #[test]
    fn an_open_mask_covers_the_whole_screen() {
        let mask = Mask::open();
        for x in [-1.0, -0.3, 0.4, 1.0] {
            for y in [-1.0, 0.0, 1.0] {
                assert_eq!(mask.coverage_at(x, y, 3.0), 1.0);
            }
        }
    }

    #[test]
    fn a_mask_drifts_across_the_screen_over_time() {
        let mut mask = Mask::random(&mut seeded_rng(131));
        mask.drift = [1.0, 0.0];
        let early = mask.coverage_at(0.2, 0.1, 0.0);
        let later = mask.coverage_at(0.2, 0.1, 2.0);
        assert_ne!(early, later);
    }

    #[test]
    fn a_layer_shows_only_where_its_mask_is_open() {
        let mut layer = Layer::random(&mut seeded_rng(101));
        layer.opacity_floor = 1.0;
        layer.mask = Mask {
            frequency: [1.0, 1.0],
            drift: [0.0, 0.0],
            bias: 1.0,
        };
        let quarter = std::f32::consts::FRAC_PI_2;
        assert!(layer.alpha_at(-quarter, -quarter, 0.0) < 1e-6);
        assert!(layer.alpha_at(quarter, quarter, 0.0) > 0.9);
    }

    #[test]
    fn no_two_layers_are_folded_the_same_way() {
        let mut rng = seeded_rng(103);
        for _ in 0..20 {
            let genome = Genome::random(&mut rng);
            let mut symmetries: Vec<u32> =
                genome.layers.iter().map(|layer| layer.symmetry).collect();
            let count = symmetries.len();
            symmetries.sort_unstable();
            symmetries.dedup();
            assert_eq!(symmetries.len(), count);
        }
    }

    #[test]
    fn a_flat_layer_is_thrown_out_even_when_the_stack_looks_fine() {
        let motion = Motion::still();
        let mut rng = seeded_rng(107);
        let mut genome = random_interesting(&mut rng, &motion);
        assert!(!is_degenerate(&genome, &motion, 0.0));
        genome.layers[1].field = Expr::Param(8);
        assert!(is_degenerate(&genome, &motion, 0.0));
    }

    #[test]
    fn every_term_measured_from_a_mover_fades_out_before_the_wrap_distance() {
        let mut rng = seeded_rng(137);
        for _ in 0..40 {
            let wave = Expr::random_mover_wave(&mut rng);
            let emitted = crate::msl::emit(&wave);
            assert!(
                emitted.contains("moverDistance"),
                "no mover term: {emitted}"
            );
            assert!(
                emitted.contains("moverWindow"),
                "unwindowed mover term: {emitted}"
            );
        }
    }

    #[test]
    fn the_ripple_window_closes_smoothly_at_the_wrap_distance() {
        assert_eq!(crate::eval::ripple_window(0.0, 1.0), 1.0);
        assert_eq!(crate::eval::ripple_window(1.0, 1.0), 0.0);
        assert_eq!(crate::eval::ripple_window(4.0, 1.0), 0.0);
        let near_edge = crate::eval::ripple_window(0.95, 1.0);
        assert!(
            near_edge < 0.02,
            "still {near_edge} of the ring at the edge"
        );
    }

    #[test]
    fn a_layer_carries_its_own_detail_scale() {
        let mut rng = seeded_rng(109);
        for _ in 0..20 {
            let layer = Layer::random(&mut rng);
            assert!((0.5..2.6).contains(&layer.detail));
        }
    }

    #[test]
    fn some_field_terms_are_tied_to_a_mover() {
        let mut rng = seeded_rng(113);
        let uses_mover = (0..40)
            .any(|_| crate::msl::emit(&Expr::random_field(&mut rng)).contains("moverDistance"));
        assert!(uses_mover);
    }

    #[test]
    fn a_layer_opacity_swells_between_its_floor_and_full() {
        let mut layer = Layer::random(&mut seeded_rng(59));
        layer.opacity_floor = 0.2;
        layer.opacity = Cycle {
            period: 8.0,
            phase: 0.0,
        };
        assert!((layer.opacity_at(2.0) - 1.0).abs() < 1e-6);
        assert!((layer.opacity_at(6.0) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_genome_stacks_at_least_two_layers() {
        let mut rng = seeded_rng(61);
        for _ in 0..20 {
            let genome = Genome::random(&mut rng);
            assert!((2..=MAX_LAYERS).contains(&genome.layers.len()));
        }
    }

    #[test]
    fn layers_are_stored_in_paint_order() {
        let genome = Genome::random(&mut seeded_rng(63));
        assert!(
            genome
                .layers
                .windows(2)
                .all(|pair| pair[0].depth <= pair[1].depth)
        );
    }

    #[test]
    fn the_base_layer_is_the_one_everything_paints_over() {
        let genome = Genome::random(&mut seeded_rng(67));
        assert_eq!(genome.base_layer(), &genome.layers[0]);
    }

    #[test]
    fn sorting_layers_puts_the_shallowest_first() {
        let mut rng = seeded_rng(79);
        let mut layers: Vec<Layer> = (0..3).map(|_| Layer::random(&mut rng)).collect();
        layers[0].depth = 0.8;
        layers[1].depth = 0.2;
        layers[2].depth = 0.5;
        sort_layers_by_depth(&mut layers);
        assert_eq!(
            layers.iter().map(|layer| layer.depth).collect::<Vec<_>>(),
            vec![0.2, 0.5, 0.8]
        );
    }

    #[test]
    fn mutation_keeps_the_layer_count_inside_its_range() {
        let mut rng = seeded_rng(83);
        let mut genome = Genome::random(&mut rng);
        for _ in 0..40 {
            genome = genome.mutate(&mut rng, 1.0);
            assert!((2..=MAX_LAYERS).contains(&genome.layers.len()));
        }
    }

    #[test]
    fn a_crossed_genome_keeps_its_layers_in_paint_order() {
        let mut rng = seeded_rng(97);
        let first = Genome::random(&mut rng);
        let second = Genome::random(&mut rng);
        let child = first.crossover(&second, &mut rng);
        assert!(
            child
                .layers
                .windows(2)
                .all(|pair| pair[0].depth <= pair[1].depth)
        );
    }

    #[test]
    fn a_mutated_genome_differs_from_its_parent() {
        let mut rng = seeded_rng(9);
        let parent = Genome::random(&mut rng);
        let child = parent.mutate(&mut rng, 1.0);
        assert_ne!(parent, child);
    }

    #[test]
    fn a_crossed_genome_keeps_the_parameter_slot_count() {
        let mut rng = seeded_rng(13);
        let first = Genome::random(&mut rng);
        let second = Genome::random(&mut rng);
        let child = first.crossover(&second, &mut rng);
        assert_eq!(child.params.len(), PARAM_COUNT);
    }

    #[test]
    fn folding_maps_a_point_into_one_wedge() {
        let (_, _, radius, theta) = fold(-1.0, -1.0, 6);
        let segment = std::f32::consts::TAU / 6.0;
        assert!((radius - 2.0f32.sqrt()).abs() < 1e-5);
        assert!((0.0..segment).contains(&theta));
    }

    #[test]
    fn folding_a_pattern_with_no_symmetry_reflects_it_about_one_axis() {
        let (x, y, _, theta) = fold(-1.0, 0.0, 1);
        assert!((theta - std::f32::consts::PI).abs() < 1e-5);
        assert!((x + 1.0).abs() < 1e-5);
        assert!(y.abs() < 1e-5);
    }

    #[test]
    fn mirroring_reflects_the_far_half_of_a_wedge() {
        let segment = std::f32::consts::TAU / 4.0;
        let angle = segment * 0.9;
        let (_, _, _, theta) = fold(angle.cos(), angle.sin(), 4);
        assert!((theta - segment * 0.1).abs() < 1e-5);
    }

    #[test]
    fn a_flat_field_counts_as_degenerate() {
        let mut genome = fallback_genome();
        genome.layers[0].field = Expr::Param(8);
        assert!(is_degenerate(&genome, &Motion::still(), 0.0));
    }

    #[test]
    fn a_field_that_is_noise_at_pixel_scale_counts_as_degenerate() {
        let mut genome = fallback_genome();
        genome.layers[0].field = Expr::Mul(Box::new(Expr::X), Box::new(Expr::Param(0)));
        genome.params[0].base = 4000.0;
        genome.params[0].amplitude = 0.0;
        genome.layers[0].symmetry = 1;
        assert!(field_stats(&genome, &Motion::still(), 0.0).jump_fraction > 0.2);
        assert!(is_degenerate(&genome, &Motion::still(), 0.0));
    }

    #[test]
    fn a_field_that_runs_through_the_palette_over_and_over_counts_as_degenerate() {
        let mut genome = fallback_genome();
        genome.layers[0].symmetry = 1;
        genome.layers[0].field = Expr::Mul(Box::new(Expr::Param(0)), Box::new(Expr::X));
        genome.params[0].base = 20.0;
        genome.params[0].amplitude = 0.0;
        let stats = field_stats(&genome, &Motion::still(), 0.0);
        assert!(
            stats.stripe_index > 0.14,
            "stripe index {}",
            stats.stripe_index
        );
        assert!(is_degenerate(&genome, &Motion::still(), 0.0));
    }

    #[test]
    fn a_broad_field_passes_the_striping_check() {
        let mut genome = fallback_genome();
        genome.layers[0].symmetry = 1;
        genome.layers[0].field = Expr::Mul(Box::new(Expr::Param(0)), Box::new(Expr::X));
        genome.params[0].base = 5.0;
        genome.params[0].amplitude = 0.0;
        let stats = field_stats(&genome, &Motion::still(), 0.0);
        assert!((0.025..=0.14).contains(&stats.stripe_index));
        assert!(!is_degenerate(&genome, &Motion::still(), 0.0));
    }

    #[test]
    fn a_field_with_an_infinite_value_counts_as_degenerate() {
        let mut genome = fallback_genome();
        genome.layers[0].field = Expr::Mul(Box::new(Expr::Param(8)), Box::new(Expr::Radius));
        genome.params[8].base = f32::INFINITY;
        genome.params[8].amplitude = 0.0;
        assert_eq!(field_stats(&genome, &Motion::still(), 0.0).range, 0.0);
        assert!(is_degenerate(&genome, &Motion::still(), 0.0));
    }

    #[test]
    fn generation_falls_back_when_it_runs_out_of_attempts() {
        let mut rng = seeded_rng(31);
        assert_eq!(
            random_interesting_within(&mut rng, &Motion::still(), 0.0, 0),
            fallback_genome()
        );
    }

    #[test]
    fn the_fallback_genome_is_not_degenerate() {
        assert!(!is_degenerate(&fallback_genome(), &Motion::still(), 0.0));
    }

    #[test]
    fn generation_keeps_drawing_until_a_genome_varies() {
        let mut rng = seeded_rng(21);
        for _ in 0..20 {
            let genome = random_interesting(&mut rng, &Motion::still());
            assert!(!is_degenerate(&genome, &Motion::still(), 0.0));
        }
    }

    #[test]
    fn a_seeded_generator_repeats_itself() {
        let first = Genome::random(&mut seeded_rng(42));
        let second = Genome::random(&mut seeded_rng(42));
        assert_eq!(first, second);
    }
}
