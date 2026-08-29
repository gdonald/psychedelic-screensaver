//! Scenes and the engine that evolves them. A scene is one genome plus its
//! palette; the engine drifts the current scene continuously and crossfades
//! into a bred successor every so often.

use rand::RngExt;
use rand::rngs::StdRng;

use crate::eval::field_at;
use crate::genome::{Cycle, Genome, is_degenerate, random_interesting_at, seeded_rng};
use crate::motion::Motion;
use crate::palette::Palette;

#[derive(Clone, Debug)]
pub struct Scene {
    pub genome: Genome,
    pub palette: Palette,
    /// The palette this scene is fading toward. Colors cross between the two
    /// continuously, so the whole image keeps shifting hue.
    pub palette_next: Palette,
    pub palette_blend: Cycle,
    pub rotation: f32,
}

impl Scene {
    pub fn random(rng: &mut impl RngExt, motion: &Motion) -> Scene {
        Scene {
            genome: random_interesting_at(rng, motion, 0.0),
            palette: Palette::random(rng),
            palette_next: Palette::random(rng),
            palette_blend: Cycle::random(rng, 90.0, 260.0),
            rotation: 0.0,
        }
    }

    /// How far the scene has crossed from its palette to the next one.
    pub fn blend_at(&self, time: f32) -> f32 {
        self.palette_blend.value_at(time)
    }

    pub fn genome_ref(&self) -> &crate::genome::Genome {
        &self.genome
    }

    pub fn advance(&mut self, delta: f32) {
        self.rotation += delta * self.genome.palette_rate;
        self.rotation -= self.rotation.floor();
    }

    pub fn color_at(&self, motion: &Motion, x: f32, y: f32, time: f32) -> [u8; 3] {
        let params = self.genome.param_values(time);
        let value = field_at(&self.genome, &params, motion, x, y, time);
        let scale = self.genome.palette_scale_at(time);
        let near = self.palette.sample(value, self.rotation, scale);
        let far = self.palette_next.sample(value, self.rotation, scale);
        let blend = self.blend_at(time);
        [
            crate::scene::blend(near[0], far[0], blend),
            crate::scene::blend(near[1], far[1], blend),
            crate::scene::blend(near[2], far[2], blend),
        ]
    }
}

pub struct Engine {
    rng: StdRng,
    current: Scene,
    incoming: Option<Scene>,
    motion: Motion,
    fade: f32,
    time: f32,
    since_scene_start: f32,
    /// This scene's own hold time, drawn around `scene_seconds` so successive
    /// patterns do not change on a metronome.
    scene_hold: f32,
    scene_seconds: f32,
    pub fade_seconds: f32,
    pub mutation_strength: f32,
}

impl Engine {
    pub fn new(seed: u64) -> Engine {
        let mut rng = seeded_rng(seed);
        let motion = Motion::random(&mut rng);
        let current = Scene::random(&mut rng, &motion);
        Engine {
            rng,
            current,
            incoming: None,
            motion,
            fade: 0.0,
            time: 0.0,
            since_scene_start: 0.0,
            scene_hold: 60.0,
            scene_seconds: 60.0,
            fade_seconds: 6.0,
            mutation_strength: 0.6,
        }
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    /// How long a pattern holds before it starts crossfading. Each scene draws
    /// its own hold time around this.
    pub fn set_scene_seconds(&mut self, seconds: f32) {
        self.scene_seconds = seconds.max(1.0);
        self.scene_hold = self.draw_scene_hold();
    }

    pub fn scene_seconds(&self) -> f32 {
        self.scene_seconds
    }

    pub fn motion(&self) -> &Motion {
        &self.motion
    }

    /// Keep the movers inside the screen they are traveling across.
    pub fn set_aspect(&mut self, width: f32, height: f32) {
        self.motion.set_aspect(width, height);
    }

    pub fn current(&self) -> &Scene {
        &self.current
    }

    pub fn incoming(&self) -> Option<&Scene> {
        self.incoming.as_ref()
    }

    /// How much of the incoming scene is showing, 0 to 1.
    pub fn fade(&self) -> f32 {
        if self.incoming.is_some() {
            (self.fade / self.fade_seconds).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn update(&mut self, delta: f32) {
        self.time += delta;
        self.motion.update(delta);
        self.current.advance(delta);
        if let Some(incoming) = self.incoming.as_mut() {
            incoming.advance(delta);
            self.fade += delta;
            if self.fade >= self.fade_seconds {
                self.current = self.incoming.take().expect("incoming scene is present");
                self.fade = 0.0;
                self.since_scene_start = 0.0;
                self.scene_hold = self.draw_scene_hold();
            }
        } else {
            self.since_scene_start += delta;
            if self.since_scene_start >= self.scene_hold {
                self.incoming = Some(self.breed());
            }
        }
    }

    /// Drop a pending successor, used when its shader failed to compile.
    pub fn reset_incoming(&mut self) {
        self.incoming = None;
        self.fade = 0.0;
        self.since_scene_start = 0.0;
    }

    /// Force the next crossfade to start now.
    pub fn advance_scene(&mut self) {
        if self.incoming.is_none() {
            self.incoming = Some(self.breed());
        }
    }

    fn draw_scene_hold(&mut self) -> f32 {
        self.scene_seconds * self.rng.random_range(0.6..1.6)
    }

    fn breed(&mut self) -> Scene {
        let strength = self.mutation_strength;
        let genome = {
            let partner = random_interesting_at(&mut self.rng, &self.motion, self.time);
            let candidate = if self.rng.random_bool(0.5) {
                self.current.genome.crossover(&partner, &mut self.rng)
            } else {
                self.current.genome.mutate(&mut self.rng, strength)
            };
            if is_degenerate(&candidate, &self.motion, self.time) {
                partner
            } else {
                candidate
            }
        };
        Scene {
            genome,
            palette: Palette::random(&mut self.rng),
            palette_next: Palette::random(&mut self.rng),
            palette_blend: Cycle::random(&mut self.rng, 90.0, 260.0),
            rotation: self.current.rotation,
        }
    }

    /// Render the blended frame on the CPU into an RGB buffer. The GPU path
    /// draws the same thing; this one keeps the engine testable and renders
    /// preview stills.
    pub fn render_rgb(&self, width: usize, height: usize) -> Vec<u8> {
        let short_axis = width.min(height) as f32 * 0.5;
        let fade = self.fade();
        let mut pixels = Vec::with_capacity(width * height * 3);
        for row in 0..height {
            for column in 0..width {
                let x = (column as f32 + 0.5 - width as f32 * 0.5) / short_axis;
                let y = (row as f32 + 0.5 - height as f32 * 0.5) / short_axis;
                let near = self.current.color_at(&self.motion, x, y, self.time);
                let color = match (&self.incoming, fade) {
                    (Some(incoming), fade) if fade > 0.0 => {
                        let far = incoming.color_at(&self.motion, x, y, self.time);
                        [
                            blend(near[0], far[0], fade),
                            blend(near[1], far[1], fade),
                            blend(near[2], far[2], fade),
                        ]
                    }
                    _ => near,
                };
                pixels.extend_from_slice(&color);
            }
        }
        pixels
    }
}

fn blend(from: u8, to: u8, amount: f32) -> u8 {
    (from as f32 + (to as f32 - from as f32) * amount).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::seeded_rng;

    #[test]
    fn rotation_wraps_into_a_single_cycle() {
        let mut scene = Scene::random(&mut seeded_rng(1), &Motion::still());
        scene.genome.palette_rate = 0.4;
        for _ in 0..20 {
            scene.advance(1.0);
            assert!((0.0..1.0).contains(&scene.rotation));
        }
    }

    #[test]
    fn a_scene_crosses_from_one_palette_to_the_other() {
        let mut scene = Scene::random(&mut seeded_rng(17), &Motion::still());
        scene.palette_blend = Cycle {
            period: 8.0,
            phase: 0.0,
        };
        assert!((scene.blend_at(2.0) - 1.0).abs() < 1e-6);
        assert!((scene.blend_at(6.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn palette_crossing_changes_the_colors_a_scene_shows() {
        let mut scene = Scene::random(&mut seeded_rng(19), &Motion::still());
        scene.genome.palette_swing = 0.0;
        scene.palette = Palette::from_stops(&[[1.0, 0.0, 0.0], [0.6, 0.0, 0.0]]);
        scene.palette_next = Palette::from_stops(&[[0.0, 0.0, 1.0], [0.0, 0.0, 0.6]]);
        scene.palette_blend = Cycle {
            period: 8.0,
            phase: 0.0,
        };
        let near = scene.color_at(&Motion::still(), 0.1, 0.1, 6.0);
        let far = scene.color_at(&Motion::still(), 0.1, 0.1, 2.0);
        assert!(near[0] > near[2], "expected red, got {near:?}");
        assert!(far[2] > far[0], "expected blue, got {far:?}");
    }

    #[test]
    fn a_layer_fading_in_changes_what_the_scene_shows() {
        let mut scene = Scene::random(&mut seeded_rng(23), &Motion::still());
        scene.genome.palette_swing = 0.0;
        scene.genome.palette_rate = 0.0;
        for layer in &mut scene.genome.layers {
            layer.spin = 0.0;
        }
        let hidden = {
            let above = &mut scene.genome.layers[1];
            above.opacity_floor = 0.0;
            above.opacity = crate::genome::Cycle {
                period: 8.0,
                phase: 0.0,
            };
            above.mask = crate::genome::Mask::open();
            scene.color_at(&Motion::still(), 0.3, -0.2, 6.0)
        };
        let shown = scene.color_at(&Motion::still(), 0.3, -0.2, 2.0);
        assert_ne!(hidden, shown);
    }

    #[test]
    fn a_scene_gives_a_color_for_any_point() {
        let scene = Scene::random(&mut seeded_rng(2), &Motion::still());
        let color = scene.color_at(&Motion::still(), -0.75, 0.5, 1.0);
        assert_eq!(color.len(), 3);
    }

    #[test]
    fn a_new_engine_shows_one_scene() {
        let engine = Engine::new(3);
        assert!(engine.incoming().is_none());
        assert_eq!(engine.fade(), 0.0);
        assert_eq!(engine.time(), 0.0);
    }

    #[test]
    fn the_travel_area_follows_the_screen_the_engine_draws_to() {
        let mut engine = Engine::new(13);
        engine.set_aspect(1600.0, 1000.0);
        assert_eq!(engine.motion().extent, [1.6, 1.0]);
    }

    #[test]
    fn movers_travel_while_the_engine_runs() {
        let mut engine = Engine::new(14);
        let before = engine.motion().movers[0].position;
        engine.update(1.0);
        assert_ne!(engine.motion().movers[0].position, before);
    }

    #[test]
    fn time_accumulates_across_updates() {
        let mut engine = Engine::new(4);
        engine.update(0.5);
        engine.update(0.25);
        assert!((engine.time() - 0.75).abs() < 1e-6);
    }

    #[test]
    fn a_successor_arrives_once_the_scene_has_run_its_course() {
        let mut engine = Engine::new(5);
        engine.set_scene_seconds(1.0);
        engine.update(2.0);
        assert!(engine.incoming().is_some());
    }

    #[test]
    fn the_crossfade_finishes_by_replacing_the_current_scene() {
        let mut engine = Engine::new(6);
        engine.set_scene_seconds(1.0);
        engine.fade_seconds = 2.0;
        engine.update(2.0);
        let successor = engine.incoming().expect("successor").genome.clone();
        engine.update(1.0);
        assert!((engine.fade() - 0.5).abs() < 1e-6);
        engine.update(1.0);
        assert!(engine.incoming().is_none());
        assert_eq!(engine.current().genome, successor);
    }

    #[test]
    fn dropping_a_successor_returns_the_engine_to_one_scene() {
        let mut engine = Engine::new(12);
        engine.advance_scene();
        engine.update(1.0);
        engine.reset_incoming();
        assert!(engine.incoming().is_none());
        assert_eq!(engine.fade(), 0.0);
    }

    #[test]
    fn advancing_the_scene_starts_a_crossfade_immediately() {
        let mut engine = Engine::new(7);
        engine.advance_scene();
        assert!(engine.incoming().is_some());
    }

    #[test]
    fn advancing_again_during_a_crossfade_changes_nothing() {
        let mut engine = Engine::new(8);
        engine.advance_scene();
        let successor = engine.incoming().expect("successor").genome.clone();
        engine.advance_scene();
        assert_eq!(engine.incoming().expect("successor").genome, successor);
    }

    #[test]
    fn each_scene_holds_for_its_own_stretch_of_time() {
        let mut engine = Engine::new(15);
        engine.set_scene_seconds(20.0);
        assert_eq!(engine.scene_seconds(), 20.0);
        engine.fade_seconds = 1.0;
        let mut holds = Vec::new();
        for _ in 0..6 {
            let mut held = 0.0;
            while engine.incoming().is_none() {
                engine.update(0.5);
                held += 0.5;
            }
            holds.push(held);
            while engine.incoming().is_some() {
                engine.update(0.5);
            }
        }
        assert!(
            holds.iter().any(|hold| *hold != holds[0]),
            "every scene held for {holds:?}"
        );
        assert!(holds.iter().all(|hold| (12.0..=32.5).contains(hold)));
    }

    #[test]
    fn every_bred_scene_is_worth_showing() {
        let mut engine = Engine::new(9);
        engine.set_scene_seconds(0.0);
        for _ in 0..20 {
            engine.advance_scene();
            let bred = engine.incoming().expect("successor").genome.clone();
            assert!(!crate::genome::is_degenerate(
                &bred,
                engine.motion(),
                engine.time()
            ));
            engine.update(engine.fade_seconds);
        }
    }

    #[test]
    fn a_rendered_frame_has_three_bytes_per_pixel() {
        let engine = Engine::new(10);
        assert_eq!(engine.render_rgb(16, 9).len(), 16 * 9 * 3);
    }

    #[test]
    fn a_crossfade_renders_a_blend_of_both_scenes() {
        let mut engine = Engine::new(11);
        engine.set_scene_seconds(1.0);
        engine.fade_seconds = 4.0;
        let before = engine.render_rgb(24, 24);
        engine.update(2.0);
        engine.update(2.0);
        let midway = engine.render_rgb(24, 24);
        assert!(engine.fade() > 0.0);
        assert_ne!(before, midway);
    }

    #[test]
    fn blending_moves_from_one_value_to_the_other() {
        assert_eq!(blend(0, 200, 0.0), 0);
        assert_eq!(blend(0, 200, 1.0), 200);
        assert_eq!(blend(0, 200, 0.5), 100);
    }
}
