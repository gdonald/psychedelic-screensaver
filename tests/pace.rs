//! How fast the picture moves. Structure that shifts too much from one second
//! to the next reads as shaking; structure that never shifts reads as a still
//! image. Color is measured separately, since a palette crossing can change
//! every pixel without anything moving.

use psychedelic::eval::field_at;
use psychedelic::scene::Engine;

const GRID: usize = 96;

fn field_grid(engine: &Engine) -> Vec<f32> {
    let scene = engine.current();
    let params = scene.genome.param_values(engine.time());
    let mut values = Vec::with_capacity(GRID * GRID);
    for row in 0..GRID {
        for column in 0..GRID {
            let x = (column as f32 / (GRID - 1) as f32) * 2.0 - 1.0;
            let y = (row as f32 / (GRID - 1) as f32) * 2.0 - 1.0;
            values.push(field_at(
                &scene.genome,
                &params,
                engine.motion(),
                x,
                y,
                engine.time(),
            ));
        }
    }
    values
}

fn mean_change(before: &[f32], after: &[f32]) -> f32 {
    let total: f32 = before
        .iter()
        .zip(after.iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    total / before.len() as f32
}

/// Structural change over one second, as a share of the field's full range,
/// averaged over several moments so one unusual instant cannot decide it.
fn pace(seed: u64) -> f32 {
    let mut engine = Engine::new(seed);
    engine.set_aspect(GRID as f32, GRID as f32);
    engine.update(4.0);
    let mut samples = Vec::new();
    for _ in 0..4 {
        let before = field_grid(&engine);
        engine.update(1.0);
        let after = field_grid(&engine);
        samples.push(mean_change(&before, &after) * 0.5);
        engine.update(3.0);
    }
    samples.iter().sum::<f32>() / samples.len() as f32
}

#[test]
fn a_pattern_drifts_rather_than_shakes() {
    let paces: Vec<f32> = (1..=8u64).map(pace).collect();
    let average = paces.iter().sum::<f32>() / paces.len() as f32;
    let worst = paces.iter().cloned().fold(0.0f32, f32::max);
    println!("structural change per second: {paces:?}");
    println!("average {average:.4}, worst {worst:.4}");
    assert!(
        average > 0.008,
        "the pattern is frozen: {average:.4} of its range per second"
    );
    assert!(
        average < 0.045,
        "the pattern is shaking: {average:.4} of its range per second"
    );
    assert!(
        worst < 0.075,
        "one pattern is shaking: {worst:.4} of its range per second"
    );
}
