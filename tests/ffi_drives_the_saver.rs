//! The C entry points the screen saver bundle calls. A crash or a null saver
//! here is a black screen in System Settings.

use objc2_quartz_core::CAMetalLayer;
use psychedelic::ffi::{
    psy_advance_scene, psy_create, psy_destroy, psy_frame, psy_frames_presented, psy_resize,
    psy_set_mutation_strength, psy_set_scene_seconds, psy_set_speed,
};

#[test]
fn a_saver_runs_through_its_whole_life_cycle() {
    let layer = CAMetalLayer::new();
    let saver = unsafe { psy_create(objc2::rc::Retained::as_ptr(&layer) as *mut _, 77) };
    assert!(!saver.is_null(), "the saver could not start");

    unsafe {
        // Before any resize the layer has no drawable, which the renderer skips.
        psy_frame(saver, 1.0 / 60.0);

        psy_resize(saver, 640.0, 400.0);
        psy_set_speed(saver, 2.0);
        psy_set_scene_seconds(saver, 5.0);
        psy_set_mutation_strength(saver, 0.9);
        for _ in 0..120 {
            psy_frame(saver, 1.0 / 60.0);
        }
        psy_advance_scene(saver);
        for _ in 0..60 {
            psy_frame(saver, 1.0 / 60.0);
        }
        assert!(
            psy_frames_presented(saver) > 0,
            "the layer never handed out a drawable"
        );
        psy_destroy(saver);
    }
}

#[test]
fn the_entry_points_ignore_a_null_saver() {
    unsafe {
        psy_resize(std::ptr::null_mut(), 100.0, 100.0);
        psy_frame(std::ptr::null_mut(), 0.016);
        psy_set_speed(std::ptr::null_mut(), 1.0);
        psy_set_scene_seconds(std::ptr::null_mut(), 10.0);
        psy_set_mutation_strength(std::ptr::null_mut(), 0.5);
        psy_advance_scene(std::ptr::null_mut());
        assert_eq!(psy_frames_presented(std::ptr::null_mut()), 0);
        psy_destroy(std::ptr::null_mut());
    }
}

#[test]
fn a_null_layer_gives_no_saver() {
    assert!(unsafe { psy_create(std::ptr::null_mut(), 1) }.is_null());
}
