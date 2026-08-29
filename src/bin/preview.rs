//! Windowed preview of the screen saver, for tuning patterns without
//! installing a bundle.
//!
//! Space breeds the next pattern, F toggles fullscreen, Escape quits.

use std::time::Instant;

use objc2::rc::Retained;
use objc2_app_kit::NSView;
use objc2_quartz_core::CAMetalLayer;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Fullscreen, Window, WindowId};

use psychedelic::render::Renderer;
use psychedelic::scene::Engine;

struct Preview {
    window: Option<Window>,
    layer: Option<Retained<CAMetalLayer>>,
    engine: Engine,
    renderer: Option<Renderer>,
    last_frame: Instant,
}

impl Preview {
    fn new(seed: u64) -> Preview {
        Preview {
            window: None,
            layer: None,
            engine: Engine::new(seed),
            renderer: None,
            last_frame: Instant::now(),
        }
    }

    fn resize(&mut self) {
        let (Some(window), Some(layer), Some(renderer)) = (
            self.window.as_ref(),
            self.layer.as_ref(),
            self.renderer.as_ref(),
        ) else {
            return;
        };
        let size = window.inner_size();
        let scale = window.scale_factor();
        self.engine
            .set_aspect(size.width as f32, size.height as f32);
        layer.setFrame(objc2_foundation::NSRect::new(
            objc2_foundation::NSPoint::new(0.0, 0.0),
            objc2_foundation::NSSize::new(
                f64::from(size.width) / scale,
                f64::from(size.height) / scale,
            ),
        ));
        layer.setContentsScale(scale);
        renderer.configure_layer(layer, size.width.into(), size.height.into());
    }
}

impl ApplicationHandler for Preview {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes().with_title("Psychedelic");
        let window = event_loop
            .create_window(attributes)
            .expect("could not create a window");

        let layer = CAMetalLayer::new();
        let handle = window.window_handle().expect("window handle");
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            panic!("this preview runs on macOS only");
        };
        // A layer-hosting view takes its layer before wantsLayer is set. The
        // other order leaves AppKit managing a layer of its own.
        let view: &NSView = unsafe { appkit.ns_view.cast().as_ref() };
        view.setLayer(Some(&layer));
        view.setWantsLayer(true);

        let renderer = Renderer::new(&self.engine).expect("Metal renderer");
        self.window = Some(window);
        self.layer = Some(layer);
        self.renderer = Some(renderer);
        self.resize();
        self.last_frame = Instant::now();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => self.resize(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::Space) => self.engine.advance_scene(),
                    Key::Character("f") => {
                        if let Some(window) = self.window.as_ref() {
                            let next = match window.fullscreen() {
                                Some(_) => None,
                                None => Some(Fullscreen::Borderless(None)),
                            };
                            window.set_fullscreen(next);
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let delta = now.duration_since(self.last_frame).as_secs_f32();
                self.last_frame = now;
                self.engine.update(delta.min(0.1));
                if let (Some(renderer), Some(layer)) = (self.renderer.as_mut(), self.layer.as_ref())
                {
                    if renderer.sync(&self.engine).is_err() {
                        self.engine.reset_incoming();
                    }
                    let _ = renderer.draw_to_layer(&self.engine, layer);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let seed = std::env::args()
        .nth(1)
        .map_or_else(rand::random::<u64>, |arg| arg.parse().expect("seed"));
    println!("seed {seed}");
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop
        .run_app(&mut Preview::new(seed))
        .expect("event loop finished with an error");
}
