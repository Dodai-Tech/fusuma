#![doc = include_str!("../README.md")]

mod canvas;
mod clip;
mod input;
mod plan;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fusuma::fonts::FontSet;
pub use fusuma::platform::{Backend, BackendSetting, RenderLayer};
use fusuma::platform::{FontChoice, Plan, Render, Settings, ThemeChoice};
use fusuma::{App, Input, Pointer, Shell};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

pub use canvas::{Canvas, blit};
pub use clip::{Clipboard, SystemClipboard, run_shortcut};
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
    let settings = match options.settings {
        Some(text) => Settings::from_toml(text)
            .map_err(|e| anyhow::anyhow!("fusuma.toml is not valid: {e}"))?,
        None => Settings::default(),
    };
    let fonts = font_set(&settings.fonts)?;
    let palette = palette_for(&settings.theme)?;
    let layers = [settings.render, options.build];
    let event_loop = EventLoop::new()?;
    let mut handler = Handler {
        make: Some(Box::new(make)),
        options,
        layers,
        fonts,
        palette,
        state: None,
        error: None,
    };
    event_loop.run_app(&mut handler)?;
    handler.error.map_or(Ok(()), Err)
}

/// Everything that exists once the window does.
struct State<A> {
    /// The font families every canvas draws with.
    fonts: FontSet,
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    canvas: Canvas,
    shell: Shell<A>,
    started: Instant,
    cursor: Option<fusuma::render::Point>,
    shift: bool,
    /// Ctrl, or Cmd on a Mac: the key of shortcuts.
    command: bool,
    clipboard: SystemClipboard,
    /// Whether an input method is on, and where its candidate window was last put.
    ime_on: bool,
    ime_area: Option<fusuma::render::Rect>,
    /// The window must be shown again even if nothing was redrawn (it was exposed, or resized).
    show: bool,
    /// The loop was asleep: the time asleep is not time the app lived through.
    idle: bool,
}

struct Handler<A> {
    make: Option<Box<dyn FnOnce(u32, u32) -> A>>,
    options: Options,
    layers: [RenderLayer; 2],
    fonts: FontSet,
    /// The colours `[theme]` asks for, sent to the app before its first frame.
    palette: fusuma::Palette,
    state: Option<State<A>>,
    error: Option<anyhow::Error>,
}

/// A canvas for `plan` drawing with `fonts`, stepping down to the CPU when the GPU surface cannot be made.
fn canvas_for(plan: &Plan, w: u32, h: u32, fonts: FontSet) -> Option<Canvas> {
    Canvas::new(plan.uses, w, h, fonts).or_else(|| {
        (plan.uses != Backend::VelloCpu).then(|| {
            log::warn!("{} failed, using vello-cpu", plan.uses.name());
            Canvas::new(Backend::VelloCpu, w, h, fonts)
        })?
    })
}

/// The palette `[theme]` asks for, with its accent.
fn palette_for(choice: &ThemeChoice) -> anyhow::Result<fusuma::Palette> {
    let p = fusuma::Palette::choose(choice.palette.as_deref(), choice.accent.as_deref())
        .map_err(|e| anyhow::anyhow!("fusuma.toml [theme]: {e}"))?;
    log::info!(
        "palette: {}{}",
        p.name,
        choice
            .accent
            .as_deref()
            .map_or(String::new(), |a| format!(" with the {a} accent"))
    );
    Ok(p)
}

/// The font families the `[fonts]` table asks for, among those the build linked.
fn font_set(choice: &FontChoice) -> anyhow::Result<FontSet> {
    let set = FontSet::choose(
        choice.preset.as_deref(),
        choice.sans.as_deref(),
        choice.mono.as_deref(),
    )
    .map_err(|e| anyhow::anyhow!("fusuma.toml [fonts]: {e}"))?;
    log::info!(
        "fonts: sans {}, mono {}",
        set.sans.map_or("the bitmap font", |f| f.name),
        set.mono.map_or("the bitmap font", |f| f.name)
    );
    Ok(set)
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
        let canvas = canvas_for(&plan, w, h, self.fonts)
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
        shell.input(Input::Palette(self.palette));
        shell.input(Input::Resize {
            width: w,
            height: h,
            scale_percent: input::scale_percent(window.scale_factor()),
        });
        Ok(State {
            fonts: self.fonts,
            window,
            surface,
            canvas,
            shell,
            started: Instant::now(),
            cursor: None,
            shift: false,
            command: false,
            clipboard: SystemClipboard::new(),
            ime_on: false,
            ime_area: None,
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

    /// Keeps the input method in step with the app: on while a text field has the focus, with its candidate
    /// window next to the caret.
    fn sync_ime(&mut self) {
        let wants = self.shell.app.wants_text();
        if wants != self.ime_on {
            self.window.set_ime_allowed(wants);
            self.ime_on = wants;
            self.ime_area = None;
        }
        if wants
            && let Some(c) = self.shell.app.text_cursor()
            && self.ime_area != Some(c)
        {
            self.window.set_ime_cursor_area(
                PhysicalPosition::new(c.x, c.y),
                PhysicalSize::new(c.w.max(1), c.h.max(1)),
            );
            self.ime_area = Some(c);
        }
    }

    /// A key press: a clipboard shortcut, an edit command or typed text when a text field has the focus, a
    /// toolkit key otherwise.
    fn key(&mut self, event: &winit::event::KeyEvent) {
        let wants = self.shell.app.wants_text();
        if let Some(s) = input::shortcut(&event.logical_key, self.command) {
            clip::run_shortcut(s, &mut self.shell, &mut self.clipboard);
            return;
        }
        if wants && let Some(e) = input::map_edit(&event.logical_key, self.shift, self.command) {
            self.shell.input(Input::Edit(e));
            return;
        }
        if let Some(text) = input::typed_text(event.text.as_deref(), self.command) {
            self.shell.inputs(fusuma::text_inputs(text));
            return;
        }
        if let Some(k) = input::map_key(&event.logical_key, self.shift) {
            self.shell.input(Input::Key(k));
        }
    }

    /// An input method event: a composition in progress, or text it commits.
    fn ime(&mut self, ime: Ime) {
        let clear = Input::Preedit(fusuma::InputText::EMPTY);
        match ime {
            Ime::Preedit(text, _) => self
                .shell
                .input(Input::Preedit(fusuma::InputText::new(&text))),
            Ime::Commit(text) => {
                self.shell.input(clear);
                self.shell.inputs(fusuma::text_inputs(&text));
            }
            Ime::Enabled | Ime::Disabled => self.shell.input(clear),
        }
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
        match canvas_for(&plan, w, h, self.fonts) {
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
        self.sync_ime();
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
            WindowEvent::ModifiersChanged(m) => {
                st.shift = m.state().shift_key();
                st.command = m.state().control_key() || m.state().super_key();
            }
            WindowEvent::Ime(ime) => st.ime(ime),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                st.key(&event);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_theme_table_picks_a_palette_and_names_a_mistake() {
        assert_eq!(
            palette_for(&ThemeChoice::default()).unwrap(),
            fusuma::Palette::FUSUMA
        );
        let forest = ThemeChoice {
            palette: Some("forest".into()),
            accent: None,
        };
        assert_eq!(palette_for(&forest).unwrap(), fusuma::Palette::FOREST);
        let bad = ThemeChoice {
            palette: None,
            accent: Some("red".into()),
        };
        let e = palette_for(&bad).unwrap_err().to_string();
        assert!(e.contains("[theme]") && e.contains("'red'"), "{e}");
    }

    #[test]
    fn the_fonts_table_picks_linked_families_and_names_a_mistake() {
        let set = font_set(&FontChoice::default()).unwrap();
        assert_eq!(set, FontSet::default_set());
        let bad = FontChoice {
            preset: Some("nope".into()),
            ..FontChoice::default()
        };
        let e = font_set(&bad).unwrap_err().to_string();
        assert!(e.contains("[fonts]") && e.contains("'nope'"), "{e}");
        let mono_as_sans = FontChoice {
            sans: Some("jetbrains-mono".into()),
            ..FontChoice::default()
        };
        assert!(font_set(&mono_as_sans).is_err());
    }
}
