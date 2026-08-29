//! Metal rendering. Each scene compiles to its own pipeline; a crossfade draws
//! the incoming scene over the current one with blending.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

use objc2_metal::{
    MTLBlendFactor, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLLibrary, MTLLoadAction, MTLPixelFormat,
    MTLPrimitiveType, MTLRegion, MTLRenderCommandEncoder, MTLRenderPassDescriptor,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLSamplerAddressMode,
    MTLSamplerDescriptor, MTLSamplerMinMagFilter, MTLSamplerState, MTLSize, MTLStoreAction,
    MTLTexture, MTLTextureDescriptor, MTLTextureType, MTLTextureUsage,
};

use crate::genome::PARAM_COUNT;
use crate::motion::{Edge, MOVER_COUNT, Motion};
use crate::msl::shader_source;
use crate::scene::{Engine, Scene};

pub const PIXEL_FORMAT: MTLPixelFormat = MTLPixelFormat::BGRA8Unorm;

/// Mirrors the `Mover` struct in the generated shader, padding included.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MoverUniform {
    position: [f32; 2],
    wraps: f32,
    unused: f32,
}

/// Mirrors the `Uniforms` struct in the generated shader. Field order and
/// padding have to match it exactly.
#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    rotation: f32,
    palette_scale: f32,
    opacity: f32,
    extent: [f32; 2],
    palette_blend: f32,
    /// Keeps the parameter array at the offset Metal gives it, since the mover
    /// array that follows is eight byte aligned there.
    unused: f32,
    params: [f32; PARAM_COUNT],
    movers: [MoverUniform; MOVER_COUNT],
}

fn mover_uniforms(motion: &Motion) -> [MoverUniform; MOVER_COUNT] {
    let mut uniforms = [MoverUniform::default(); MOVER_COUNT];
    for (slot, mover) in uniforms.iter_mut().zip(motion.movers.iter()) {
        slot.position = mover.position;
        slot.wraps = if mover.edge == Edge::Wrap { 1.0 } else { 0.0 };
    }
    uniforms
}

struct GpuScene {
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    palette: Retained<ProtocolObject<dyn MTLTexture>>,
    palette_next: Retained<ProtocolObject<dyn MTLTexture>>,
}

pub struct Renderer {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    sampler: Retained<ProtocolObject<dyn MTLSamplerState>>,
    current: GpuScene,
    incoming: Option<GpuScene>,
    /// Identifies which engine scene `incoming` was built from, so the
    /// pipeline is compiled once per crossfade rather than once per frame.
    incoming_is_stale: bool,
}

impl Renderer {
    pub fn new(engine: &Engine) -> Result<Renderer, String> {
        let device = MTLCreateSystemDefaultDevice().ok_or("no Metal device")?;
        let queue = device
            .newCommandQueue()
            .ok_or("could not create a command queue")?;
        let sampler = make_sampler(&device)?;
        let current = GpuScene::new(&device, engine.current())?;
        Ok(Renderer {
            device,
            queue,
            sampler,
            current,
            incoming: None,
            incoming_is_stale: false,
        })
    }

    pub fn device(&self) -> &ProtocolObject<dyn MTLDevice> {
        &self.device
    }

    /// Pick up any scene change the engine has made. Shader compilation happens
    /// here, off the per-frame path, during the crossfade.
    pub fn sync(&mut self, engine: &Engine) -> Result<(), String> {
        match engine.incoming() {
            Some(scene) => {
                if self.incoming.is_none() || self.incoming_is_stale {
                    self.incoming = Some(GpuScene::new(&self.device, scene)?);
                    self.incoming_is_stale = false;
                }
            }
            None => {
                if let Some(finished) = self.incoming.take() {
                    self.current = finished;
                }
                self.incoming_is_stale = true;
            }
        }
        Ok(())
    }

    /// Point a layer at this renderer's device and match its drawable size to
    /// the pixels it will be shown at.
    pub fn configure_layer(&self, layer: &CAMetalLayer, width: f64, height: f64) {
        layer.setDevice(Some(&self.device));
        layer.setPixelFormat(PIXEL_FORMAT);
        layer.setFramebufferOnly(true);
        layer.setDrawableSize(objc2_foundation::NSSize { width, height });
    }

    /// Draw one frame into the layer's next drawable and present it. Returns
    /// false when the layer had no drawable to give, which happens before it
    /// has a size.
    pub fn draw_to_layer(&self, engine: &Engine, layer: &CAMetalLayer) -> bool {
        let Some(drawable) = layer.nextDrawable() else {
            return false;
        };
        let texture = drawable.texture();
        self.draw_with(engine, &texture, Some(&drawable));
        true
    }

    /// Draw one frame into `target`, which is a drawable texture or an
    /// offscreen texture.
    pub fn draw(&self, engine: &Engine, target: &ProtocolObject<dyn MTLTexture>) {
        self.draw_with(engine, target, None);
    }

    fn draw_with(
        &self,
        engine: &Engine,
        target: &ProtocolObject<dyn MTLTexture>,
        drawable: Option<&ProtocolObject<dyn CAMetalDrawable>>,
    ) {
        let width = target.width() as f32;
        let height = target.height() as f32;
        let descriptor = MTLRenderPassDescriptor::new();
        let attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
        attachment.setTexture(Some(target));
        attachment.setLoadAction(MTLLoadAction::Clear);
        attachment.setStoreAction(MTLStoreAction::Store);
        attachment.setClearColor(MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        });

        let command_buffer = self
            .queue
            .commandBuffer()
            .expect("command buffer from queue");
        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&descriptor)
            .expect("render encoder from pass descriptor");

        let fade = engine.fade();
        self.encode_scene(
            &encoder,
            &self.current,
            engine.current(),
            engine.motion(),
            engine.time(),
            width,
            height,
            1.0,
        );
        if let (Some(gpu_scene), Some(scene)) = (self.incoming.as_ref(), engine.incoming())
            && fade > 0.0
        {
            self.encode_scene(
                &encoder,
                gpu_scene,
                scene,
                engine.motion(),
                engine.time(),
                width,
                height,
                fade,
            );
        }

        encoder.endEncoding();
        match drawable {
            Some(drawable) => {
                command_buffer.presentDrawable(drawable.as_ref());
                command_buffer.commit();
            }
            None => {
                command_buffer.commit();
                command_buffer.waitUntilCompleted();
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_scene(
        &self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        gpu_scene: &GpuScene,
        scene: &Scene,
        motion: &Motion,
        time: f32,
        width: f32,
        height: f32,
        opacity: f32,
    ) {
        let mut params = [0.0f32; PARAM_COUNT];
        for (slot, value) in params.iter_mut().zip(scene.genome.param_values(time)) {
            *slot = value;
        }
        let uniforms = Uniforms {
            resolution: [width, height],
            time,
            rotation: scene.rotation,
            palette_scale: scene.genome.palette_scale_at(time),
            opacity,
            extent: motion.extent,
            palette_blend: scene.blend_at(time),
            unused: 0.0,
            params,
            movers: mover_uniforms(motion),
        };
        encoder.setRenderPipelineState(&gpu_scene.pipeline);
        unsafe {
            encoder.setFragmentBytes_length_atIndex(
                std::ptr::NonNull::from(&uniforms).cast(),
                std::mem::size_of::<Uniforms>(),
                0,
            );
            encoder.setFragmentTexture_atIndex(Some(&gpu_scene.palette), 0);
            encoder.setFragmentTexture_atIndex(Some(&gpu_scene.palette_next), 1);
            encoder.setFragmentSamplerState_atIndex(Some(&self.sampler), 0);
            encoder.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3);
        }
    }
}

impl GpuScene {
    fn new(device: &ProtocolObject<dyn MTLDevice>, scene: &Scene) -> Result<GpuScene, String> {
        let source = NSString::from_str(&shader_source(&scene.genome));
        let library = device
            .newLibraryWithSource_options_error(&source, None)
            .map_err(|error| format!("shader compile failed: {error}"))?;
        let vertex = library
            .newFunctionWithName(&NSString::from_str("psy_vertex"))
            .ok_or("missing psy_vertex")?;
        let fragment = library
            .newFunctionWithName(&NSString::from_str("psy_fragment"))
            .ok_or("missing psy_fragment")?;

        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(&vertex));
        descriptor.setFragmentFunction(Some(&fragment));
        let attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
        attachment.setPixelFormat(PIXEL_FORMAT);
        attachment.setBlendingEnabled(true);
        attachment.setSourceRGBBlendFactor(MTLBlendFactor::SourceAlpha);
        attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
        attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
        attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);

        let pipeline = device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|error| format!("pipeline creation failed: {error}"))?;

        Ok(GpuScene {
            pipeline,
            palette: make_palette_texture(device, &scene.palette)?,
            palette_next: make_palette_texture(device, &scene.palette_next)?,
        })
    }
}

fn make_palette_texture(
    device: &ProtocolObject<dyn MTLDevice>,
    palette: &crate::palette::Palette,
) -> Result<Retained<ProtocolObject<dyn MTLTexture>>, String> {
    let bytes = palette.to_rgba_bytes();
    let descriptor = MTLTextureDescriptor::new();
    descriptor.setTextureType(MTLTextureType::Type1D);
    descriptor.setPixelFormat(MTLPixelFormat::RGBA8Unorm);
    unsafe {
        descriptor.setWidth(crate::palette::PALETTE_SIZE);
        descriptor.setHeight(1);
    }
    descriptor.setUsage(MTLTextureUsage::ShaderRead);
    let texture = device
        .newTextureWithDescriptor(&descriptor)
        .ok_or("could not create the palette texture")?;
    let region = MTLRegion {
        origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
        size: MTLSize {
            width: crate::palette::PALETTE_SIZE,
            height: 1,
            depth: 1,
        },
    };
    unsafe {
        texture.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
            region,
            0,
            std::ptr::NonNull::new(bytes.as_ptr() as *mut _).expect("palette bytes"),
            0,
        );
    }
    Ok(texture)
}

fn make_sampler(
    device: &ProtocolObject<dyn MTLDevice>,
) -> Result<Retained<ProtocolObject<dyn MTLSamplerState>>, String> {
    let descriptor = MTLSamplerDescriptor::new();
    descriptor.setMinFilter(MTLSamplerMinMagFilter::Linear);
    descriptor.setMagFilter(MTLSamplerMinMagFilter::Linear);
    descriptor.setSAddressMode(MTLSamplerAddressMode::Repeat);
    device
        .newSamplerStateWithDescriptor(&descriptor)
        .ok_or_else(|| "could not create the palette sampler".to_string())
}

/// Offscreen render target, used for tests and for the settings preview.
pub fn make_offscreen_texture(
    device: &ProtocolObject<dyn MTLDevice>,
    width: usize,
    height: usize,
) -> Retained<ProtocolObject<dyn MTLTexture>> {
    let descriptor = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            PIXEL_FORMAT,
            width,
            height,
            false,
        )
    };
    descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    descriptor.setStorageMode(objc2_metal::MTLStorageMode::Managed);
    device
        .newTextureWithDescriptor(&descriptor)
        .expect("offscreen texture")
}

/// Read a rendered texture back as RGB bytes.
pub fn read_texture_rgb(texture: &ProtocolObject<dyn MTLTexture>) -> Vec<u8> {
    let width = texture.width();
    let height = texture.height();
    let mut bgra = vec![0u8; width * height * 4];
    let region = MTLRegion {
        origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
        size: MTLSize {
            width,
            height,
            depth: 1,
        },
    };
    unsafe {
        texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
            std::ptr::NonNull::new(bgra.as_mut_ptr().cast()).expect("readback buffer"),
            width * 4,
            region,
            0,
        );
    }
    bgra.chunks_exact(4)
        .flat_map(|pixel| [pixel[2], pixel[1], pixel[0]])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uniform_block_matches_the_layout_the_shader_declares() {
        assert_eq!(std::mem::size_of::<MoverUniform>(), 16);
        assert_eq!(
            std::mem::size_of::<Uniforms>(),
            40 + PARAM_COUNT * 4 + MOVER_COUNT * 16
        );
    }

    #[test]
    fn a_wrapping_mover_is_marked_for_the_shader() {
        let mut motion = Motion::still();
        motion.movers[1].edge = Edge::Wrap;
        let uniforms = mover_uniforms(&motion);
        assert_eq!(uniforms[0].wraps, 0.0);
        assert_eq!(uniforms[1].wraps, 1.0);
        assert_eq!(uniforms[1].position, motion.movers[1].position);
    }
}
