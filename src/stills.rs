//! Still-frame rendering to PNG, which is how patterns get judged without
//! running a screen saver.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use crate::render::{Renderer, make_offscreen_texture, read_texture_rgb};
use crate::scene::Engine;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// The evaluator, which needs no GPU.
    Cpu,
    /// The generated shaders, which is what the screen saver runs.
    Gpu,
}

/// What the frames in a run vary by.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Series {
    /// One frame per seed, each from a fresh engine.
    Seeds,
    /// One engine sampled over time, which is how motion gets reviewed.
    Frames { interval_seconds: f32 },
}

#[derive(Clone, Debug)]
pub struct StillsRequest {
    pub seed: u64,
    pub count: usize,
    pub size: usize,
    /// Seconds of drift applied before the frame is captured.
    pub warmup_seconds: f32,
    pub backend: Backend,
    pub series: Series,
    pub directory: PathBuf,
}

impl StillsRequest {
    /// Read `seed count size directory` from command line arguments, keeping
    /// the defaults for any that are missing.
    pub fn from_args(args: &[String], backend: Backend) -> Result<StillsRequest, String> {
        let parse = |index: usize, name: &str, fallback: usize| -> Result<usize, String> {
            match args.get(index) {
                Some(value) => value
                    .parse()
                    .map_err(|_| format!("{name} must be a number, got {value}")),
                None => Ok(fallback),
            }
        };
        Ok(StillsRequest {
            seed: parse(1, "seed", 1)? as u64,
            count: parse(2, "count", 4)?,
            size: parse(3, "size", 320)?,
            warmup_seconds: 3.0,
            backend,
            series: match args.get(5) {
                Some(interval) => Series::Frames {
                    interval_seconds: interval
                        .parse()
                        .map_err(|_| format!("interval must be a number, got {interval}"))?,
                },
                None => Series::Seeds,
            },
            directory: PathBuf::from(args.get(4).cloned().unwrap_or_else(|| ".".to_string())),
        })
    }
}

pub fn render(request: &StillsRequest) -> Result<Vec<PathBuf>, String> {
    let prefix = match request.backend {
        Backend::Cpu => "still",
        Backend::Gpu => "gpu",
    };
    let mut written = Vec::with_capacity(request.count);
    let mut engine = Engine::new(request.seed);
    engine.set_aspect(request.size as f32, request.size as f32);
    engine.update(request.warmup_seconds);
    for index in 0..request.count {
        match request.series {
            Series::Seeds => {
                engine = Engine::new(request.seed + index as u64);
                engine.set_aspect(request.size as f32, request.size as f32);
                engine.update(request.warmup_seconds);
            }
            Series::Frames { interval_seconds } => {
                if index > 0 {
                    engine.update(interval_seconds);
                }
            }
        }
        let pixels = capture(&engine, request)?;
        let path = request.directory.join(format!("{prefix}-{index:03}.png"));
        write_png(&path, request.size, request.size, &pixels)?;
        written.push(path);
    }
    Ok(written)
}

fn capture(engine: &Engine, request: &StillsRequest) -> Result<Vec<u8>, String> {
    match request.backend {
        Backend::Cpu => Ok(engine.render_rgb(request.size, request.size)),
        Backend::Gpu => {
            let renderer = Renderer::new(engine)?;
            let texture = make_offscreen_texture(renderer.device(), request.size, request.size);
            renderer.draw(engine, &texture);
            Ok(read_texture_rgb(&texture))
        }
    }
}

pub fn write_png(path: &Path, width: usize, height: usize, pixels: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(|error| format!("{}: {error}", path.display()))?
        .write_image_data(pixels)
        .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("psychedelic-{name}"));
        std::fs::create_dir_all(&path).expect("temp directory");
        path
    }

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn arguments_fall_back_to_defaults() {
        let request =
            StillsRequest::from_args(&arguments(&["stills"]), Backend::Cpu).expect("request");
        assert_eq!(request.seed, 1);
        assert_eq!(request.count, 4);
        assert_eq!(request.size, 320);
        assert_eq!(request.directory, PathBuf::from("."));
    }

    #[test]
    fn frames_over_time_show_the_same_pattern_moving() {
        let directory = temp_directory("series");
        let request = StillsRequest {
            seed: 4,
            count: 3,
            size: 24,
            warmup_seconds: 0.0,
            backend: Backend::Cpu,
            series: Series::Frames {
                interval_seconds: 2.0,
            },
            directory,
        };
        let written = render(&request).expect("stills");
        assert_eq!(written.len(), 3);
        let sizes: Vec<u64> = written
            .iter()
            .map(|path| std::fs::metadata(path).expect("png").len())
            .collect();
        assert!(sizes.iter().all(|size| *size > 0));
    }

    #[test]
    fn an_interval_argument_asks_for_frames_over_time() {
        let request = StillsRequest::from_args(
            &arguments(&["stills", "1", "3", "64", "/tmp/out", "0.5"]),
            Backend::Cpu,
        )
        .expect("request");
        assert_eq!(
            request.series,
            Series::Frames {
                interval_seconds: 0.5
            }
        );
    }

    #[test]
    fn an_interval_that_is_not_a_number_is_reported() {
        let error = StillsRequest::from_args(
            &arguments(&["stills", "1", "1", "8", "/tmp", "soon"]),
            Backend::Cpu,
        )
        .expect_err("bad interval");
        assert_eq!(error, "interval must be a number, got soon");
    }

    #[test]
    fn arguments_are_read_in_order() {
        let request = StillsRequest::from_args(
            &arguments(&["stills", "9", "2", "64", "/tmp/out"]),
            Backend::Gpu,
        )
        .expect("request");
        assert_eq!(request.seed, 9);
        assert_eq!(request.count, 2);
        assert_eq!(request.size, 64);
        assert_eq!(request.directory, PathBuf::from("/tmp/out"));
        assert_eq!(request.backend, Backend::Gpu);
        assert_eq!(request.series, Series::Seeds);
    }

    #[test]
    fn an_argument_that_is_not_a_number_is_reported() {
        let error = StillsRequest::from_args(&arguments(&["stills", "wat"]), Backend::Cpu)
            .expect_err("bad seed");
        assert_eq!(error, "seed must be a number, got wat");
    }

    #[test]
    fn the_evaluator_writes_one_png_per_frame() {
        let directory = temp_directory("cpu-stills");
        let request = StillsRequest {
            seed: 3,
            count: 2,
            size: 32,
            warmup_seconds: 1.0,
            backend: Backend::Cpu,
            series: Series::Seeds,
            directory: directory.clone(),
        };
        let written = render(&request).expect("stills");
        assert_eq!(written.len(), 2);
        assert!(
            written
                .iter()
                .all(|path| std::fs::metadata(path).expect("png").len() > 0)
        );
        assert_eq!(written[0], directory.join("still-000.png"));
    }

    #[test]
    fn the_shader_path_writes_its_own_frames() {
        let directory = temp_directory("gpu-stills");
        let request = StillsRequest {
            seed: 3,
            count: 1,
            size: 32,
            warmup_seconds: 1.0,
            backend: Backend::Gpu,
            series: Series::Seeds,
            directory: directory.clone(),
        };
        let written = render(&request).expect("stills");
        assert_eq!(written[0], directory.join("gpu-000.png"));
    }

    #[test]
    fn a_directory_that_does_not_exist_is_reported() {
        let request = StillsRequest {
            seed: 1,
            count: 1,
            size: 8,
            warmup_seconds: 0.0,
            backend: Backend::Cpu,
            series: Series::Seeds,
            directory: PathBuf::from("/nonexistent-psychedelic-directory"),
        };
        assert!(render(&request).is_err());
    }
}
