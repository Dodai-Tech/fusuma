#![doc = include_str!("../README.md")]

mod canvas;
mod input;
mod plan;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use fusuma::platform::{Backend, BackendSetting, RenderLayer};
use fusuma::platform::{Plan, Render};
use fusuma::{App, Input, Pointer, Shell};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

pub use canvas::{Canvas, blit};
pub use plan::{LINKED, hardware, plan_for};

/// What the application chooses about the window and the renderer. The default is the best choice.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The window title.
    pub title: &'static str,
    /// The first size of the drawing area, in physical pixels.
    pub size: (u32, u32),
    /// The text of the project's settings file (`fusuma.toml`), usually `Some(include_str!("../fusuma.toml"))`:
    /// the middle layer of the renderer choice.
    pub settings: Option<&'static str>,
    /// The build's layer, the strongest: a command-line flag or what the program insists on.
    pub build: RenderLayer,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            title: "Fusuma",
            size: (1024, 600),
            settings: None,
            build: RenderLayer::EMPTY,
        }
    }
}

/// How often a frame is run while the app has an animation going, in milliseconds (about 60 a second).
const FRAME_MS: u64 = 16;

/// When the event loop should wake next: in a frame's time while the app has more to do, and not until the
/// next event otherwise, so an idle window costs no wake-ups.
#[must_use]
fn next_wake(wants_frame: bool, now: Instant) -> ControlFlow {
    if wants_frame {
        ControlFlow::WaitUntil(now + Duration::from_millis(FRAME_MS))
    } else {
        ControlFlow::Wait
    }
}

/// Opens a window and runs `make(width, height)` in it until it is closed.
///
/// # Errors
/// When the settings file is not valid, there is no window system, or no renderer can be made.
pub fn run<A: App + 'static>(
    make: impl FnOnce(u32, u32) -> A + 'static,
    options: Options,
) -> anyhow::Result<()> {
    let file = match options.settings {
        Some(text) => RenderLayer::from_toml(text)
            .map_err(|e| anyhow::anyhow!("fusuma.toml is not valid: {e}"))?,
        None => RenderLayer::EMPTY,
    };
    let layers = [file, options.build];
    let event_loop = EventLoop::new()?;
    let mut handler = Handler {
        make: Some(Box::new(make)),
        options,
        layers,
        state: None,
        error: None,
    };
    event_loop.run_app(&mut handler)?;
    handler.error.map_or(Ok(()), Err)
}

/// Everything that exists once the window does.
struct State<A> {
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    canvas: Canvas,
    shell: Shell<A>,
    started: Instant,
    cursor: Option<fusuma::render::Point>,
    shift: bool,
    /// The window must be shown again even if nothing was redrawn (it was exposed, or resized).
    show: bool,
    /// The loop was asleep: the time asleep is not time the app lived through.
    idle: bool,
}

struct Handler<A> {
    make: Option<Box<dyn FnOnce(u32, u32) -> A>>,
    options: Options,
    layers: [RenderLayer; 2],
    state: Option<State<A>>,
    error: Option<anyhow::Error>,
}

/// A canvas for `plan`, stepping down to the CPU when the GPU surface cannot be made.
fn canvas_for(plan: &Plan, w: u32, h: u32) -> Option<Canvas> {
    Canvas::new(plan.uses, w, h).or_else(|| {
        (plan.uses != Backend::VelloCpu).then(|| {
            log::warn!("{} failed, using vello-cpu", plan.uses.name());
            Canvas::new(Backend::VelloCpu, w, h)
        })?
    })
}

impl<A: App> Handler<A> {
    fn start(&mut self, el: &ActiveEventLoop) -> anyhow::Result<State<A>> {
        let attrs = Window::default_attributes()
            .with_title(self.options.title)
            .with_inner_size(PhysicalSize::new(self.options.size.0, self.options.size.1));
        let window = Arc::new(el.create_window(attrs)?);
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let gpu = fusuma::renderers::vello_gpu::gpu_available();
        let render = Render::layered(&self.layers);
        let plan = plan::plan_for(w, h, gpu, &self.layers)
            .ok_or_else(|| anyhow::anyhow!("no renderer for a {w}x{h} window"))?;
        log::info!(
            "{w}x{h} window, gpu {gpu}; settings: fallback {}, keep {} KB free",
            render.fallback,
            render.min_free_kb
        );
        log::info!("renderer: {plan}");
        let canvas = canvas_for(&plan, w, h)
            .ok_or_else(|| anyhow::anyhow!("cannot make a {w}x{h} surface"))?;
        log::info!("drawing with {}", canvas.backend().name());
        let context = softbuffer::Context::new(window.clone())
            .map_err(|e| anyhow::anyhow!("no display context: {e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("no window surface: {e}"))?;
        let make = self
            .make
            .take()
            .ok_or_else(|| anyhow::anyhow!("the app was already made"))?;
        let mut shell = Shell::new(make(w, h));
        shell.input(Input::Resize {
            width: w,
            height: h,
            scale_percent: input::scale_percent(window.scale_factor()),
        });
        Ok(State {
            window,
            surface,
            canvas,
            shell,
            started: Instant::now(),
            cursor: None,
            shift: false,
            show: true,
            idle: false,
        })
    }

    fn fail(&mut self, el: &ActiveEventLoop, e: anyhow::Error) {
        self.error = Some(e);
        el.exit();
    }
}

impl<A: App> State<A> {
    fn pointer(&mut self, p: Pointer) {
        self.shell.input(Input::Pointer(p));
    }

    /// The window changed size: a new surface of that size, and the app lays itself out again.
    fn resize(&mut self, size: PhysicalSize<u32>, layers: &[RenderLayer; 2]) {
        let (w, h) = (size.width, size.height);
        if w == 0 || h == 0 || (w, h) == self.canvas.size() {
            return;
        }
        let gpu = self.canvas.backend() == Backend::VelloGpu;
        let Some(plan) = plan::plan_for(w, h, gpu, layers) else {
            return;
        };
        match canvas_for(&plan, w, h) {
            Some(canvas) => self.canvas = canvas,
            None => {
                log::warn!("cannot make a {w}x{h} surface, keeping the old one");
                return;
            }
        }
        self.shell.input(Input::Resize {
            width: w,
            height: h,
            scale_percent: input::scale_percent(self.window.scale_factor()),
        });
        self.show = true;
    }

    /// One frame; shows it if anything was redrawn or `force` says the window needs the picture again.
    fn step(&mut self, force: bool) -> anyhow::Result<()> {
        let now = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if self.idle {
            self.shell.resume(now);
        }
        let redrawn = self.canvas.frame(&mut self.shell, now).is_some();
        self.idle = !self.shell.wants_frame();
        if !(redrawn || force || self.show) {
            return Ok(());
        }
        self.show = false;
        let (w, h) = self.canvas.size();
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return Ok(());
        };
        self.surface
            .resize(nw, nh)
            .map_err(|e| anyhow::anyhow!("cannot resize the window surface: {e}"))?;
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| anyhow::anyhow!("cannot get the window buffer: {e}"))?;
        let n = buffer.len().min(self.canvas.pixels().len());
        buffer[..n].copy_from_slice(&self.canvas.pixels()[..n]);
        buffer
            .present()
            .map_err(|e| anyhow::anyhow!("cannot show the frame: {e}"))
    }
}

impl<A: App> ApplicationHandler for Handler<A> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        match self.start(el) {
            Ok(s) => self.state = Some(s),
            Err(e) => self.fail(el, e),
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let layers = self.layers;
        let Some(st) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => st.resize(size, &layers),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let (width, height) = st.canvas.size();
                st.shell.input(Input::Resize {
                    width,
                    height,
                    scale_percent: input::scale_percent(scale_factor),
                });
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = input::point(position);
                st.cursor = Some(p);
                st.pointer(Pointer::Move(p));
            }
            WindowEvent::CursorLeft { .. } => {
                st.cursor = None;
                st.pointer(Pointer::Leave);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(p) = st.cursor {
                    st.pointer(match state {
                        ElementState::Pressed => Pointer::Down(p),
                        ElementState::Released => Pointer::Up(p),
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(p) = st.cursor {
                    st.pointer(Pointer::Wheel(p, input::wheel(delta)));
                }
            }
            WindowEvent::Touch(t) => {
                let at = input::point(t.location);
                for e in input::touch(t.phase, at).into_iter().flatten() {
                    st.pointer(e);
                }
            }
            WindowEvent::ModifiersChanged(m) => st.shift = m.state().shift_key(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(k) = input::map_key(&event.logical_key, st.shift) {
                    st.shell.input(Input::Key(k));
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = st.step(true) {
                    self.fail(el, e);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(st) = self.state.as_mut() else {
            return;
        };
        if st.shell.wants_frame()
            && let Err(e) = st.step(false)
        {
            self.fail(el, e);
            return;
        }
        let wants = st.shell.wants_frame();
        el.set_control_flow(next_wake(wants, Instant::now()));
    }
}

#[cfg(test)]
mod wake_tests {
    use super::*;

    #[test]
    fn an_app_with_nothing_to_do_sleeps_until_an_event_and_one_animating_wakes_in_a_frame() {
        let now = Instant::now();
        assert_eq!(next_wake(false, now), ControlFlow::Wait);
        match next_wake(true, now) {
            ControlFlow::WaitUntil(t) => {
                let ms = t.duration_since(now).as_millis();
                assert!((10..=20).contains(&ms), "about a frame: {ms} ms");
            }
            other => panic!("expected a timed wake, got {other:?}"),
        }
    }
}
