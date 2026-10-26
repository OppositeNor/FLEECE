//! Native window, input, graphics and clipboard integration.
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use fleece::app::{Editor, configure_context};
#[cfg(not(target_arch = "wasm32"))]
use fleece::model::{Circuit, Document};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use web_time::Instant;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key as NativeKey, NamedKey},
    window::{CursorIcon, Window, WindowId},
};

#[cfg(not(target_arch = "wasm32"))]
pub fn run(path: Option<std::path::PathBuf>) -> Result<(), String> {
    let document = match &path {
        Some(path) => Document::load(path)?,
        None => Document::new(Circuit::empty()),
    };
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut application = Desktop::new(document, path);
    event_loop
        .run_app(&mut application)
        .map_err(|e| e.to_string())?;
    match application.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct Graphics {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: egui_wgpu::Renderer,
}

fn ui_surface_format(formats: &[wgpu::TextureFormat]) -> Result<wgpu::TextureFormat, String> {
    use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb};
    // egui writes gamma-encoded colors to non-sRGB targets. A linear HDR
    // surface (such as Rgba16Float) would apply the transfer function again
    // during presentation, turning the dark theme into washed-out gray.
    formats
        .iter()
        .copied()
        .find(|format| matches!(format, Bgra8Unorm | Rgba8Unorm))
        .or_else(|| {
            formats
                .iter()
                .copied()
                .find(|format| matches!(format, Bgra8UnormSrgb | Rgba8UnormSrgb))
        })
        .ok_or_else(|| "No compatible 8-bit UI surface format.".into())
}

impl Graphics {
    async fn new(window: Arc<Window>) -> Result<Self, String> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        if cfg!(target_arch = "wasm32") {
            descriptor.backends = wgpu::Backends::GL;
        }
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let mut options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        };
        let adapter = match instance.request_adapter(&options).await {
            Ok(adapter) => adapter,
            Err(_) => {
                options.force_fallback_adapter = true;
                instance
                    .request_adapter(&options)
                    .await
                    .map_err(|e| format!("Cannot initialize graphics: {e}"))?
            }
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("FLEECE"),
                required_limits: if cfg!(target_arch = "wasm32") {
                    wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
                } else {
                    wgpu::Limits::default()
                },
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("No compatible window surface.")?;
        config.format = ui_surface_format(&surface.get_capabilities(&adapter).formats)?;
        config.color_space = wgpu::SurfaceColorSpace::Srgb;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        let renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );
        Ok(Self {
            instance,
            surface,
            device,
            queue,
            config,
            renderer,
        })
    }
    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }
    fn paint(
        &mut self,
        window: &Arc<Window>,
        context: &egui::Context,
        output: egui::FullOutput,
    ) -> Result<(), String> {
        let mut textures = output.textures_delta;
        let updates = std::mem::take(&mut textures.set);
        let freed = std::mem::take(&mut textures.free);
        // Upload even if the surface is currently unavailable, preserving font atlas changes.
        for (id, deltas) in &updates {
            for delta in deltas {
                self.renderer
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(window.clone())
                    .map_err(|e| e.to_string())?;
                self.surface.configure(&self.device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => None,
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Graphics surface validation failed.".into());
            }
        };
        if let Some(frame) = frame {
            let jobs = context.tessellate(output.shapes, output.pixels_per_point);
            let screen = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [self.config.width, self.config.height],
                pixels_per_point: output.pixels_per_point,
            };
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("editor frame"),
                });
            let buffers = self.renderer.update_buffers(
                &self.device,
                &self.queue,
                &mut encoder,
                &jobs,
                &screen,
            );
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("editor UI"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                self.renderer
                    .render(&mut pass.forget_lifetime(), &jobs, &screen);
            }
            self.queue
                .submit(buffers.into_iter().chain(std::iter::once(encoder.finish())));
            window.pre_present_notify();
            self.queue.present(frame);
        }
        for id in &freed {
            self.renderer.free_texture(id);
        }
        Ok(())
    }
}

struct Desktop {
    editor: Editor,
    window: Option<Arc<Window>>,
    graphics: Option<Graphics>,
    #[cfg(target_arch = "wasm32")]
    graphics_ready: std::rc::Rc<std::cell::RefCell<Option<Result<Graphics, String>>>>,
    context: egui::Context,
    input: egui::RawInput,
    modifiers: Modifiers,
    pointer: Pos2,
    started: Instant,
    #[cfg(not(target_arch = "wasm32"))]
    next_frame: Instant,
    clipboard: String,
    ime_composing: bool,
    minimized: bool,
    error: Option<String>,
}
impl Desktop {
    #[cfg(not(target_arch = "wasm32"))]
    fn new(document: Document, path: Option<std::path::PathBuf>) -> Self {
        Self::with_editor(Editor::with_native_preferences(document, path))
    }
    fn with_editor(editor: Editor) -> Self {
        let context = egui::Context::default();
        configure_context(&context);
        Self {
            editor,
            window: None,
            graphics: None,
            #[cfg(target_arch = "wasm32")]
            graphics_ready: Default::default(),
            context,
            input: egui::RawInput::default(),
            modifiers: Modifiers::default(),
            pointer: Pos2::ZERO,
            started: Instant::now(),
            #[cfg(not(target_arch = "wasm32"))]
            next_frame: Instant::now(),
            clipboard: String::new(),
            ime_composing: false,
            minimized: false,
            error: None,
        }
    }
    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(result) = self.graphics_ready.borrow_mut().take() {
                match result {
                    Ok(mut graphics) => {
                        if let Some(window) = &self.window {
                            let size = window.inner_size();
                            graphics.resize(size.width, size.height);
                        }
                        self.graphics = Some(graphics);
                        crate::web::ready();
                    }
                    Err(error) => {
                        crate::web::fatal(&error);
                        event_loop.exit();
                        return;
                    }
                }
            }
            if self.graphics.is_none() {
                return;
            }
            crate::web::receive(&mut self.editor, &mut self.input);
        }
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 || self.minimized {
            return;
        }
        let scale = window.scale_factor() as f32;
        self.input.screen_rect = Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(size.width as f32 / scale, size.height as f32 / scale),
        ));
        self.input.time = Some(self.started.elapsed().as_secs_f64());
        if let Some(viewport) = self.input.viewports.get_mut(&egui::ViewportId::ROOT) {
            viewport.native_pixels_per_point = Some(scale);
        }
        let output = self
            .context
            .run_ui(self.input.take(), |ui| self.editor.ui(ui));
        window.set_title(&self.editor.title());
        #[cfg(target_arch = "wasm32")]
        crate::web::transfers(&mut self.editor);
        window.set_cursor(match output.platform_output.cursor_icon {
            egui::CursorIcon::Text => CursorIcon::Text,
            egui::CursorIcon::PointingHand => CursorIcon::Pointer,
            egui::CursorIcon::Grab => CursorIcon::Grab,
            egui::CursorIcon::Grabbing => CursorIcon::Grabbing,
            egui::CursorIcon::ResizeHorizontal => CursorIcon::EwResize,
            egui::CursorIcon::ResizeVertical => CursorIcon::NsResize,
            _ => CursorIcon::Default,
        });
        window.set_ime_allowed(output.platform_output.ime.is_some());
        if let Some(ime) = &output.platform_output.ime {
            window.set_ime_cursor_area(
                winit::dpi::LogicalPosition::new(ime.cursor_rect.min.x, ime.cursor_rect.min.y),
                winit::dpi::LogicalSize::new(ime.cursor_rect.width(), ime.cursor_rect.height()),
            );
        }
        for command in &output.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = command {
                self.clipboard = text.clone();
                clipboard_write(text);
            }
        }
        if let Some(graphics) = &mut self.graphics
            && let Err(error) = graphics.paint(&window, &self.context, output)
        {
            #[cfg(target_arch = "wasm32")]
            crate::web::fatal(&error);
            self.error = Some(error);
            event_loop.exit();
        }
        if self.editor.should_quit {
            event_loop.exit();
        }
    }
    fn keyboard(&mut self, event: winit::event::KeyEvent) {
        // Enabling an input method does not mean it is composing text. Ordinary
        // keys still arrive through KeyEvent::text while a text field has focus.
        if self.ime_composing {
            return;
        }
        let pressed = event.state == ElementState::Pressed;
        let key = translate_key(&event.logical_key);
        if let Some(key) = key {
            if pressed && self.modifiers.command {
                match key {
                    Key::V => {
                        #[cfg(target_arch = "wasm32")]
                        return;
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            let text = clipboard_read().unwrap_or_else(|| self.clipboard.clone());
                            self.input.events.push(Event::Paste(text));
                            return;
                        }
                    }
                    Key::C => self.input.events.push(Event::Copy),
                    Key::X => self.input.events.push(Event::Cut),
                    _ => {}
                }
            }
            self.input.events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: event.repeat,
                modifiers: self.modifiers,
            });
        }
        if pressed && let Some(text) = event.text {
            self.keyboard_text(&text);
        }
    }

    fn keyboard_text(&mut self, text: &str) {
        if self.modifiers.command || self.modifiers.ctrl || self.ime_composing {
            return;
        }
        let text: String = text
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        if !text.is_empty() {
            self.input.events.push(Event::Text(text));
        }
    }

    fn ime(&mut self, event: Ime) {
        match event {
            Ime::Enabled => {}
            Ime::Disabled => {
                self.ime_composing = false;
                self.input.events.push(Event::Ime(egui::ImeEvent::Preedit {
                    text: String::new(),
                    active_range_chars: None,
                }));
            }
            Ime::Preedit(text, range) => {
                self.ime_composing = !text.is_empty();
                let active_range_chars = range.and_then(|(start, end)| {
                    Some(text.get(..start)?.chars().count()..text.get(..end)?.chars().count())
                });
                self.input.events.push(Event::Ime(egui::ImeEvent::Preedit {
                    text,
                    active_range_chars,
                }));
            }
            Ime::Commit(text) => {
                self.ime_composing = false;
                self.input
                    .events
                    .push(Event::Ime(egui::ImeEvent::Commit(text)));
            }
        }
    }
}
impl ApplicationHandler for Desktop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| {
            let attributes = Window::default_attributes().with_title(self.editor.title());
            #[cfg(not(target_arch = "wasm32"))]
            let attributes = attributes
                .with_inner_size(winit::dpi::LogicalSize::new(1440.0, 900.0))
                .with_min_inner_size(winit::dpi::LogicalSize::new(900.0, 600.0));
            #[cfg(target_arch = "wasm32")]
            let attributes = {
                use winit::platform::web::WindowAttributesExtWebSys;
                attributes
                    .with_canvas(Some(crate::web::canvas()?))
                    .with_prevent_default(true)
            };
            let window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .map_err(|e| e.to_string())?,
            );
            #[cfg(not(target_arch = "wasm32"))]
            {
                let graphics = pollster::block_on(Graphics::new(window.clone()))?;
                self.graphics = Some(graphics);
            }
            #[cfg(target_arch = "wasm32")]
            {
                let ready = self.graphics_ready.clone();
                let canvas_window = window.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    *ready.borrow_mut() = Some(Graphics::new(canvas_window.clone()).await);
                    canvas_window.request_redraw();
                });
            }
            self.window = Some(window);
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            #[cfg(target_arch = "wasm32")]
            crate::web::fatal(&error);
            self.error = Some(error);
            event_loop.exit();
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.editor.request_close(),
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            WindowEvent::Resized(size) => {
                if let Some(graphics) = &mut self.graphics {
                    graphics.resize(size.width, size.height);
                }
                self.minimized = size.width == 0 || size.height == 0;
            }
            WindowEvent::Occluded(value) => self.minimized = value,
            WindowEvent::Focused(focused) => {
                self.input.focused = focused;
                self.input.events.push(Event::WindowFocused(focused));
                if !focused {
                    self.modifiers = Modifiers::default();
                    self.input
                        .events
                        .push(Event::ModifiersChanged(self.modifiers));
                    self.ime(Ime::Disabled);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = Modifiers {
                    alt: state.alt_key(),
                    ctrl: state.control_key(),
                    shift: state.shift_key(),
                    mac_cmd: is_macos() && state.super_key(),
                    command: if is_macos() {
                        state.super_key()
                    } else {
                        state.control_key()
                    },
                };
                self.input
                    .events
                    .push(Event::ModifiersChanged(self.modifiers));
            }
            WindowEvent::KeyboardInput { event, .. } => self.keyboard(event),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor()) as f32;
                self.pointer = Pos2::new(position.x as f32 / scale, position.y as f32 / scale);
                self.input.events.push(Event::PointerMoved(self.pointer));
            }
            WindowEvent::CursorLeft { .. } => self.input.events.push(Event::PointerGone),
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => Some(PointerButton::Primary),
                    MouseButton::Right => Some(PointerButton::Secondary),
                    MouseButton::Middle => Some(PointerButton::Middle),
                    _ => None,
                };
                if let Some(button) = button {
                    self.input.events.push(Event::PointerButton {
                        pos: self.pointer,
                        button,
                        pressed: state == ElementState::Pressed,
                        modifiers: self.modifiers,
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor()) as f32;
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => Vec2::new(x * 40.0, y * 40.0),
                    MouseScrollDelta::PixelDelta(p) => {
                        Vec2::new(p.x as f32 / scale, p.y as f32 / scale)
                    }
                };
                self.input.events.push(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta,
                    modifiers: self.modifiers,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                });
            }
            WindowEvent::PinchGesture { delta, .. } => {
                self.input.events.push(Event::Zoom((delta as f32).exp()))
            }
            WindowEvent::Ime(event) => self.ime(event),
            WindowEvent::DroppedFile(path) => self.editor.open_path(path),
            _ => {}
        }
        if self.editor.should_quit {
            event_loop.exit();
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if Instant::now() >= self.next_frame {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                self.next_frame =
                    Instant::now() + Duration::from_millis(if self.minimized { 200 } else { 16 });
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        }
        #[cfg(target_arch = "wasm32")]
        {
            // On web, request_redraw schedules requestAnimationFrame. Waiting
            // between frames avoids a busy polling loop and follows tab throttling.
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn translate_key(key: &NativeKey) -> Option<Key> {
    match key {
        NativeKey::Named(named) => Some(match named {
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::Escape => Key::Escape,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Enter => Key::Enter,
            NamedKey::Space => Key::Space,
            NamedKey::Insert => Key::Insert,
            NamedKey::Delete => Key::Delete,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::F1 => Key::F1,
            _ => return None,
        }),
        NativeKey::Character(text) => Some(match text.to_ascii_lowercase().as_str() {
            "a" => Key::A,
            "b" => Key::B,
            "c" => Key::C,
            "d" => Key::D,
            "e" => Key::E,
            "f" => Key::F,
            "g" => Key::G,
            "h" => Key::H,
            "i" => Key::I,
            "j" => Key::J,
            "k" => Key::K,
            "l" => Key::L,
            "m" => Key::M,
            "n" => Key::N,
            "o" => Key::O,
            "p" => Key::P,
            "q" => Key::Q,
            "r" => Key::R,
            "s" => Key::S,
            "t" => Key::T,
            "u" => Key::U,
            "v" => Key::V,
            "w" => Key::W,
            "x" => Key::X,
            "y" => Key::Y,
            "z" => Key::Z,
            "0" => Key::Num0,
            "1" => Key::Num1,
            "2" => Key::Num2,
            "3" => Key::Num3,
            "4" => Key::Num4,
            "5" => Key::Num5,
            "6" => Key::Num6,
            "7" => Key::Num7,
            "8" => Key::Num8,
            "9" => Key::Num9,
            " " => Key::Space,
            "-" => Key::Minus,
            "+" => Key::Plus,
            "=" => Key::Equals,
            _ => return None,
        }),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn clipboard_read() -> Option<String> {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbpaste", &[])]
    } else {
        &[
            ("wl-paste", &["--no-newline"]),
            ("xclip", &["-selection", "clipboard", "-o"]),
        ]
    };
    for (program, args) in candidates {
        if let Ok(output) = std::process::Command::new(program)
            .args(*args)
            .stderr(std::process::Stdio::null())
            .output()
            && output.status.success()
        {
            return String::from_utf8(output.stdout).ok();
        }
    }
    None
}
#[cfg(not(target_arch = "wasm32"))]
fn clipboard_write(text: &str) {
    use std::io::Write;
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else {
        &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])]
    };
    for (program, args) in candidates {
        if let Ok(mut child) = std::process::Command::new(program)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            if child.wait().is_ok_and(|s| s.success()) {
                return;
            }
        }
    }
}

fn is_macos() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::is_macos()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        cfg!(target_os = "macos")
    }
}

#[cfg(target_arch = "wasm32")]
fn clipboard_write(text: &str) {
    crate::web::copy(text);
}

#[cfg(target_arch = "wasm32")]
pub(super) fn run_web(editor: Editor) -> Result<(), String> {
    use winit::platform::web::EventLoopExtWebSys;
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    event_loop.spawn_app(Desktop::with_editor(editor));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleece::model::{Circuit, Document};

    fn desktop() -> Desktop {
        Desktop::with_editor(Editor::new(Document::new(Circuit::empty()), None))
    }

    #[test]
    fn ui_surface_keeps_srgb_theme_colors_when_hdr_formats_are_preferred() {
        use wgpu::TextureFormat::{Bgra8Unorm, Rgb10a2Unorm, Rgba8Unorm, Rgba16Float};
        for format in [Bgra8Unorm, Rgba8Unorm] {
            assert_eq!(
                ui_surface_format(&[Rgba16Float, Rgb10a2Unorm, format]).unwrap(),
                format
            );
        }
        assert!(ui_surface_format(&[Rgba16Float, Rgb10a2Unorm]).is_err());
        assert!(ui_surface_format(&[]).is_err());
    }

    #[test]
    fn ui_surface_supports_srgb_only_displays_and_prefers_gamma_targets() {
        use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8UnormSrgb, Rgba16Float};
        for format in [Bgra8UnormSrgb, Rgba8UnormSrgb] {
            assert_eq!(ui_surface_format(&[Rgba16Float, format]).unwrap(), format);
        }
        assert_eq!(
            ui_surface_format(&[Rgba16Float, Bgra8UnormSrgb, Bgra8Unorm]).unwrap(),
            Bgra8Unorm
        );
    }

    #[test]
    fn enabling_ime_still_allows_ordinary_keyboard_text() {
        let mut desktop = desktop();
        desktop.ime(Ime::Enabled);
        desktop.keyboard_text("Signal A");
        desktop.keyboard_text(" ");
        desktop.keyboard_text("N");
        assert_eq!(
            desktop.input.events,
            vec![
                Event::Text("Signal A".into()),
                Event::Text(" ".into()),
                Event::Text("N".into()),
            ]
        );
    }

    #[test]
    fn only_active_composition_suppresses_keyboard_text() {
        let mut desktop = desktop();
        desktop.ime(Ime::Enabled);
        desktop.ime(Ime::Preedit("é".into(), Some((2, 2))));
        desktop.keyboard_text("e");
        assert_eq!(
            desktop.input.events,
            vec![Event::Ime(egui::ImeEvent::Preedit {
                text: "é".into(),
                active_range_chars: Some(1..1),
            })]
        );
        desktop.ime(Ime::Commit("é".into()));
        desktop.keyboard_text("A");
        assert_eq!(
            &desktop.input.events[1..],
            &[
                Event::Ime(egui::ImeEvent::Commit("é".into())),
                Event::Text("A".into()),
            ]
        );
    }

    #[test]
    fn cancelled_composition_restores_typing() {
        for cancellation in [Ime::Preedit(String::new(), None), Ime::Disabled] {
            let mut desktop = desktop();
            desktop.ime(Ime::Preedit("pending".into(), None));
            desktop.ime(cancellation);
            desktop.keyboard_text("Output label");
            assert_eq!(
                desktop.input.events.last(),
                Some(&Event::Text("Output label".into()))
            );
        }
    }

    #[test]
    fn shortcuts_and_control_characters_are_not_inserted_as_text() {
        let mut desktop = desktop();
        desktop.modifiers = Modifiers::COMMAND;
        desktop.keyboard_text("a");
        desktop.modifiers = Modifiers::CTRL;
        desktop.keyboard_text("v");
        assert!(desktop.input.events.is_empty());
        desktop.modifiers = Modifiers::NONE;
        desktop.keyboard_text("A\n\tB");
        assert_eq!(desktop.input.events, vec![Event::Text("AB".into())]);
    }
}
