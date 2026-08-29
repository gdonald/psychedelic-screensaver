//! Every genome the engine can breed has to produce a shader Metal accepts,
//! since a compile failure at runtime would leave a blank screen.

use psychedelic::render::{Renderer, make_offscreen_texture, read_texture_rgb};
use psychedelic::scene::Engine;

#[test]
fn every_bred_genome_compiles_and_draws() {
    let mut engine = Engine::new(2024);
    engine.fade_seconds = 1.0;
    let mut renderer = Renderer::new(&engine).expect("renderer");
    let texture = make_offscreen_texture(renderer.device(), 64, 64);

    for _ in 0..24 {
        engine.advance_scene();
        renderer.sync(&engine).expect("incoming scene compiles");
        engine.update(1.0);
        renderer.sync(&engine).expect("scene swap");
        renderer.draw(&engine, &texture);
        let pixels = read_texture_rgb(&texture);
        assert!(
            pixels.iter().any(|channel| *channel != pixels[0]),
            "the frame came out flat"
        );
    }
}
