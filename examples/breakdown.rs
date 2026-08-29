use psychedelic::eval::field_at;
use psychedelic::motion::Motion;
use psychedelic::scene::Scene;

const SIZE: usize = 64;

/// Field values rather than colors, so a palette crossing does not read as
/// motion.
fn render(scene: &Scene, motion: &Motion, time: f32) -> Vec<f32> {
    let short = SIZE as f32 * 0.5;
    let params = scene.genome.param_values(time);
    let mut values = Vec::new();
    for row in 0..SIZE {
        for column in 0..SIZE {
            let x = (column as f32 + 0.5 - short) / short;
            let y = (row as f32 + 0.5 - short) / short;
            values.push(field_at(&scene.genome, &params, motion, x, y, time));
        }
    }
    values
}

fn change(scene: &Scene, motion: &mut Motion, moving: bool) -> f32 {
    let mut total = 0.0;
    for step in 0..4 {
        let start = 4.0 + step as f32 * 4.0;
        let before = render(scene, motion, start);
        if moving {
            motion.update(1.0);
        }
        let after = render(scene, motion, start + 1.0);
        let sum: f32 = before
            .iter()
            .zip(after.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        total += sum / before.len() as f32 * 0.5;
    }
    total / 4.0
}

fn main() {
    for seed in [3u64, 4, 5] {
        // The same draw order the engine uses, so these are the patterns the
        // pace test measures.
        let mut rng = psychedelic::genome::seeded_rng(seed);
        let motion = Motion::random(&mut rng);
        let base = Scene::random(&mut rng, &motion);

        type Change = Box<dyn Fn(&mut Scene, &mut Motion)>;
        let variants: Vec<(&str, Change)> = vec![
            ("everything", Box::new(|_: &mut Scene, _: &mut Motion| {})),
            (
                "no palette blend",
                Box::new(|scene: &mut Scene, _: &mut Motion| {
                    scene.palette_next = scene.palette.clone();
                }),
            ),
            (
                "no palette swing",
                Box::new(|scene: &mut Scene, _: &mut Motion| {
                    scene.genome.palette_swing = 0.0;
                }),
            ),
            (
                "no parameter drift",
                Box::new(|scene: &mut Scene, _: &mut Motion| {
                    for param in &mut scene.genome.params {
                        param.amplitude = 0.0;
                    }
                }),
            ),
            (
                "no warp motion",
                Box::new(|scene: &mut Scene, _: &mut Motion| {
                    scene.genome.warp.speed = 0.0;
                    scene.genome.warp.ripple_speed = 0.0;
                    scene.genome.warp.swell.period = 1.0e6;
                }),
            ),
            (
                "no spin",
                Box::new(|scene: &mut Scene, _: &mut Motion| {
                    for layer in &mut scene.genome.layers {
                        layer.spin = 0.0;
                    }
                }),
            ),
            (
                "no layer opacity or mask motion",
                Box::new(|scene: &mut Scene, _: &mut Motion| {
                    for layer in &mut scene.genome.layers {
                        layer.opacity.period = 1.0e6;
                        layer.mask.drift = [0.0, 0.0];
                    }
                }),
            ),
            (
                "no movers",
                Box::new(|_: &mut Scene, motion: &mut Motion| {
                    for mover in &mut motion.movers {
                        mover.velocity = [0.0, 0.0];
                    }
                }),
            ),
        ];

        println!("seed {seed}");
        for (name, apply) in variants {
            let mut scene = base.clone();
            let mut motion = motion.clone();
            apply(&mut scene, &mut motion);
            println!("  {name:34} {:.4}", change(&scene, &mut motion, true));
        }
    }
}
