#![doc = include_str!("../README.md")]

use std::sync::Arc;

pub use fusuma_render_vello_common::Scene;
use fusuma_render_vello_common::{
    Backend, ImageSource, MAX_SIDE,
    kurbo::{Affine, BezPath, Rect as KRect},
    peniko::Fill,
    stroke,
};
use vello_gpu::{
    RenderOptions, RenderSize, RenderTarget, RenderTargetConfig, Renderer as GpuRenderer,
    Resources, TargetInit, TextureBindings,
};
use vello_gpu::{TextureId, TextureRegion};
/// The GPU library the scene draws with, so a host makes its textures with the same version.
pub use wgpu;

/// A place in a texture, for a copy.
fn at(texture: &wgpu::Texture, x: u32, y: u32) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d { x, y, z: 0 },
        aspect: wgpu::TextureAspect::All,
    }
}

/// The most picture moves one frame may ask for.
/// Renders that may fail in a row before the frames kept for drawing again are given up (a lost device never recovers).
const MAX_FAILED: u8 = 3;

const MAX_SHIFTS: usize = 4;

/// The Fusuma renderer on `vello_gpu`.
pub type VelloGpu = Backend<GpuScene>;

/// Why a GPU scene could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuError {
    /// A side is zero or above `u16::MAX`.
    BadSize,
    /// No usable GPU adapter was found.
    NoAdapter,
    /// The adapter would not give a device.
    NoDevice(String),
}

impl core::fmt::Display for GpuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadSize => f.write_str("a surface side is zero or too big"),
            Self::NoAdapter => f.write_str("no usable GPU adapter"),
            Self::NoDevice(e) => write!(f, "the GPU gave no device: {e}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// A texture the scene owns for a surface the host fills (a video, a camera): the host gets the device and the queue the
/// window draws with, and a texture made on them, so it can write its frames into it on the GPU with no copy through the
/// CPU. The texture is the scene's, never the host's: a `wgpu` texture from another device (or another instance) cannot be
/// told apart from a good one and crashes the renderer, so there is no way to hand one over.
pub struct TextureTarget<'a> {
    scene: &'a mut GpuScene,
    id: u32,
    used: Option<(u32, u32)>,
}

impl core::fmt::Debug for TextureTarget<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextureTarget")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl TextureTarget<'_> {
    /// The device the window draws with.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.scene.device
    }

    /// The queue the window draws with.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.scene.queue
    }

    /// The texture of this surface, `width` by `height`, `Rgba8Unorm` with `TEXTURE_BINDING`, `COPY_DST`, `COPY_SRC` and
    /// `RENDER_ATTACHMENT` (write to it with the queue, copy into it, or render into it). It is kept between frames, so
    /// the same one comes back while the size stays; a new size makes a new texture. Whatever is in it is drawn,
    /// stretched over the surface, when the closure returns `true`.
    pub fn texture(&mut self, width: u32, height: u32) -> wgpu::Texture {
        self.used = Some((width.max(1), height.max(1)));
        self.scene.host_texture(self.id, width, height)
    }
}

/// A `vello_gpu` scene, the `wgpu` device it renders on and a texture to render into.
pub struct GpuScene {
    scene: vello_gpu::Scene,
    renderer: GpuRenderer,
    resources: Resources,
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    depth: wgpu::TextureView,
    size: (u16, u16),
    pixels: Vec<u8>,
    /// The parts of the next frame that matter (from [`Scene::add_damage`] and [`Scene::set_damage`]): what is read back
    /// to the CPU. The GPU draws only what was drawn since the last render, wherever that is.
    damage: Vec<(i32, i32, u32, u32)>,
    /// Moves of the picture to make before the new commands are drawn over it ([`Scene::shift`]).
    shifts: Vec<((i32, i32, u32, u32), i32)>,
    /// A copy of rows being moved: a texture cannot be copied onto itself.
    scratch: Option<wgpu::Texture>,
    /// The buffer the changed part is copied into, kept between frames and made larger when a frame needs more.
    #[cfg(not(target_arch = "wasm32"))]
    staging: Option<wgpu::Buffer>,
    /// What the next shapes are drawn with, kept to put back after the scene is emptied.
    translation: (f64, f64),
    paint_transform: Affine,
    /// Clips pushed and not yet popped, with the transform each had: emptying the scene drops them, and they come back.
    open_clips: Vec<(KRect, Affine)>,
    dirty: bool,
    /// Renders in a row that failed. The frame's commands are kept and drawn again with the next one, so a failure
    /// loses no part of the picture (see [`MAX_FAILED`]).
    failed: u8,
    /// Test hook: the next renders fail.
    #[cfg(test)]
    fail_renders: u8,
    /// The textures of the surfaces the host fills, by the surface's id.
    hosted: std::collections::HashMap<u32, wgpu::Texture>,
    /// Images uploaded for the frame being recorded.
    bindings: TextureBindings,
    next_texture: u64,
}

impl core::fmt::Debug for GpuScene {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GpuScene")
            .field("size", &self.size)
            .field("dirty", &self.dirty)
            .finish_non_exhaustive()
    }
}

impl GpuScene {
    /// A scene on the best available GPU, without a window. Not on the web, where a device comes from an
    /// asynchronous request: make it there and use [`with_device`](Self::with_device).
    ///
    /// # Errors
    /// [`GpuError`] when the size is bad or there is no GPU.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(width: u32, height: u32) -> Result<Self, GpuError> {
        let (w, h) = Self::check(width, height)?;
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|_| GpuError::NoAdapter)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fusuma-render-vello"),
            ..Default::default()
        }))
        .map_err(|e| GpuError::NoDevice(e.to_string()))?;
        Ok(Self::with_device(device, queue, w, h))
    }

    /// A scene on a device that the host already has, so the picture can be used by its own
    /// pipelines.
    #[must_use]
    pub fn with_device(device: wgpu::Device, queue: wgpu::Queue, width: u16, height: u16) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fusuma render target"),
            size: wgpu::Extent3d {
                width: u32::from(width),
                height: u32::from(height),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let (renderer, resources) = GpuRenderer::new(
            &device,
            &queue,
            &RenderTargetConfig {
                format: texture.format(),
                width,
                height,
            },
        );
        let depth = renderer.create_depth_texture_view(&RenderSize { width, height });
        Self {
            scene: vello_gpu::Scene::new(width, height),
            renderer,
            resources,
            device,
            queue,
            texture,
            depth,
            size: (width, height),
            pixels: vec![0; usize::from(width) * usize::from(height) * 4],
            damage: Vec::new(),
            shifts: Vec::new(),
            scratch: None,
            translation: (0.0, 0.0),
            paint_transform: Affine::IDENTITY,
            open_clips: Vec::new(),
            dirty: true,
            failed: 0,
            hosted: std::collections::HashMap::new(),
            #[cfg(test)]
            fail_renders: 0,
            bindings: TextureBindings::new(),
            #[cfg(not(target_arch = "wasm32"))]
            staging: None,
            next_texture: 0,
        }
    }

    fn check(width: u32, height: u32) -> Result<(u16, u16), GpuError> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err(GpuError::BadSize);
        }
        Ok((
            u16::try_from(width).map_err(|_| GpuError::BadSize)?,
            u16::try_from(height).map_err(|_| GpuError::BadSize)?,
        ))
    }

    /// The scene, for what Vello offers beyond Fusuma's trait (layers, filters, blurs). Anything
    /// drawn through it shows up in the next frame.
    pub fn scene_mut(&mut self) -> &mut vello_gpu::Scene {
        self.dirty = true;
        &mut self.scene
    }

    /// Fills surface `id` of the layer plan through `fill`, which is given the texture to write into (see
    /// [`TextureTarget`]), and returns the image to draw over the surface's rectangle (with `Backend::draw_source`) with its
    /// size, or `None` when `fill` said it had nothing.
    pub fn fill_texture(
        &mut self,
        id: u32,
        fill: impl FnOnce(&mut TextureTarget<'_>) -> bool,
    ) -> Option<(ImageSource, (u32, u32))> {
        let mut target = TextureTarget {
            scene: self,
            id,
            used: None,
        };
        let ok = fill(&mut target);
        let size = target.used?;
        if !ok {
            return None;
        }
        let texture = self.hosted.get(&id)?.clone();
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let tid = TextureId(self.next_texture);
        self.next_texture += 1;
        self.bindings.insert(tid, view);
        Some((
            ImageSource::external_texture(tid, TextureRegion::Full, true),
            size,
        ))
    }

    fn host_texture(&mut self, id: u32, width: u32, height: u32) -> wgpu::Texture {
        let (width, height) = (width.clamp(1, 16384), height.clamp(1, 16384));
        if let Some(t) = self.hosted.get(&id)
            && t.width() == width
            && t.height() == height
        {
            return t.clone();
        }
        let t = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fusuma surface"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.hosted.insert(id, t.clone());
        t
    }

    /// The texture that is rendered into, in `Rgba8Unorm`. It holds the last frame after
    /// [`pixels`](Scene::pixels) has been called.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Draws what was drawn since the last frame over the picture the scene keeps, and copies the whole picture into
    /// `target` (a texture of the same size and format that the host shows, such as the current texture of a window
    /// surface or a canvas), with no read-back to the CPU.
    ///
    /// The picture lives in the scene's own texture, so a frame that draws one button draws one button: the target
    /// needs no history, which is what a swap chain cannot give (a surface has no buffer age). Only the copy to the
    /// target is the whole picture, and that stays on the GPU.
    pub fn present(&mut self, target: &wgpu::Texture) {
        let (w, h) = self.size;
        let Some((mut encoder, _)) = self.recorded() else {
            return;
        };
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: u32::from(w).min(target.width()),
                height: u32::from(h).min(target.height()),
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        self.forget_drawn();
        self.dirty = false;
    }

    /// [`record`](Self::record) into a new encoder, tried again on a fresh renderer when it fails (a lost pipeline or a
    /// full pool is rebuilt). When it fails again the frame is kept, not dropped: the commands, the moves and the damage
    /// stay in the scene and are drawn with the next frame, so no part of the picture is left stale for good.
    fn recorded(&mut self) -> Option<(wgpu::CommandEncoder, (u32, u32, u32, u32))> {
        for _ in 0..2 {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fusuma frame"),
                });
            if let Some(region) = self.record(&mut encoder) {
                self.failed = 0;
                return Some((encoder, region));
            }
            self.rebuild_renderer();
        }
        self.failed = self.failed.saturating_add(1);
        None
    }

    fn rebuild_renderer(&mut self) {
        let (renderer, resources) = GpuRenderer::new(
            &self.device,
            &self.queue,
            &RenderTargetConfig {
                format: self.texture.format(),
                width: self.size.0,
                height: self.size.1,
            },
        );
        self.depth = renderer.create_depth_texture_view(&RenderSize {
            width: self.size.0,
            height: self.size.1,
        });
        self.renderer = renderer;
        self.resources = resources;
    }

    /// Records the moves of the picture and then the new commands drawn over it into `encoder`. Returns the part that
    /// matters to the CPU (x, y, width, height): what the frame named, or everything when it named nothing; `None`
    /// when the scene failed to render: the damage and moves are put back, the commands stay in the scene.
    fn record(&mut self, encoder: &mut wgpu::CommandEncoder) -> Option<(u32, u32, u32, u32)> {
        let damage = std::mem::take(&mut self.damage);
        let damage_kept = damage.clone();
        let shifts = std::mem::take(&mut self.shifts);
        let mut area: Option<(u32, u32, u32, u32)> = None;
        let mut join = |r: Option<(u32, u32, u32, u32)>| {
            if let Some((x, y, w, h)) = r {
                area = Some(area.map_or((x, y, w, h), |(ax, ay, aw, ah)| {
                    let (x0, y0) = (ax.min(x), ay.min(y));
                    let (x1, y1) = ((ax + aw).max(x + w), (ay + ah).max(y + h));
                    (x0, y0, x1 - x0, y1 - y0)
                }));
            }
        };
        for &(region, dy) in &shifts {
            let clipped = self.clip_rect(region);
            self.shift_picture(encoder, clipped, dy);
            join(clipped);
        }
        let named = !damage.is_empty();
        for r in damage {
            join(self.clip_rect(r));
        }
        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut options = RenderOptions::default();
        // The scene holds only what was drawn since the last render, drawn over the picture.
        options.target_init = TargetInit::SrcOver;
        // A scene that fails to render leaves the previous picture; the error has nowhere to go
        // through the `Renderer` trait, and the next frame tries again.
        #[cfg(test)]
        let forced = self.fail_renders > 0;
        #[cfg(test)]
        {
            self.fail_renders = self.fail_renders.saturating_sub(1);
        }
        #[cfg(not(test))]
        let forced = false;
        let rendered = self
            .renderer
            .render(
                &self.scene,
                &mut self.resources,
                encoder,
                RenderTarget {
                    view: &view,
                    depth_view: Some(&self.depth),
                },
                &self.bindings,
                options,
            )
            .ok();
        if forced || rendered.is_none() {
            self.shifts = shifts;
            self.damage = damage_kept;
            return None;
        }
        let whole = (0, 0, u32::from(self.size.0), u32::from(self.size.1));
        Some(if named {
            area.unwrap_or((0, 0, 0, 0))
        } else {
            whole
        })
    }

    /// `region` cut to the surface; `None` when nothing is left.
    fn clip_rect(&self, region: (i32, i32, u32, u32)) -> Option<(u32, u32, u32, u32)> {
        let (w, h) = (i64::from(self.size.0), i64::from(self.size.1));
        let x0 = i64::from(region.0).clamp(0, w);
        let y0 = i64::from(region.1).clamp(0, h);
        let x1 = (i64::from(region.0) + i64::from(region.2)).clamp(0, w);
        let y1 = (i64::from(region.1) + i64::from(region.3)).clamp(0, h);
        (x1 > x0 && y1 > y0).then(|| (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
    }

    /// Moves the rows of `region` down by `dy` (up when negative) in the picture. What comes in at an edge is left as it
    /// was; the frame draws it.
    fn shift_picture(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        region: Option<(u32, u32, u32, u32)>,
        dy: i32,
    ) {
        let Some((x, y, w, h)) = region else { return };
        let by = dy.unsigned_abs();
        if by == 0 || by >= h {
            return;
        }
        let rows = h - by;
        let (from, to) = if dy > 0 { (y, y + by) } else { (y + by, y) };
        let (tw, th) = (u32::from(self.size.0), u32::from(self.size.1));
        let device = &self.device;
        let scratch = self.scratch.get_or_insert_with(|| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("fusuma shift"),
                size: wgpu::Extent3d {
                    width: tw,
                    height: th,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });
        let extent = wgpu::Extent3d {
            width: w,
            height: rows,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_texture(at(&self.texture, x, from), at(scratch, x, from), extent);
        encoder.copy_texture_to_texture(at(scratch, x, from), at(&self.texture, x, to), extent);
    }

    /// Empties the scene once its commands are in the picture, keeping what the next commands are drawn with.
    fn forget_drawn(&mut self) {
        let paint = self.scene.paint().clone();
        self.scene.reset();
        // The images of the frame are in the picture; they are not needed any more.
        self.bindings = TextureBindings::new();
        // The clips still open come back, each under the transform it had.
        for (rect, at) in &self.open_clips {
            self.scene.set_transform(*at);
            self.scene.push_clip_rect(rect);
        }
        self.scene
            .set_transform(Affine::translate(self.translation));
        self.scene.set_paint_transform(self.paint_transform);
        self.scene.set_paint(paint);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn render(&mut self) {
        let Some((mut encoder, region)) = self.recorded() else {
            return;
        };
        let (w, _) = self.size;
        // Only the part that matters is copied back to the CPU.
        let (rx, ry, rw, rh) = region;
        if rw == 0 || rh == 0 {
            self.queue.submit([encoder.finish()]);
            self.forget_drawn();
            return;
        }
        let row = (rw * 4).next_multiple_of(256);
        let need = u64::from(row) * u64::from(rh);
        let buffer = match self.staging.take() {
            Some(b) if b.size() >= need => b,
            _ => self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fusuma read back"),
                size: need.next_power_of_two().max(4096),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
        };
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: rx, y: ry, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: rw,
                height: rh,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        self.forget_drawn();
        let (tx, rx_done) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        if self
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .is_err()
        {
            return;
        }
        if !matches!(rx_done.recv(), Ok(Ok(()))) {
            return;
        }
        if let Ok(mapped) = buffer.slice(..).get_mapped_range() {
            let stride = usize::from(w) * 4;
            let line = rw as usize * 4;
            for y in 0..rh as usize {
                let from = y * row as usize;
                let to = (ry as usize + y) * stride + rx as usize * 4;
                self.pixels[to..to + line].copy_from_slice(&mapped[from..from + line]);
            }
        }
        buffer.unmap();
        self.staging = Some(buffer);
    }
}

impl Scene for GpuScene {
    fn size(&self) -> (u16, u16) {
        self.size
    }

    fn reset(&mut self) {
        self.translation = (0.0, 0.0);
        self.paint_transform = Affine::IDENTITY;
        self.open_clips.clear();
        if self.failed > 0 && self.failed <= MAX_FAILED {
            // The last render failed: what it should have drawn is still in the scene, with the images, the moves and the
            // damage. The new frame is drawn after it, so nothing is lost.
            self.scene.set_transform(Affine::IDENTITY);
            self.scene.set_paint_transform(Affine::IDENTITY);
            self.dirty = true;
            return;
        }
        self.failed = 0;
        self.scene.reset();
        // The images of the forgotten frame are not needed any more.
        self.bindings = TextureBindings::new();
        self.damage.clear();
        self.shifts.clear();
        self.dirty = true;
    }

    fn set_damage(&mut self, region: (i32, i32, u32, u32)) {
        // A frame that said which parts it draws (`add_damage`) knows better than a platform's bounding box. The GPU
        // draws what was drawn wherever it is; the parts only narrow what is copied back to the CPU.
        if self.damage.is_empty() {
            self.damage.push(region);
        }
    }

    fn add_damage(&mut self, region: (i32, i32, u32, u32)) {
        self.damage.push(region);
    }

    fn shift(&mut self, region: (i32, i32, u32, u32), dy: i32) -> bool {
        // More than a few moves in one frame is not a scroll: let the caller redraw.
        if self.shifts.len() >= MAX_SHIFTS {
            return false;
        }
        self.shifts.push((region, dy));
        self.dirty = true;
        true
    }

    fn image_source(&mut self, pixmap: Arc<fusuma_render_vello_common::Pixmap>) -> ImageSource {
        let (w, h) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fusuma image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            texture.as_image_copy(),
            pixmap.data_as_u8_slice(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: None,
            },
            size,
        );
        let id = TextureId(self.next_texture);
        self.next_texture += 1;
        self.bindings.insert(
            id,
            texture.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        ImageSource::external_texture(id, TextureRegion::Full, true)
    }

    fn set_translation(&mut self, dx: f64, dy: f64) {
        self.translation = (dx, dy);
        self.scene.set_transform(Affine::translate((dx, dy)));
    }

    fn set_paint(&mut self, paint: fusuma_render_vello_common::PaintType) {
        self.scene.set_paint(paint);
    }

    fn set_paint_transform(&mut self, transform: Affine) {
        self.paint_transform = transform;
        self.scene.set_paint_transform(transform);
    }

    fn fill_rect(&mut self, rect: &KRect) {
        self.scene.fill_rect(rect);
        self.dirty = true;
    }

    fn fill_path(&mut self, path: &BezPath) {
        self.scene.set_fill_rule(Fill::EvenOdd);
        self.scene.fill_path(path);
        self.scene.set_fill_rule(Fill::NonZero);
        self.dirty = true;
    }

    fn stroke_path(&mut self, path: &BezPath, width: f64) {
        self.scene.set_stroke(stroke(width));
        self.scene.stroke_path(path);
        self.dirty = true;
    }

    fn push_clip(&mut self, rect: &KRect) {
        self.open_clips
            .push((*rect, Affine::translate(self.translation)));
        self.scene.push_clip_rect(rect);
    }

    fn pop_clip(&mut self) {
        self.open_clips.pop();
        self.scene.pop_clip();
    }

    #[cfg(feature = "text")]
    fn can_draw_glyphs(&self) -> bool {
        true
    }

    #[cfg(feature = "text")]
    fn draw_glyphs(
        &mut self,
        font: &fusuma_render_vello_common::peniko::FontData,
        size: f32,
        glyphs: &[fusuma_text::Glyph],
        origin: (f64, f64),
    ) -> bool {
        let (ox, oy) = (origin.0 as f32, origin.1 as f32);
        let placed: Vec<glifo::Glyph> = glyphs
            .iter()
            .map(|g| glifo::Glyph {
                id: g.id,
                x: g.x + ox,
                y: g.y + oy,
            })
            .collect();
        let drawn = self
            .scene
            .glyph_run(&mut self.resources, font)
            .font_size(size)
            .hint(true)
            .fill_glyphs(placed.into_iter())
            .is_ok();
        self.dirty = true;
        drawn
    }

    fn pixels(&mut self) -> &[u8] {
        // Read back on a machine that can wait for the GPU. On the web nothing can, and the picture is shown by
        // `present`: the CPU copy stays as it was.
        #[cfg(not(target_arch = "wasm32"))]
        if self.dirty {
            self.render();
            // A render that failed is tried again by the next read.
            self.dirty = self.failed > 0;
        }
        &self.pixels
    }
}

/// A renderer on a GPU surface of `width` x `height` pixels (not on the web: see [`from_device`]).
///
/// # Errors
/// [`GpuError`] when the size is bad or there is no usable GPU: use `fusuma-render-vello-cpu` then.
#[cfg(not(target_arch = "wasm32"))]
pub fn new(width: u32, height: u32) -> Result<VelloGpu, GpuError> {
    GpuScene::new(width, height).map(Backend::new)
}

/// A renderer on a device the host made (the browser's WebGPU, a window's `wgpu` device), `width` x `height`. Show a
/// frame with [`GpuScene::present`].
///
/// # Errors
/// [`GpuError::BadSize`] for a side that is zero or too big.
pub fn from_device(
    device: wgpu::Device,
    queue: wgpu::Queue,
    width: u32,
    height: u32,
) -> Result<VelloGpu, GpuError> {
    let (w, h) = GpuScene::check(width, height)?;
    Ok(Backend::new(GpuScene::with_device(device, queue, w, h)))
}

/// Whether a GPU adapter is available, for choosing between this backend and the CPU one.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn gpu_available() -> bool {
    pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }),
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fusuma_render::{Color, Point, Probe, Rect, Renderer, triangle};

    /// A renderer, or `None` (and a note) on a machine without a GPU, where the test is skipped.
    fn gpu(w: u32, h: u32) -> Option<VelloGpu> {
        match new(w, h) {
            Ok(r) => Some(r),
            Err(GpuError::NoAdapter | GpuError::NoDevice(_)) => {
                eprintln!("skipped: no usable GPU on this machine");
                None
            }
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn passes_the_shared_conformance_suite() {
        if gpu(64, 64).is_none() {
            return;
        }
        fusuma_render::conformance::run_all(|| gpu(64, 64).unwrap());
        fusuma_render::conformance::run_all(|| gpu(200, 120).unwrap());
    }

    #[test]
    fn sizes_that_cannot_exist_are_refused_before_a_gpu_is_asked() {
        assert_eq!(new(0, 10).unwrap_err(), GpuError::BadSize);
        assert_eq!(new(10, 0).unwrap_err(), GpuError::BadSize);
        assert_eq!(new(70_000, 10).unwrap_err(), GpuError::BadSize);
        assert!(!GpuError::BadSize.to_string().is_empty());
        assert!(!GpuError::NoAdapter.to_string().is_empty());
        assert!(GpuError::NoDevice("x".into()).to_string().contains('x'));
    }

    #[test]
    fn drawing_after_reading_pixels_keeps_working() {
        let Some(mut r) = gpu(16, 16) else { return };
        r.clear(Color::BLACK).unwrap();
        r.fill_rect(Rect::new(0, 0, 4, 4), 0, Color::rgb(255, 0, 0))
            .unwrap();
        assert_eq!(r.pixel(1, 1).unwrap(), Color::rgb(255, 0, 0));
        r.fill_rect(Rect::new(8, 8, 4, 4), 0, Color::rgb(0, 255, 0))
            .unwrap();
        assert_eq!(r.pixel(9, 9).unwrap(), Color::rgb(0, 255, 0));
        assert_eq!(r.pixel(1, 1).unwrap(), Color::rgb(255, 0, 0));
    }

    /// A picture drawn in pieces over frames equals the same picture drawn at once.
    fn scene(r: &mut VelloGpu, parts: &[(Rect, Color)]) {
        for (rect, color) in parts {
            r.fill_rect(*rect, 0, *color).unwrap();
        }
    }

    fn same(a: &mut VelloGpu, b: &mut VelloGpu, w: i32, h: i32) {
        for y in 0..h {
            for x in 0..w {
                assert_eq!(a.pixel(x, y), b.pixel(x, y), "({x}, {y})");
            }
        }
    }

    #[test]
    fn a_frame_that_draws_one_part_leaves_the_rest_of_the_picture() {
        let (Some(mut inc), Some(mut whole)) = (gpu(64, 64), gpu(64, 64)) else {
            return;
        };
        let bg = Color::rgb(10, 20, 30);
        let blue = Color::rgb(0, 0, 255);
        let red = Color::rgb(255, 0, 0);
        inc.clear(bg).unwrap();
        scene(&mut inc, &[(Rect::new(4, 4, 20, 20), blue)]);
        assert_eq!(inc.pixel(5, 5).unwrap(), blue);
        // The next frame draws only a small rectangle: nothing else is touched or drawn again.
        inc.with_scene(Scene::reset);
        inc.will_draw(Rect::new(40, 40, 8, 8));
        scene(&mut inc, &[(Rect::new(40, 40, 8, 8), red)]);
        whole.clear(bg).unwrap();
        scene(
            &mut whole,
            &[
                (Rect::new(4, 4, 20, 20), blue),
                (Rect::new(40, 40, 8, 8), red),
            ],
        );
        same(&mut inc, &mut whole, 64, 64);
    }

    #[test]
    fn a_render_that_fails_loses_nothing() {
        let Some(mut r) = gpu(64, 64) else { return };
        let bg = Color::rgb(10, 20, 30);
        let blue = Color::rgb(0, 0, 255);
        let red = Color::rgb(255, 0, 0);
        r.clear(bg).unwrap();
        scene(&mut r, &[(Rect::new(4, 4, 20, 20), blue)]);
        assert_eq!(r.pixel(5, 5).unwrap(), blue);
        // The first render of the next frame fails; the retry on a fresh renderer draws it.
        r.with_scene(Scene::reset);
        r.with_scene(|s| s.fail_renders = 1);
        r.will_draw(Rect::new(40, 40, 8, 8));
        scene(&mut r, &[(Rect::new(40, 40, 8, 8), red)]);
        r.with_scene(|s| assert_eq!(s.failed, 0, "the retry on a fresh renderer drew it"));
        assert_eq!(r.pixel(42, 42).unwrap(), red);
        // Fails for both attempts: the commands are kept for the next frame.
        r.with_scene(Scene::reset);
        r.will_draw(Rect::new(10, 40, 8, 8));
        r.fill_rect(Rect::new(10, 40, 8, 8), 0, blue).unwrap();
        r.with_scene(|s| {
            s.fail_renders = 2;
            s.render();
            assert_eq!(s.failed, 1);
        });
        // The next frame draws something else; the kept part comes with it.
        r.with_scene(Scene::reset);
        r.will_draw(Rect::new(30, 10, 8, 8));
        scene(&mut r, &[(Rect::new(30, 10, 8, 8), red)]);
        assert_eq!(r.pixel(12, 42).unwrap(), blue, "the lost frame is drawn");
        assert_eq!(r.pixel(32, 12).unwrap(), red);
        assert_eq!(r.pixel(5, 5).unwrap(), blue);
    }

    #[test]
    fn a_translucent_fill_is_drawn_once_over_the_kept_picture() {
        let Some(mut r) = gpu(16, 16) else { return };
        r.clear(Color::WHITE).unwrap();
        assert_eq!(r.pixel(1, 1).unwrap(), Color::WHITE);
        r.with_scene(Scene::reset);
        r.fill_rect(Rect::new(0, 0, 8, 8), 0, Color::rgba(0, 0, 0, 128))
            .unwrap();
        let once = r.pixel(1, 1).unwrap();
        // Reading again, and drawing elsewhere, does not blend it a second time.
        r.fill_rect(Rect::new(8, 8, 4, 4), 0, Color::rgb(255, 0, 0))
            .unwrap();
        assert_eq!(r.pixel(1, 1).unwrap(), once);
        assert!(once.r > 100 && once.r < 160, "{once:?}");
    }

    #[test]
    fn shifting_rows_moves_the_picture_and_leaves_the_exposed_strip() {
        let (Some(mut r), Some(mut whole)) = (gpu(32, 32), gpu(32, 32)) else {
            return;
        };
        let bg = Color::WHITE;
        let stripes = |r: &mut VelloGpu, dy: i32| {
            for k in 0..8 {
                let c = Color::rgb(k as u8 * 30, 255 - k as u8 * 30, 40);
                r.fill_rect(Rect::new(0, k * 4 + dy, 32, 4), 0, c).unwrap();
            }
        };
        r.clear(bg).unwrap();
        stripes(&mut r, 0);
        assert!(r.pixel(1, 1).is_some());
        // Move the window of rows 0..32 down by 8, then draw the strip that came in at the top.
        r.with_scene(Scene::reset);
        assert!(r.shift_area(Rect::new(0, 0, 32, 32), 8));
        r.will_draw(Rect::new(0, 0, 32, 8));
        r.fill_rect(Rect::new(0, 0, 32, 8), 0, Color::rgb(1, 2, 3))
            .unwrap();
        whole.clear(bg).unwrap();
        stripes(&mut whole, 8);
        whole
            .fill_rect(Rect::new(0, 0, 32, 8), 0, Color::rgb(1, 2, 3))
            .unwrap();
        same(&mut r, &mut whole, 32, 32);
        // And up.
        r.with_scene(Scene::reset);
        assert!(r.shift_area(Rect::new(0, 0, 32, 32), -4));
        r.will_draw(Rect::new(0, 28, 32, 4));
        r.fill_rect(Rect::new(0, 28, 32, 4), 0, Color::rgb(9, 9, 9))
            .unwrap();
        let mut c = [[Color::BLACK; 32]; 32];
        for (y, row) in c.iter_mut().enumerate().take(28) {
            for (x, px) in row.iter_mut().enumerate() {
                *px = whole.pixel(x as i32, y as i32 + 4).unwrap();
            }
        }
        for (y, row) in c.iter().enumerate() {
            for (x, want) in row.iter().enumerate() {
                let want = if y >= 28 { Color::rgb(9, 9, 9) } else { *want };
                assert_eq!(r.pixel(x as i32, y as i32).unwrap(), want, "({x}, {y})");
            }
        }
    }

    #[test]
    fn many_frames_do_not_grow_the_scene() {
        let Some(mut r) = gpu(64, 64) else { return };
        for i in 0..60 {
            r.clear(Color::BLACK).unwrap();
            r.fill_circle(Point::new(i % 60, 30), 5, Color::WHITE)
                .unwrap();
            assert_eq!(r.pixel(i % 60, 30).unwrap(), Color::WHITE);
        }
    }

    #[test]
    fn the_render_texture_is_available_to_the_host() {
        let Some(r) = gpu(32, 16) else { return };
        r.with_scene(|s| {
            assert_eq!((s.texture().width(), s.texture().height()), (32, 16));
            assert_eq!(s.texture().format(), wgpu::TextureFormat::Rgba8Unorm);
        });
    }

    #[test]
    fn the_availability_check_agrees_with_making_a_renderer() {
        assert_eq!(gpu_available(), new(8, 8).is_ok());
    }

    /// The GPU and the CPU are both Vello: the same scene must give nearly the same picture.
    #[test]
    fn the_gpu_and_the_cpu_draw_the_same_picture() {
        let Some(mut a) = gpu(180, 120) else { return };
        let mut b = fusuma_render_vello_cpu::new(180, 120).unwrap();
        fn scene<R: Renderer>(r: &mut R) {
            r.clear(Color::rgb(24, 24, 32)).unwrap();
            r.fill_rect(Rect::new(8, 8, 100, 40), 0, Color::rgb(0, 160, 160))
                .unwrap();
            r.fill_rect(Rect::new(8, 56, 60, 40), 12, Color::rgba(250, 200, 0, 160))
                .unwrap();
            r.stroke_rect(Rect::new(70, 56, 60, 40), 8, 3, Color::WHITE)
                .unwrap();
            r.fill_circle(Point::new(150, 30), 20, Color::rgb(200, 40, 40))
                .unwrap();
            r.fill_path(
                &triangle(
                    Point::new(100, 110),
                    Point::new(170, 110),
                    Point::new(135, 70),
                ),
                Color::rgb(40, 200, 80),
            )
            .unwrap();
            r.save().unwrap();
            r.translate(10, 10).unwrap();
            r.clip(Rect::new(0, 0, 50, 20)).unwrap();
            r.fill_rect(Rect::new(-20, -20, 200, 200), 0, Color::rgb(90, 90, 255))
                .unwrap();
            r.restore().unwrap();
        }
        scene(&mut a);
        scene(&mut b);
        let differing = (0..120)
            .flat_map(|y| (0..180).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let (p, q) = (a.pixel(x, y).unwrap(), b.pixel(x, y).unwrap());
                p.r.abs_diff(q.r) > 12 || p.g.abs_diff(q.g) > 12 || p.b.abs_diff(q.b) > 12
            })
            .count();
        assert!(
            differing < 180 * 120 / 100,
            "{differing} of {} pixels differ",
            180 * 120
        );
    }

    #[test]
    fn absurd_coordinates_and_images_do_not_panic_on_the_gpu() {
        use fusuma_render::{Image, TextStyle};
        let Some(mut r) = gpu(48, 40) else { return };
        r.translate(i32::MAX, i32::MIN).unwrap();
        r.fill_rect(
            Rect::new(i32::MAX, i32::MIN, u32::MAX, u32::MAX),
            u32::MAX,
            Color::WHITE,
        )
        .unwrap();
        r.stroke_rect(Rect::new(0, 0, 9, 9), 4, u32::MAX, Color::WHITE)
            .unwrap();
        r.fill_circle(Point::new(i32::MAX, i32::MIN), u32::MAX, Color::WHITE)
            .unwrap();
        r.draw_text(
            "x",
            Point::new(i32::MIN, i32::MAX),
            &TextStyle::new(10, Color::WHITE),
        )
        .unwrap();
        let px = [255u8; 16];
        let img = Image::new(2, 2, &px).unwrap();
        r.draw_image(&img, Point::new(i32::MAX, i32::MAX)).unwrap();
        r.translate(i32::MAX, i32::MIN).unwrap();
        r.draw_image(&img, Point::new(i32::MAX - 1, i32::MAX - 1))
            .unwrap();
        assert_eq!(r.pixel(-1, 0), None);
        assert!(r.pixel(0, 0).is_some());
    }

    #[test]
    fn images_survive_a_clear_and_are_uploaded_again() {
        use fusuma_render::Image;
        let Some(mut r) = gpu(16, 16) else { return };
        let data = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let img = Image::new(2, 2, &data).unwrap();
        for _ in 0..3 {
            r.clear(Color::BLACK).unwrap();
            r.draw_image(&img, Point::new(4, 4)).unwrap();
            assert_eq!(r.pixel(4, 4).unwrap(), Color::rgb(255, 0, 0));
            assert_eq!(r.pixel(5, 4).unwrap(), Color::rgb(0, 255, 0));
            assert_eq!(r.pixel(4, 5).unwrap(), Color::rgb(0, 0, 255));
        }
    }

    /// The GPU draws vector text through `glyph_run` like the CPU does. Skipped where there is no GPU.
    #[cfg(feature = "text")]
    #[test]
    fn vector_text_draws_on_the_gpu_and_matches_the_cpu() {
        use fusuma_render::{Color, Font, Point, Probe, Renderer, TextStyle};
        let Ok(mut gpu) = new(200, 50) else {
            eprintln!("no GPU here, skipped");
            return;
        };
        let mut cpu = fusuma_render_vello_cpu::new(200, 50).unwrap();
        let style = TextStyle::new(24, Color::WHITE).font(Font::Sans);
        gpu.clear(Color::BLACK).unwrap();
        gpu.draw_text("Hello Ășț", Point::new(4, 4), &style)
            .unwrap();
        cpu.clear(Color::BLACK).unwrap();
        cpu.draw_text("Hello Ășț", Point::new(4, 4), &style)
            .unwrap();
        assert_eq!(
            gpu.measure_text("Hello Ășț", &style),
            cpu.measure_text("Hello Ășț", &style)
        );
        let (mut lit, mut far) = (0, 0);
        for y in 0..50 {
            for x in 0..200 {
                let (a, b) = (gpu.pixel(x, y).unwrap(), cpu.pixel(x, y).unwrap());
                lit += usize::from(a != Color::BLACK);
                let d = |p: u8, q: u8| p.abs_diff(q) > 60;
                far += usize::from(d(a.r, b.r) || d(a.g, b.g) || d(a.b, b.b));
            }
        }
        assert!(lit > 80, "the GPU drew text: {lit} pixels");
        assert!(
            far * 20 < lit,
            "the two pictures agree: {far} of {lit} pixels differ clearly"
        );
    }

    /// Showing a frame by copying it into a texture of the host's gives the picture that reading it back gives.
    #[test]
    fn a_frame_presented_to_a_texture_is_the_frame_read_back() {
        let Some(mut r) = gpu(40, 24) else { return };
        r.clear(Color::rgb(10, 20, 30)).unwrap();
        r.fill_rect(Rect::new(4, 4, 12, 8), 0, Color::rgb(250, 0, 0))
            .unwrap();
        r.fill_circle(Point::new(28, 14), 6, Color::rgb(0, 200, 0))
            .unwrap();
        let read_back = r.with_scene(|s| s.pixels().to_vec());
        let shown = r.with_scene(|s| {
            let target = s.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("target"),
                size: wgpu::Extent3d {
                    width: 40,
                    height: 24,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            s.present(&target);
            // Read the target back by hand.
            let row = (40u32 * 4).next_multiple_of(256);
            let buffer = s.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("check"),
                size: u64::from(row) * 24,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut enc = s
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            enc.copy_texture_to_buffer(
                target.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d {
                    width: 40,
                    height: 24,
                    depth_or_array_layers: 1,
                },
            );
            s.queue.submit([enc.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            s.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = buffer.slice(..).get_mapped_range().unwrap();
            let mut out = Vec::new();
            for y in 0..24usize {
                out.extend_from_slice(&mapped[y * row as usize..y * row as usize + 160]);
            }
            out
        });
        assert_eq!(shown, read_back, "the same pixels");
        assert_eq!(&shown[(4 * 40 + 4) * 4..(4 * 40 + 4) * 4 + 3], &[250, 0, 0]);
    }
}
