//! The Fusuma components showcase in a web page.
//!
//! One `<canvas id="fusuma">` and nothing else on the page: the app is `fusuma-showcase`, the same code the window,
//! the terminal and the watch run. Where the browser has WebGPU, Vello draws on the GPU and shows each frame on the
//! canvas without reading it back; otherwise (or with `?renderer=cpu` in the address) Vello draws on the CPU into
//! WebAssembly memory and the changed rectangle is copied to the canvas. Pointer, touch, wheel and keys come from the
//! page's events; a hidden `<textarea>` that follows the focused text field takes typing, composition (an input
//! method) and a phone's keyboard.

// The crate is WebAssembly only: on any other target it is empty, so the workspace builds everywhere.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;

use fusuma::platform::Settings;
use fusuma::render::Point;
use fusuma::renderers::vello_cpu::{self, Scene as _, VelloCpu};
use fusuma::renderers::vello_gpu::{self, VelloGpu};
use fusuma::{App, Edit, Input, InputText, Key, Pointer, Shell};
use fusuma_showcase::Showcase;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{
    CanvasRenderingContext2d, ClipboardEvent, CompositionEvent, Document, HtmlCanvasElement,
    HtmlTextAreaElement, ImageData, KeyboardEvent, MouseEvent, TouchEvent, WheelEvent, Window,
};

/// The settings of this build: fonts and palette, as for every other host.
const SETTINGS: &str = include_str!("../fusuma.toml");

/// Rows of text a wheel notch in lines mode scrolls, in CSS pixels.
const LINE_PX: f64 = 16.0;

/// What draws and shows the picture.
enum Gfx {
    /// Vello on the CPU, copied to a 2D canvas a rectangle at a time.
    Cpu(Box<Cpu>),
    /// Vello on WebGPU, shown through the canvas's own surface.
    Gpu(Box<Gpu>),
}

/// The CPU side: a Vello surface and the canvas's 2D context.
struct Cpu {
    renderer: VelloCpu,
    ctx: CanvasRenderingContext2d,
}

/// The WebGPU side: a renderer on the device, and the canvas surface it presents to.
struct Gpu {
    renderer: VelloGpu,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

struct State {
    shell: Shell<Showcase>,
    gfx: Gfx,
    canvas: HtmlCanvasElement,
    textarea: HtmlTextAreaElement,
    fonts: fusuma::fonts::FontSet,
    /// Device pixels a CSS pixel is.
    dpr: f64,
    /// The size of the surface in device pixels.
    size: (u32, u32),
    /// An input method is composing.
    composing: bool,
    /// The page asked for a frame that has not run yet.
    scheduled: bool,
    /// Whether the text area has the focus on behalf of a text field.
    typing: bool,
}

type Shared = Rc<RefCell<State>>;

fn window() -> Window {
    web_sys::window().expect("a window")
}

fn document() -> Document {
    window().document().expect("a document")
}

fn now_ms() -> f64 {
    window().performance().map_or(0.0, |p| p.now())
}

/// The size the canvas should have: the window, in device pixels.
fn wanted_size(dpr: f64) -> (u32, u32) {
    let w = window();
    let css = |v: Result<JsValue, JsValue>| v.ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
    let (cw, ch) = (css(w.inner_width()), css(w.inner_height()));
    (
        ((cw * dpr).round() as u32).clamp(64, 8192),
        ((ch * dpr).round() as u32).clamp(64, 8192),
    )
}

/// Asks the page for a frame, once.
fn schedule(state: &Shared) {
    if state.borrow().scheduled {
        return;
    }
    state.borrow_mut().scheduled = true;
    let st = state.clone();
    let cb = Closure::once_into_js(move |_t: f64| frame(&st));
    let _ = window().request_animation_frame(cb.unchecked_ref());
}

/// One frame: time passes in the app, what changed is drawn, and it is shown on the canvas.
fn frame(state: &Shared) {
    let again = {
        let mut guard = state.borrow_mut();
        let s = &mut *guard;
        s.scheduled = false;
        let (w, h) = s.size;
        let now = now_ms() as u64;
        match &mut s.gfx {
            Gfx::Cpu(cpu) => {
                let Cpu { renderer, ctx } = &mut **cpu;
                // The scene forgets what it drew before; this frame redraws only the damaged part.
                renderer.with_scene(vello_cpu::Scene::reset);
                let drawn = s.shell.frame(now, renderer);
                if let Ok(Some(r)) = drawn {
                    let region = r.intersect(&fusuma::render::Rect::new(0, 0, w, h));
                    if region.w > 0 && region.h > 0 {
                        renderer.with_scene(|scene| {
                            vello_cpu::Scene::set_damage(
                                scene,
                                (region.x, region.y, region.w, region.h),
                            );
                            show(ctx, scene.pixels(), w, region);
                        });
                    }
                }
            }
            Gfx::Gpu(gpu) => {
                // The GPU draws the whole picture each frame (its target is cleared), so ask for all of it.
                s.shell.app.invalidate();
                gpu.renderer.with_scene(vello_gpu::Scene::reset);
                if matches!(s.shell.frame(now, &mut gpu.renderer), Ok(Some(_))) {
                    use wgpu::CurrentSurfaceTexture::{Suboptimal, Success};
                    match gpu.surface.get_current_texture() {
                        Success(t) | Suboptimal(t) => {
                            gpu.renderer.with_scene(|scene| scene.present(&t.texture));
                            gpu.queue.present(t);
                        }
                        _ => {}
                    }
                }
            }
        }
        sync_text_area(s);
        s.shell.wants_frame()
    };
    if again {
        schedule(state);
    }
}

/// Copies `region` of the RGBA picture `rgba` (`width` pixels a row) onto the canvas.
fn show(ctx: &CanvasRenderingContext2d, rgba: &[u8], width: u32, region: fusuma::render::Rect) {
    let (x, y) = (region.x.max(0) as usize, region.y.max(0) as usize);
    let (w, h) = (region.w as usize, region.h as usize);
    let stride = width as usize * 4;
    let mut out = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        let start = row * stride + x * 4;
        match rgba.get(start..start + w * 4) {
            Some(px) => out.extend_from_slice(px),
            None => return,
        }
    }
    if let Ok(img) = ImageData::new_with_u8_clamped_array_and_sh(Clamped(&out), w as u32, h as u32)
    {
        let _ = ctx.put_image_data(&img, x as f64, y as f64);
    }
}

/// The hidden text area follows the focused text field: it has the focus while one has, and sits at its caret so an
/// input method's candidate window and a phone's keyboard appear there.
fn sync_text_area(s: &mut State) {
    let wants = s.shell.app.wants_text();
    if wants {
        if let Some(c) = s.shell.app.text_cursor() {
            let css = |v: i32| f64::from(v) / s.dpr;
            let style = s.textarea.style();
            let _ = style.set_property("left", &format!("{}px", css(c.x)));
            let _ = style.set_property("top", &format!("{}px", css(c.y)));
            let _ = style.set_property(
                "height",
                &format!("{}px", (f64::from(c.h) / s.dpr).max(16.0)),
            );
        }
        let active = document().active_element();
        let ours: &web_sys::Element = s.textarea.as_ref();
        if active.as_ref() != Some(ours) {
            let _ = s.textarea.focus();
        }
    } else if s.typing {
        let _ = s.textarea.blur();
        let _ = s.canvas.focus();
        s.composing = false;
    }
    s.typing = wants;
}

/// Hands an input to the app and asks for a frame.
fn send(state: &Shared, input: Input) {
    state.borrow_mut().shell.input(input);
    schedule(state);
}

/// The pointer's position in the canvas, in device pixels.
fn at(state: &Shared, client_x: f64, client_y: f64) -> Point {
    let s = state.borrow();
    let r = s.canvas.get_bounding_client_rect();
    Point::new(
        ((client_x - r.left()) * s.dpr).round() as i32,
        ((client_y - r.top()) * s.dpr).round() as i32,
    )
}

fn listen<E: JsCast + wasm_bindgen::convert::FromWasmAbi + 'static>(
    target: &web_sys::EventTarget,
    name: &str,
    f: impl FnMut(E) + 'static,
) {
    let cb = Closure::<dyn FnMut(E)>::new(f);
    let _ = target.add_event_listener_with_callback(name, cb.as_ref().unchecked_ref());
    // The page lives as long as the program.
    cb.forget();
}

/// A key as input, or `None` for what the app does not use. `wants_text`: a text field has the focus, so typing
/// arrives through the text area and the edit keys edit.
fn key_input(ev: &KeyboardEvent, wants_text: bool) -> Option<Input> {
    let (shift, ctrl) = (ev.shift_key(), ev.ctrl_key() || ev.meta_key());
    let key = ev.key();
    let edit = |e: Edit| wants_text.then_some(Input::Edit(e));
    match key.as_str() {
        "Tab" => Some(Input::Key(if shift { Key::Prev } else { Key::Next })),
        "Enter" => Some(Input::Key(Key::Activate)),
        "Escape" => Some(Input::Key(Key::Cancel)),
        "ArrowUp" => Some(Input::Key(Key::Up)),
        "ArrowDown" => Some(Input::Key(Key::Down)),
        "ArrowLeft" if wants_text && ctrl => edit(Edit::WordLeft(shift)),
        "ArrowRight" if wants_text && ctrl => edit(Edit::WordRight(shift)),
        "ArrowLeft" if wants_text => edit(Edit::Left(shift)),
        "ArrowRight" if wants_text => edit(Edit::Right(shift)),
        "ArrowLeft" => Some(Input::Key(Key::Left)),
        "ArrowRight" => Some(Input::Key(Key::Right)),
        "Backspace" => edit(Edit::Backspace),
        "Delete" => edit(Edit::Delete),
        "Home" => edit(Edit::Home(shift)),
        "End" => edit(Edit::End(shift)),
        k if ctrl && wants_text && k.eq_ignore_ascii_case("a") => edit(Edit::SelectAll),
        k if ctrl && wants_text && k.eq_ignore_ascii_case("z") => {
            edit(if shift { Edit::Redo } else { Edit::Undo })
        }
        k if ctrl && wants_text && k.eq_ignore_ascii_case("y") => edit(Edit::Redo),
        // Space is text: a field types it, anything else is pressed by it (the toolkit decides).
        " " if !wants_text => Some(Input::Text(InputText::new(" "))),
        _ => None,
    }
}

fn install(state: &Shared) {
    let canvas = state.borrow().canvas.clone();
    let target: &web_sys::EventTarget = canvas.as_ref();

    // Mouse.
    let st = state.clone();
    listen(target, "mousedown", move |e: MouseEvent| {
        if e.button() == 0 {
            let p = at(&st, f64::from(e.client_x()), f64::from(e.client_y()));
            send(&st, Input::Pointer(Pointer::Down(p)));
            e.prevent_default();
            let _ = st.borrow().canvas.focus();
        }
    });
    let st = state.clone();
    listen(target, "mousemove", move |e: MouseEvent| {
        let p = at(&st, f64::from(e.client_x()), f64::from(e.client_y()));
        send(&st, Input::Pointer(Pointer::Move(p)));
    });
    let st = state.clone();
    listen(target, "mouseup", move |e: MouseEvent| {
        if e.button() == 0 {
            let p = at(&st, f64::from(e.client_x()), f64::from(e.client_y()));
            send(&st, Input::Pointer(Pointer::Up(p)));
        }
    });
    let st = state.clone();
    listen(target, "mouseleave", move |_: MouseEvent| {
        send(&st, Input::Pointer(Pointer::Leave));
    });
    let st = state.clone();
    listen(target, "wheel", move |e: WheelEvent| {
        let p = at(&st, f64::from(e.client_x()), f64::from(e.client_y()));
        let unit = if e.delta_mode() == 1 { LINE_PX } else { 1.0 };
        let dpr = st.borrow().dpr;
        let dy = (e.delta_y() * unit * dpr)
            .round()
            .clamp(-100_000.0, 100_000.0) as i32;
        e.prevent_default();
        send(&st, Input::Pointer(Pointer::Wheel(p, dy)));
    });

    // Touch: the first finger is the pointer.
    for (name, kind) in [
        ("touchstart", 0),
        ("touchmove", 1),
        ("touchend", 2),
        ("touchcancel", 2),
    ] {
        let st = state.clone();
        listen(target, name, move |e: TouchEvent| {
            e.prevent_default();
            let touch = e.changed_touches().item(0).or_else(|| e.touches().item(0));
            let Some(t) = touch else { return };
            let p = at(&st, f64::from(t.client_x()), f64::from(t.client_y()));
            match kind {
                0 => send(&st, Input::Pointer(Pointer::Down(p))),
                1 => send(&st, Input::Pointer(Pointer::Move(p))),
                _ => {
                    send(&st, Input::Pointer(Pointer::Up(p)));
                    send(&st, Input::Pointer(Pointer::Leave));
                }
            }
        });
    }

    // Keys: on the page, whoever has the focus (the canvas or the text area).
    let st = state.clone();
    let doc: web_sys::EventTarget = document().into();
    listen(&doc, "keydown", move |e: KeyboardEvent| {
        if e.is_composing() {
            return;
        }
        let wants = st.borrow().shell.app.wants_text();
        if let Some(input) = key_input(&e, wants) {
            e.prevent_default();
            send(&st, input);
        }
    });

    // The text area: typing, composition and paste arrive here while a field has the focus.
    let area: web_sys::EventTarget = state.borrow().textarea.clone().into();
    let st = state.clone();
    listen(&area, "input", move |_: web_sys::Event| {
        if st.borrow().composing {
            return;
        }
        let text = st.borrow().textarea.value();
        if !text.is_empty() {
            st.borrow().textarea.set_value("");
            for piece in fusuma::text_inputs(&text) {
                send(&st, piece);
            }
        }
    });
    let st = state.clone();
    listen(&area, "compositionstart", move |_: CompositionEvent| {
        st.borrow_mut().composing = true;
    });
    let st = state.clone();
    listen(&area, "compositionupdate", move |e: CompositionEvent| {
        send(
            &st,
            Input::Preedit(InputText::new(&e.data().unwrap_or_default())),
        );
    });
    let st = state.clone();
    listen(&area, "compositionend", move |e: CompositionEvent| {
        st.borrow_mut().composing = false;
        send(&st, Input::Preedit(InputText::new("")));
        let text = e.data().unwrap_or_default();
        st.borrow().textarea.set_value("");
        if !text.is_empty() {
            send(&st, Input::Text(InputText::new(&text)));
        }
    });
    // Copy and cut take the field's selection.
    for (name, cut) in [("copy", false), ("cut", true)] {
        let st = state.clone();
        listen(&area, name, move |e: ClipboardEvent| {
            let selected = st.borrow().shell.app.selected_text().map(str::to_owned);
            if let (Some(text), Some(data)) = (selected, e.clipboard_data()) {
                let _ = data.set_data("text/plain", &text);
                e.prevent_default();
                if cut {
                    send(&st, Input::Edit(Edit::Backspace));
                }
            }
        });
    }

    // A new size or scale of the window.
    let st = state.clone();
    let win: web_sys::EventTarget = window().into();
    listen(&win, "resize", move |_: web_sys::Event| resize(&st));
}

/// The window changed: a new surface, and the app told.
fn resize(state: &Shared) {
    let (size, dpr, fonts) = {
        let s = state.borrow();
        let dpr = window().device_pixel_ratio().max(0.5);
        (wanted_size(dpr), dpr, s.fonts)
    };
    {
        let mut guard = state.borrow_mut();
        let s = &mut *guard;
        s.canvas.set_width(size.0);
        s.canvas.set_height(size.1);
        match &mut s.gfx {
            Gfx::Cpu(cpu) => {
                let Some(mut r) = vello_cpu::new(size.0, size.1) else {
                    return;
                };
                r.set_fonts(fonts);
                cpu.renderer = r;
            }
            Gfx::Gpu(gpu) => {
                configure(&gpu.surface, &gpu.device, size);
                let Ok(mut r) =
                    vello_gpu::from_device(gpu.device.clone(), gpu.queue.clone(), size.0, size.1)
                else {
                    return;
                };
                r.set_fonts(fonts);
                gpu.renderer = r;
            }
        }
        s.size = size;
        s.dpr = dpr;
        let _ = s.canvas.style().set_property("width", "100vw");
        let _ = s.canvas.style().set_property("height", "100vh");
    }
    send(
        state,
        Input::Resize {
            width: size.0,
            height: size.1,
            scale_percent: (dpr * 100.0).round() as u32,
        },
    );
    // Everything is drawn again on a fresh surface.
    state.borrow_mut().shell.app.invalidate();
}

/// Sets up the canvas surface for `size`: the format Vello renders in, so a frame is a copy.
fn configure(surface: &wgpu::Surface<'_>, device: &wgpu::Device, size: (u32, u32)) {
    surface.configure(
        device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            format: wgpu::TextureFormat::Rgba8Unorm,
            width: size.0,
            height: size.1,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::default(),
        },
    );
}

/// Vello on the browser's WebGPU, if the browser has it and can show Vello's format on the canvas.
async fn try_gpu(
    canvas: &HtmlCanvasElement,
    size: (u32, u32),
    fonts: fusuma::fonts::FontSet,
) -> Result<Gfx, String> {
    // No `navigator.gpu`: nothing to ask, and the canvas stays free for the 2D fallback.
    let has_gpu = js_sys::Reflect::get(&window().navigator(), &JsValue::from_str("gpu"))
        .map(|g| !g.is_undefined() && !g.is_null())
        .unwrap_or(false);
    if !has_gpu {
        return Err("this browser has no WebGPU".into());
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
        .map_err(|e| format!("no surface: {e}"))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("no adapter: {e}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("fusuma web"),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("no device: {e}"))?;
    if !surface
        .get_capabilities(&adapter)
        .formats
        .contains(&wgpu::TextureFormat::Rgba8Unorm)
    {
        return Err("the canvas cannot show Rgba8Unorm".into());
    }
    configure(&surface, &device, size);
    let mut renderer = vello_gpu::from_device(device.clone(), queue.clone(), size.0, size.1)
        .map_err(|e| e.to_string())?;
    renderer.set_fonts(fonts);
    Ok(Gfx::Gpu(Box::new(Gpu {
        renderer,
        surface,
        device,
        queue,
    })))
}

/// A new canvas in the place of `old`, with the same id: a canvas that has given a WebGPU context cannot give a 2D
/// one, so after a failed attempt the page gets a fresh one.
fn fresh_canvas(old: &HtmlCanvasElement) -> Result<HtmlCanvasElement, JsValue> {
    let new: HtmlCanvasElement = document().create_element("canvas")?.dyn_into()?;
    new.set_id("fusuma");
    new.set_attribute("tabindex", "0")?;
    if let Some(parent) = old.parent_node() {
        parent.replace_child(&new, old)?;
    }
    Ok(new)
}

/// Which renderer the address asks for: `?renderer=cpu` or `?renderer=gpu`, else whichever works (the GPU first).
fn wanted_renderer() -> Option<String> {
    let search = window().location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search)
        .ok()?
        .get("renderer")
}

/// Builds the page's app and starts the loop.
///
/// # Errors
/// A JavaScript error when the page has no canvas.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        if let Err(e) = run().await {
            web_sys::console::error_1(&e);
            if let Some(b) = document().body() {
                b.set_text_content(Some(&format!("{e:?}")));
            }
        }
    });
    Ok(())
}

async fn run() -> Result<(), JsValue> {
    let doc = document();
    let mut canvas: HtmlCanvasElement = doc
        .get_element_by_id("fusuma")
        .ok_or("no <canvas id=\"fusuma\"> on the page")?
        .dyn_into()?;
    let settings = Settings::from_toml(SETTINGS).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let fonts = fusuma::fonts::FontSet::choose(
        settings.fonts.preset.as_deref(),
        settings.fonts.sans.as_deref(),
        settings.fonts.mono.as_deref(),
        settings.fonts.icons.as_deref(),
    )
    .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let palette = fusuma::Palette::choose(
        settings.theme.palette.as_deref(),
        settings.theme.accent.as_deref(),
    )
    .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let dpr = window().device_pixel_ratio().max(0.5);
    let size = wanted_size(dpr);
    canvas.set_width(size.0);
    canvas.set_height(size.1);
    let _ = canvas.style().set_property("width", "100vw");
    let _ = canvas.style().set_property("height", "100vh");

    let wanted = wanted_renderer();
    let mut gfx = None;
    if wanted.as_deref() != Some("cpu") {
        match try_gpu(&canvas, size, fonts).await {
            Ok(g) => gfx = Some(g),
            Err(why) => {
                web_sys::console::log_1(&JsValue::from_str(&format!("Vello on the CPU: {why}")));
                // The attempt may have taken the canvas for WebGPU.
                canvas = fresh_canvas(&canvas)?;
                canvas.set_width(size.0);
                canvas.set_height(size.1);
                let _ = canvas.style().set_property("width", "100vw");
                let _ = canvas.style().set_property("height", "100vh");
            }
        }
    }
    let gfx = match gfx {
        Some(g) => g,
        None => {
            let ctx: CanvasRenderingContext2d = canvas
                .get_context("2d")?
                .ok_or("the canvas has no 2d context")?
                .dyn_into()?;
            let mut renderer = vello_cpu::new(size.0, size.1).ok_or("cannot make a surface")?;
            renderer.set_fonts(fonts);
            Gfx::Cpu(Box::new(Cpu { renderer, ctx }))
        }
    };
    let which = match &gfx {
        Gfx::Cpu(_) => "cpu",
        Gfx::Gpu(_) => "gpu",
    };
    let _ = canvas.set_attribute("data-renderer", which);
    web_sys::console::log_1(&JsValue::from_str(&format!(
        "renderer: Vello on the {which}"
    )));

    // The hidden text area: invisible, one pixel wide, and kept where the caret is.
    let textarea: HtmlTextAreaElement = doc.create_element("textarea")?.dyn_into()?;
    textarea.set_id("fusuma-input");
    textarea.set_attribute("aria-hidden", "true")?;
    textarea.set_attribute("autocapitalize", "off")?;
    textarea.set_attribute("autocomplete", "off")?;
    textarea.set_attribute("spellcheck", "false")?;
    let style = textarea.style();
    for (k, v) in [
        ("position", "fixed"),
        ("left", "0"),
        ("top", "0"),
        ("width", "1px"),
        ("height", "16px"),
        ("opacity", "0"),
        ("padding", "0"),
        ("border", "0"),
        ("resize", "none"),
        ("overflow", "hidden"),
        ("pointer-events", "none"),
        ("font-size", "16px"),
    ] {
        style.set_property(k, v)?;
    }
    doc.body().ok_or("no body")?.append_child(&textarea)?;

    let mut shell = Shell::new(Showcase::new(size.0, size.1));
    shell.input(Input::Palette(palette));
    shell.input(Input::Resize {
        width: size.0,
        height: size.1,
        scale_percent: (dpr * 100.0).round() as u32,
    });
    let state: Shared = Rc::new(RefCell::new(State {
        shell,
        gfx,
        canvas,
        textarea,
        fonts,
        dpr,
        size,
        composing: false,
        scheduled: false,
        typing: false,
    }));
    install(&state);
    schedule(&state);
    let _ = state.borrow().canvas.focus();
    Ok(())
}
