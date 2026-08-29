//! The generated shader and the CPU evaluator must render the same image, since
//! the CPU path is what tests and previews rely on.

use psychedelic::render::{Renderer, make_offscreen_texture, read_texture_rgb};
use psychedelic::scene::Engine;

/// Where the two renderers disagree most, as a pixel position and channel gap.
fn worst_pixel(left: &[u8], right: &[u8], width: usize) -> (usize, usize, u8) {
    let mut worst = (0usize, 0usize, 0u8);
    for (index, (a, b)) in left.iter().zip(right.iter()).enumerate() {
        let gap = a.abs_diff(*b);
        if gap > worst.2 {
            let pixel = index / 3;
            worst = (pixel % width, pixel / width, gap);
        }
    }
    worst
}

fn mean_absolute_difference(left: &[u8], right: &[u8]) -> f32 {
    assert_eq!(left.len(), right.len());
    let total: u32 = left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| u32::from(a.abs_diff(*b)))
        .sum();
    total as f32 / left.len() as f32
}

#[test]
fn gpu_render_matches_cpu_render() {
    for seed in 1..=8u64 {
        let mut engine = Engine::new(seed);
        engine.update(2.5);

        let cpu = engine.render_rgb(128, 128);

        let renderer = Renderer::new(&engine).expect("renderer");
        let texture = make_offscreen_texture(renderer.device(), 128, 128);
        renderer.draw(&engine, &texture);
        let gpu = read_texture_rgb(&texture);

        let difference = mean_absolute_difference(&cpu, &gpu);
        let (worst_x, worst_y, gap) = worst_pixel(&cpu, &gpu, 128);
        println!(
            "seed {seed}: mean absolute difference {difference:.2}, worst {gap} at {worst_x},{worst_y}"
        );
        assert!(
            difference < 2.0,
            "seed {seed} rendered differently on GPU and CPU: {difference:.2}"
        );
    }
}
