//! C interface used by the screen saver bundle. The Swift shell owns the
//! window and the layer; everything else lives here.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2_quartz_core::CAMetalLayer;

use crate::render::Renderer;
use crate::scene::Engine;

pub struct Saver {
    engine: Engine,
    renderer: Renderer,
    layer: Retained<CAMetalLayer>,
    speed: f32,
    frames_presented: u64,
}

impl Saver {
    /// # Safety
    /// `layer` must be a live `CAMetalLayer`.
    pub unsafe fn new(layer: *mut c_void, seed: u64) -> Option<Saver> {
        let layer: Retained<CAMetalLayer> = unsafe { Retained::retain(layer.cast())? };
        let engine = Engine::new(seed);
        let renderer = Renderer::new(&engine).ok()?;
        Some(Saver {
            engine,
            renderer,
            layer,
            speed: 1.0,
            frames_presented: 0,
        })
    }

    pub fn resize(&mut self, width: f64, height: f64) {
        self.renderer.configure_layer(&self.layer, width, height);
        self.engine.set_aspect(width as f32, height as f32);
    }

    pub fn frame(&mut self, delta_seconds: f32) {
        self.engine.update(delta_seconds * self.speed);
        if self.renderer.sync(&self.engine).is_err() {
            self.engine.reset_incoming();
        }
        if self.renderer.draw_to_layer(&self.engine, &self.layer) {
            self.frames_presented += 1;
        }
    }
}

/// Create a saver drawing into `layer`. Returns null if Metal is unavailable.
///
/// # Safety
/// `layer` must be a live `CAMetalLayer` pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psy_create(layer: *mut c_void, seed: u64) -> *mut Saver {
    match unsafe { Saver::new(layer, seed) } {
        Some(saver) => Box::into_raw(Box::new(saver)),
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `saver` must come from `psy_create` and must not be used afterward.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psy_destroy(saver: *mut Saver) {
    if !saver.is_null() {
        drop(unsafe { Box::from_raw(saver) });
    }
}

/// # Safety
/// `saver` must be a live pointer from `psy_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psy_resize(saver: *mut Saver, width: f64, height: f64) {
    if let Some(saver) = unsafe { saver.as_mut() } {
        saver.resize(width, height);
    }
}

/// # Safety
/// `saver` must be a live pointer from `psy_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psy_frame(saver: *mut Saver, delta_seconds: f32) {
    if let Some(saver) = unsafe { saver.as_mut() } {
        saver.frame(delta_seconds);
    }
}

/// Seconds a pattern holds before it starts crossfading into its successor.
///
/// # Safety
/// `saver` must be a live pointer from `psy_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psy_set_scene_seconds(saver: *mut Saver, seconds: f32) {
    if let Some(saver) = unsafe { saver.as_mut() } {
        saver.engine.set_scene_seconds(seconds);
    }
}

/// Multiplier on how fast the pattern drifts.
///
/// # Safety
/// `saver` must be a live pointer from `psy_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psy_set_speed(saver: *mut Saver, speed: f32) {
    if let Some(saver) = unsafe { saver.as_mut() } {
        saver.speed = speed.clamp(0.05, 10.0);
    }
}

/// How far each mutation moves from the parent pattern.
///
/// # Safety
/// `saver` must be a live pointer from `psy_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psy_set_mutation_strength(saver: *mut Saver, strength: f32) {
    if let Some(saver) = unsafe { saver.as_mut() } {
        saver.engine.mutation_strength = strength.clamp(0.0, 1.0);
    }
}

/// Frames the layer actually accepted, which separates a running saver from
/// one whose layer never had a drawable.
///
/// # Safety
/// `saver` must be a live pointer from `psy_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psy_frames_presented(saver: *mut Saver) -> u64 {
    match unsafe { saver.as_ref() } {
        Some(saver) => saver.frames_presented,
        None => 0,
    }
}

/// Start a crossfade into a fresh pattern now.
///
/// # Safety
/// `saver` must be a live pointer from `psy_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psy_advance_scene(saver: *mut Saver) {
    if let Some(saver) = unsafe { saver.as_mut() } {
        saver.engine.advance_scene();
    }
}
