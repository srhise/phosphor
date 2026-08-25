mod cp437;
mod editor;
mod fileio;
mod font;
mod present;
mod status;
mod vga;
mod wrap;

use std::sync::Arc;

use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use vga::{Mode, Screen, FB_HEIGHT, FB_WIDTH};

struct App {
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    present: Option<present::Present>,
    screen: Screen,
}

impl App {
    fn new() -> Self {
        let mut screen = Screen::new(Mode::Text80x25);
        screen.clear(7, 1);
        // Proof of life; Task 10 replaces this with the document.
        screen.put_str(wrap::TEXT_LEFT, 2, "The quick brown fox jumped over", 7, 1);
        screen.put_str(wrap::TEXT_LEFT, 3, "the lazy dog.", 7, 1);
        screen.put_str(0, 24, "(UNTITLED)", 15, 1);
        screen.put_str(51, 24, "Doc 1   Pg 1   Ln 1\"   Pos 1\"", 15, 1);
        Self { window: None, pixels: None, present: None, screen }
    }

    fn redraw(&mut self) {
        let (Some(pixels), Some(present), Some(window)) =
            (self.pixels.as_mut(), self.present.as_ref(), self.window.as_ref())
        else {
            return;
        };
        self.screen.render(pixels.frame_mut());
        let size = window.inner_size();
        let params = present::Params {
            surface: (size.width, size.height),
            time: 0.0,
            effects: false,
        };
        let result = pixels.render_with(|encoder, target, context| {
            present.render(encoder, target, context, &params);
            Ok(())
        });
        if let Err(e) = result {
            eprintln!("render failed: {e}");
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("word")
            .with_inner_size(LogicalSize::new(1080.0, 810.0))
            .with_min_inner_size(LogicalSize::new(640.0, 480.0));

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("could not create a window: {e}");
                event_loop.exit();
                return;
            }
        };

        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, Arc::clone(&window));
        let pixels = match Pixels::new(FB_WIDTH as u32, FB_HEIGHT as u32, surface) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("could not initialise the display: {e}");
                event_loop.exit();
                return;
            }
        };

        self.present = Some(present::Present::new(&pixels));
        self.pixels = Some(pixels);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(pixels) = self.pixels.as_mut() {
                    if let Err(e) = pixels.resize_surface(size.width.max(1), size.height.max(1)) {
                        eprintln!("resize failed: {e}");
                    }
                }
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }
}

fn main() {
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("could not start: {e}");
            return;
        }
    };
    // No animation loop: we draw when something happens.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new();
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("exited with error: {e}");
    }
}
