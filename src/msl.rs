//! Metal Shading Language generation. Each genome becomes a fragment shader,
//! because a fixed shader cannot express an arbitrary expression tree and a CPU
//! loop cannot fill a 5K screen at 60fps.

use crate::eval::RIPPLE_DECAY;
use crate::genome::{Expr, Genome, Layer, PARAM_COUNT, TIME_SCALE, WARP_FLOOR};
use crate::motion::MOVER_COUNT;

/// Emit the genome's field expression as an MSL expression over `x`, `y`,
/// `radius`, `theta`, `time` and the parameter array `p`.
pub fn emit(expr: &Expr) -> String {
    match expr {
        Expr::X => "x".to_string(),
        Expr::Y => "y".to_string(),
        Expr::Radius => "radius".to_string(),
        Expr::Theta => "theta".to_string(),
        Expr::Time => format!("(time * {TIME_SCALE:?})"),
        Expr::Param(index) => format!("p[{}]", index % PARAM_COUNT),
        Expr::MoverDistance(index) => format!("moverDistance{}", index % MOVER_COUNT),
        Expr::MoverWindow(index) => format!("moverWindow{}", index % MOVER_COUNT),
        Expr::Sin(a) => format!("sin({})", emit(a)),
        Expr::Cos(a) => format!("cos({})", emit(a)),
        Expr::Neg(a) => format!("(-{})", emit(a)),
        Expr::Add(a, b) => format!("({} + {})", emit(a), emit(b)),
        Expr::Sub(a, b) => format!("({} - {})", emit(a), emit(b)),
        Expr::Mul(a, b) => format!("({} * {})", emit(a), emit(b)),
        Expr::Hypot(a, b) => format!("length(float2({}, {}))", emit(a), emit(b)),
    }
}

/// Displacement of the sampling point, then each mover's offset from the
/// displaced point. A wrapping mover is measured to its nearest copy so its
/// shape stays whole as it crosses an edge.
fn ripple_prologue(genome: &Genome) -> String {
    let warp = &genome.warp;
    let mut lines = vec![
        "    float2 ripplePoint = rawPoint;".to_string(),
        format!(
            "    float warpStrength = {floor:?} + {span:?} * (0.5 + 0.5 * sin(6.2831855 * time / {period:?} + {phase:?}));",
            floor = WARP_FLOOR,
            span = 1.0 - WARP_FLOOR,
            period = warp.swell.period,
            phase = warp.swell.phase,
        ),
    ];
    if warp.ripple_amplitude != 0.0 {
        for index in 0..MOVER_COUNT {
            lines.push(format!(
                "    {{\n\
                 \x20       float2 rippleDelta = psyWrapDelta(rawPoint - u.movers[{index}].position, u.extent, u.movers[{index}].wraps);\n\
                 \x20       float rippleDistance = length(rippleDelta);\n\
                 \x20       if (rippleDistance > 1e-4) {{\n\
                 \x20           float rippleRatio = min(rippleDistance / max(min(u.extent.x, u.extent.y), 1e-4), 1.0);\n\
                 \x20           float rippleWindow = (1.0 - rippleRatio * rippleRatio);\n\
                 \x20           rippleWindow = rippleWindow * rippleWindow;\n\
                 \x20           float push = {amplitude:?} * warpStrength * rippleWindow * sin({frequency:?} * rippleDistance - {speed:?} * time) / (1.0 + {decay:?} * rippleDistance * rippleDistance);\n\
                 \x20           ripplePoint += push * rippleDelta / rippleDistance;\n\
                 \x20       }}\n\
                 \x20   }}",
                amplitude = warp.ripple_amplitude,
                frequency = warp.ripple_frequency,
                speed = warp.ripple_speed,
                decay = RIPPLE_DECAY,
            ));
        }
    }
    for index in 0..MOVER_COUNT {
        lines.push(format!(
            "    float2 moverDelta{index} = psyWrapDelta(rawPoint - u.movers[{index}].position, u.extent, u.movers[{index}].wraps);\n\
             \x20   float moverDistance{index} = length(moverDelta{index});\n\
             \x20   float moverRatio{index} = min(moverDistance{index} / max(min(u.extent.x, u.extent.y), 1e-4), 1.0);\n\
             \x20   float moverWindow{index} = (1.0 - moverRatio{index} * moverRatio{index});\n\
             \x20   moverWindow{index} = moverWindow{index} * moverWindow{index};"
        ));
    }
    lines.join("\n")
}

/// The fold for one layer, with its symmetry and spin written in. The wedge is
/// always mirrored, since wrapping the angle without reflecting it leaves a
/// seam at every wedge edge.
fn fold_block(layer: &Layer) -> String {
    let segment = std::f32::consts::TAU / layer.symmetry.max(1) as f32;
    format!(
        "        float theta = atan2(y, x) + {spin:?} * time;\n\
         \x20       theta = theta - {segment:?} * floor(theta / {segment:?});\n\
         \x20       if (theta > {half:?}) {{\n\
         \x20           theta = {segment:?} - theta;\n\
         \x20       }}",
        spin = layer.spin,
        segment = segment,
        half = segment * 0.5,
    )
}

/// One layer: its own fold, the traveling waves, then its field.
fn layer_block(layer: &Layer, genome: &Genome, is_base: bool) -> String {
    let warp = &genome.warp;
    let blend = if is_base {
        "        value = layerValue;".to_string()
    } else {
        format!(
            "        float opacity = {floor:?} + {span:?} * (0.5 + 0.5 * sin(6.2831855 * time / {period:?} + {phase:?}));\n\
             \x20       float maskWave = 0.5 * (sin({mask_fx:?} * ripplePoint.x + {mask_dx:?} * time) + sin({mask_fy:?} * ripplePoint.y + {mask_dy:?} * time));\n\
             \x20       float alpha = opacity * pow(0.5 + 0.5 * maskWave, {bias:?});\n\
             \x20       value += (layerValue - value) * alpha;",
            floor = layer.opacity_floor,
            span = 1.0 - layer.opacity_floor,
            period = layer.opacity.period,
            phase = layer.opacity.phase,
            mask_fx = layer.mask.frequency[0],
            mask_dx = layer.mask.drift[0],
            mask_fy = layer.mask.frequency[1],
            mask_dy = layer.mask.drift[1],
            bias = layer.mask.bias,
        )
    };
    format!(
        "    {{\n\
         \x20       float x = ripplePoint.x;\n\
         \x20       float y = ripplePoint.y;\n\
         \x20       float radius = length(ripplePoint);\n\
         {fold}\n\
         \x20       x = radius * cos(theta);\n\
         \x20       y = radius * sin(theta);\n\
         \x20       float waveAmplitude = {amplitude:?} * warpStrength;\n\
         \x20       float waveX = x + waveAmplitude * sin({frequency:?} * y + {speed:?} * time);\n\
         \x20       float waveY = y + waveAmplitude * sin({frequency:?} * x + {speed:?} * 1.13 * time);\n\
         \x20       x = waveX * {detail:?};\n\
         \x20       y = waveY * {detail:?};\n\
         \x20       radius = length(float2(x, y));\n\
         \x20       theta = atan2(y, x);\n\
         \x20       float layerValue = {field};\n\
         \x20       if (!isfinite(layerValue)) {{\n\
         \x20           layerValue = 0.0;\n\
         \x20       }}\n\
         \x20       layerValue = sin(layerValue) + {tone:?};\n\
         {blend}\n\
         \x20   }}",
        fold = fold_block(layer),
        amplitude = warp.amplitude,
        frequency = warp.frequency,
        speed = warp.speed,
        field = emit(&layer.field),
        detail = layer.detail,
        tone = layer.tone_offset,
        blend = blend,
    )
}

/// Layers in depth order, each showing through wherever its mask is open.
fn composite(genome: &Genome) -> String {
    let mut blocks = vec![layer_block(genome.base_layer(), genome, true)];
    for layer in genome.layers_above_base() {
        blocks.push(layer_block(layer, genome, false));
    }
    blocks.join("\n")
}

/// A complete fragment shader for one genome, paired with a vertex shader that
/// covers the screen with a single triangle.
pub fn shader_source(genome: &Genome) -> String {
    format!(
        r#"#include <metal_stdlib>
using namespace metal;

struct Mover {{
    float2 position;
    float wraps;
    float unused;
}};

struct Uniforms {{
    float2 resolution;
    float time;
    float rotation;
    float paletteScale;
    float opacity;
    float2 extent;
    float paletteBlend;
    float unused;
    float p[{param_count}];
    Mover movers[{mover_count}];
}};

/// Offset to a mover, taking the nearest copy of one that wraps.
static inline float2 psyWrapDelta(float2 delta, float2 extent, float wraps) {{
    if (wraps > 0.5) {{
        float2 span = 2.0 * extent;
        delta -= span * round(delta / span);
    }}
    return delta;
}}

vertex float4 psy_vertex(uint id [[vertex_id]]) {{
    float2 corners[3] = {{ float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) }};
    return float4(corners[id], 0.0, 1.0);
}}

fragment float4 psy_fragment(float4 position [[position]],
                             constant Uniforms &u [[buffer(0)]],
                             texture1d<float> palette [[texture(0)]],
                             texture1d<float> paletteNext [[texture(1)]],
                             sampler paletteSampler [[sampler(0)]]) {{
    float shortAxis = min(u.resolution.x, u.resolution.y) * 0.5;
    float time = u.time;
    float2 rawPoint = float2(
        (position.x - u.resolution.x * 0.5) / shortAxis,
        (position.y - u.resolution.y * 0.5) / shortAxis
    );

{ripple_prologue}

    constant float *p = u.p;
    float value = 0.0;

{composite}

    float index = fract(value * 0.5 * u.paletteScale + u.rotation);
    float3 near = palette.sample(paletteSampler, index).rgb;
    float3 far = paletteNext.sample(paletteSampler, index).rgb;
    return float4(mix(near, far, u.paletteBlend), u.opacity);
}}
"#,
        param_count = PARAM_COUNT,
        mover_count = MOVER_COUNT,
        ripple_prologue = ripple_prologue(genome),
        composite = composite(genome),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, fallback_genome, seeded_rng};

    #[test]
    fn every_node_kind_emits_metal() {
        let x = Box::new(Expr::X);
        let y = Box::new(Expr::Y);
        assert_eq!(emit(&Expr::X), "x");
        assert_eq!(emit(&Expr::Y), "y");
        assert_eq!(emit(&Expr::Radius), "radius");
        assert_eq!(emit(&Expr::Theta), "theta");
        assert_eq!(emit(&Expr::Time), format!("(time * {TIME_SCALE:?})"));
        assert_eq!(emit(&Expr::Param(3)), "p[3]");
        assert_eq!(emit(&Expr::Sin(x.clone())), "sin(x)");
        assert_eq!(emit(&Expr::Cos(x.clone())), "cos(x)");
        assert_eq!(emit(&Expr::Neg(x.clone())), "(-x)");
        assert_eq!(emit(&Expr::Add(x.clone(), y.clone())), "(x + y)");
        assert_eq!(emit(&Expr::Sub(x.clone(), y.clone())), "(x - y)");
        assert_eq!(emit(&Expr::Mul(x.clone(), y.clone())), "(x * y)");
        assert_eq!(emit(&Expr::Hypot(x, y)), "length(float2(x, y))");
    }

    #[test]
    fn a_parameter_index_is_kept_inside_the_slot_array() {
        assert_eq!(emit(&Expr::Param(PARAM_COUNT + 5)), "p[5]");
    }

    #[test]
    fn the_shader_carries_the_genome_field() {
        let genome = fallback_genome();
        let source = shader_source(&genome);
        assert!(source.contains(&format!(
            "float layerValue = {};",
            emit(&genome.base_layer().field)
        )));
    }

    #[test]
    fn the_shader_declares_the_same_number_of_slots_as_the_genome() {
        let genome = Genome::random(&mut seeded_rng(8));
        assert!(shader_source(&genome).contains(&format!("float p[{PARAM_COUNT}];")));
    }

    #[test]
    fn every_layer_emits_its_own_fold_and_field() {
        let mut rng = crate::genome::seeded_rng(53);
        let genome = crate::genome::Genome::random(&mut rng);
        let source = shader_source(&genome);
        assert_eq!(
            source.matches("float layerValue =").count(),
            genome.layers.len()
        );
        assert_eq!(
            source.matches("float alpha = ").count(),
            genome.layers.len() - 1
        );
    }

    #[test]
    fn a_layer_above_the_base_emits_its_mask() {
        let mut rng = crate::genome::seeded_rng(127);
        let genome = crate::genome::Genome::random(&mut rng);
        let source = shader_source(&genome);
        assert_eq!(
            source.matches("float maskWave =").count(),
            genome.layers.len() - 1
        );
    }

    #[test]
    fn a_layer_emits_its_own_detail_scale() {
        let mut genome = fallback_genome();
        genome.layers[0].detail = 1.75;
        assert!(shader_source(&genome).contains("x = waveX * 1.75;"));
    }

    #[test]
    fn a_layer_emits_its_own_tone_offset() {
        let mut genome = fallback_genome();
        genome.layers[0].tone_offset = 0.5;
        assert!(shader_source(&genome).contains("sin(layerValue) + 0.5;"));
    }

    #[test]
    fn the_base_layer_paints_straight_into_the_value() {
        let source = shader_source(&fallback_genome());
        assert!(source.contains("value = layerValue;"));
    }

    #[test]
    fn the_shader_blends_between_the_scene_palettes() {
        let source = shader_source(&fallback_genome());
        assert!(source.contains("mix(near, far, u.paletteBlend)"));
    }

    #[test]
    fn the_fold_turns_with_the_layer_spin() {
        let mut genome = fallback_genome();
        genome.layers[0].spin = 0.05;
        assert!(shader_source(&genome).contains("atan2(y, x) + 0.05 * time"));
    }

    #[test]
    fn a_warp_with_ripples_emits_one_ring_source_per_mover() {
        let mut genome = fallback_genome();
        genome.warp = crate::genome::Warp {
            ripple_amplitude: 0.05,
            ..crate::genome::Warp::none()
        };
        let source = shader_source(&genome);
        assert_eq!(
            source.matches("float rippleDistance =").count(),
            MOVER_COUNT
        );
    }

    #[test]
    fn a_warp_without_ripples_emits_none() {
        let genome = fallback_genome();
        assert!(!shader_source(&genome).contains("rippleDistance"));
    }

    #[test]
    fn the_waves_are_applied_after_the_fold() {
        let mut genome = fallback_genome();
        genome.warp = crate::genome::Warp {
            amplitude: 0.1,
            frequency: 3.0,
            speed: 1.0,
            ..crate::genome::Warp::none()
        };
        let source = shader_source(&genome);
        let fold = source.find("theta = theta - 1.0471976").expect("fold");
        let waves = source.find("float waveX =").expect("waves");
        assert!(fold < waves);
    }

    #[test]
    fn the_shader_returns_the_opacity_used_for_crossfading() {
        assert!(
            shader_source(&fallback_genome())
                .contains("mix(near, far, u.paletteBlend), u.opacity)")
        );
    }
}
