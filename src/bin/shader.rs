//! Print the Metal shader a seed produces, for reading a pattern's structure.
//!
//! Usage: shader <seed> [seconds-of-drift]

use psychedelic::msl::shader_source;
use psychedelic::scene::Engine;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args.get(1).map_or(1, |value| value.parse().expect("seed"));
    let drift: f32 = args
        .get(2)
        .map_or(0.0, |value| value.parse().expect("seconds"));
    let mut engine = Engine::new(seed);
    engine.update(drift);
    print!("{}", shader_source(engine.current().genome_ref()));
}
