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
    Resources, TextureBindings,
};
use vello_gpu::{TextureId, TextureRegion};

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
    dirty: bool,
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
            dirty: true,
            bindings: TextureBindings::new(),
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

    /// The texture that is rendered into, in `Rgba8Unorm`. It holds the last frame after
    /// [`pixels`](Scene::pixels) has been called.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Renders the scene and copies the picture into `target` (a texture of the same size and format that the host
    /// shows, such as the current texture of a window surface or a canvas), with no read-back to the CPU. The
    /// whole scene is drawn: a frame that draws only part of the screen would leave the rest empty.
    pub fn present(&mut self, target: &wgpu::Texture) {
        let (w, h) = self.size;
        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fusuma present"),
            });
        let rendered = self.renderer.render(
            &self.scene,
            &mut self.resources,
            &mut encoder,
            RenderTarget {
                view: &view,
                depth_view: Some(&self.depth),
            },
            &self.bindings,
            RenderOptions::default(),
        );
        if rendered.is_err() {
            return;
        }
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
        self.dirty = false;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn render(&mut self) {
        let (w, h) = self.size;
        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fusuma render"),
            });
        // A scene that fails to render leaves the previous picture; the error has nowhere to go
        // through the `Renderer` trait, and the next frame tries again.
        let rendered = self.renderer.render(
            &self.scene,
            &mut self.resources,
            &mut encoder,
            RenderTarget {
                view: &view,
                depth_view: Some(&self.depth),
            },
            &self.bindings,
            RenderOptions::default(),
        );
        if rendered.is_err() {
            return;
        }
        let row = (u32::from(w) * 4).next_multiple_of(256);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fusuma read back"),
            size: u64::from(row) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
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
                width: u32::from(w),
                height: u32::from(h),
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
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
        if !matches!(rx.recv(), Ok(Ok(()))) {
            return;
        }
        if let Ok(mapped) = buffer.slice(..).get_mapped_range() {
            let line = usize::from(w) * 4;
            for (y, dst) in self.pixels.chunks_exact_mut(line).enumerate() {
                let start = y * row as usize;
                dst.copy_from_slice(&mapped[start..start + line]);
            }
        }
        buffer.unmap();
    }
}

impl Scene for GpuScene {
    fn size(&self) -> (u16, u16) {
        self.size
    }

    fn retains_picture(&self) -> bool {
        false
    }

    fn reset(&mut self) {
        self.scene.reset();
        // The images of the forgotten frame are not needed any more.
        self.bindings = TextureBindings::new();
        self.dirty = true;
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
        self.scene.set_transform(Affine::translate((dx, dy)));
    }

    fn set_paint(&mut self, paint: fusuma_render_vello_common::PaintType) {
        self.scene.set_paint(paint);
    }

    fn set_paint_transform(&mut self, transform: Affine) {
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
        self.scene.push_clip_rect(rect);
    }

    fn pop_clip(&mut self) {
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
            self.dirty = false;
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
