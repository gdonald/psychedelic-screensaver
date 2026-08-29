//! Generated color ramps. Continuous rotation through one of these is most of
//! what reads as psychedelic.

use rand::RngExt;

pub const PALETTE_SIZE: usize = 256;

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub colors: Vec<[u8; 3]>,
}

impl Palette {
    pub fn random(rng: &mut impl RngExt) -> Palette {
        let stop_count = rng.random_range(3..=8);
        let saturation = rng.random_range(0.55..1.0);
        let stops: Vec<[f32; 3]> = (0..stop_count)
            .map(|_| {
                hsv_to_rgb(
                    rng.random_range(0.0..1.0),
                    saturation,
                    rng.random_range(0.55..1.0),
                )
            })
            .collect();
        Palette::from_stops(&stops)
    }

    /// Wrap the stops around the ramp with smooth interpolation, so index 255
    /// blends back into index 0 and rotation never shows a seam.
    pub fn from_stops(stops: &[[f32; 3]]) -> Palette {
        assert!(!stops.is_empty(), "a palette needs at least one stop");
        let colors = (0..PALETTE_SIZE)
            .map(|index| {
                let position = index as f32 / PALETTE_SIZE as f32 * stops.len() as f32;
                let first = position.floor() as usize % stops.len();
                let second = (first + 1) % stops.len();
                let blend = smoothstep(position - position.floor());
                let mut rgb = [0u8; 3];
                for channel in 0..3 {
                    let value = stops[first][channel]
                        + (stops[second][channel] - stops[first][channel]) * blend;
                    rgb[channel] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                rgb
            })
            .collect();
        Palette { colors }
    }

    /// Look up a color for a field value in [-1, 1], with `rotation` in cycles.
    pub fn sample(&self, value: f32, rotation: f32, scale: f32) -> [u8; 3] {
        let position = value * 0.5 * scale + rotation;
        let wrapped = position - position.floor();
        let index = (wrapped * PALETTE_SIZE as f32) as usize % PALETTE_SIZE;
        self.colors[index]
    }

    /// Flat RGBA bytes, the layout a Metal 1D texture wants.
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        self.colors
            .iter()
            .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
            .collect()
    }
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    let sector = (hue - hue.floor()) * 6.0;
    let offset = sector - sector.floor();
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * offset);
    let t = value * (1.0 - saturation * (1.0 - offset));
    match sector as u32 {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::seeded_rng;

    #[test]
    fn a_palette_has_one_color_per_index() {
        let palette = Palette::random(&mut seeded_rng(2));
        assert_eq!(palette.colors.len(), PALETTE_SIZE);
    }

    #[test]
    fn a_palette_wraps_without_a_seam() {
        let palette = Palette::from_stops(&[[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
        let first = palette.colors[0];
        let last = palette.colors[PALETTE_SIZE - 1];
        let gap: i32 = (0..3)
            .map(|channel| (i32::from(first[channel]) - i32::from(last[channel])).abs())
            .sum();
        assert!(gap < 30, "the ramp jumps by {gap} where it wraps");
    }

    #[test]
    fn sampling_wraps_around_for_values_outside_the_ramp() {
        let palette = Palette::random(&mut seeded_rng(6));
        assert_eq!(palette.sample(0.4, 0.0, 1.0), palette.sample(0.4, 3.0, 1.0));
        assert_eq!(
            palette.sample(0.0, -0.25, 1.0),
            palette.sample(0.0, 0.75, 1.0)
        );
    }

    #[test]
    fn rotation_moves_the_colors_along_the_ramp() {
        let palette = Palette::from_stops(&[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        assert_ne!(
            palette.sample(0.0, 0.0, 1.0),
            palette.sample(0.0, 0.33, 1.0)
        );
    }

    #[test]
    fn rgba_bytes_are_opaque_and_four_per_color() {
        let bytes = Palette::from_stops(&[[1.0, 1.0, 1.0]]).to_rgba_bytes();
        assert_eq!(bytes.len(), PALETTE_SIZE * 4);
        assert!(bytes.chunks_exact(4).all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn hue_covers_every_sector_of_the_color_wheel() {
        let expected = [
            (0.0, [1.0, 0.0, 0.0]),
            (1.0 / 6.0, [1.0, 1.0, 0.0]),
            (2.0 / 6.0, [0.0, 1.0, 0.0]),
            (3.0 / 6.0, [0.0, 1.0, 1.0]),
            (4.0 / 6.0, [0.0, 0.0, 1.0]),
            (5.0 / 6.0, [1.0, 0.0, 1.0]),
        ];
        for (hue, rgb) in expected {
            let produced = hsv_to_rgb(hue, 1.0, 1.0);
            for channel in 0..3 {
                assert!(
                    (produced[channel] - rgb[channel]).abs() < 1e-5,
                    "hue {hue} gave {produced:?}"
                );
            }
        }
    }

    #[test]
    fn a_hue_past_one_wraps_back_to_the_start() {
        assert_eq!(hsv_to_rgb(1.25, 1.0, 1.0), hsv_to_rgb(0.25, 1.0, 1.0));
    }

    #[test]
    #[should_panic(expected = "a palette needs at least one stop")]
    fn a_palette_needs_a_stop() {
        Palette::from_stops(&[]);
    }
}
