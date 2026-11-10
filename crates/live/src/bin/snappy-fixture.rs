//! Opens plain windows on command so live tests have windows to snap. Requests arrive on
//! standard input and replies leave on standard output, one JSON object per line.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::rc::Rc;

use snappy_core::geometry::Rect;
use snappy_live::protocol::{Reply, Request};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Fullscreen, Window, WindowId};

fn main() {
    let event_loop = match EventLoop::<Request>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("snappy-fixture: {error}");
            std::process::exit(1);
        }
    };
    read_requests(event_loop.create_proxy());
    let mut fixture = Fixture::default();
    if let Err(error) = event_loop.run_app(&mut fixture) {
        eprintln!("snappy-fixture: {error}");
        std::process::exit(1);
    }
}

/// Forward each line of standard input to the event loop; the end of input quits.
fn read_requests(proxy: EventLoopProxy<Request>) {
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            match serde_json::from_str::<Request>(&line) {
                Ok(request) => {
                    if proxy.send_event(request).is_err() {
                        return;
                    }
                }
                Err(error) => reply(&Reply::error(format!("bad request: {error}"))),
            }
        }
        let _ = proxy.send_event(Request::Quit);
    });
}

fn reply(reply: &Reply) {
    let mut out = std::io::stdout().lock();
    let written = serde_json::to_string(reply)
        .map_err(std::io::Error::other)
        .and_then(|line| writeln!(out, "{line}"))
        .and_then(|()| out.flush());
    if let Err(error) = written {
        eprintln!("snappy-fixture: could not reply: {error}");
    }
}

#[derive(Default)]
struct Fixture {
    windows: HashMap<String, Rc<Window>>,
    #[cfg(target_os = "linux")]
    paint: paint::Painter,
}

impl Fixture {
    fn handle(&mut self, event_loop: &ActiveEventLoop, request: Request) -> Reply {
        match request {
            Request::Open {
                id,
                frame,
                min_size,
                grid,
            } => {
                let mut attributes = Window::default_attributes()
                    .with_title(format!("Fixture {id}"))
                    .with_position(LogicalPosition::new(frame.x, frame.y))
                    .with_inner_size(LogicalSize::new(frame.width, frame.height));
                if let Some(min) = min_size {
                    attributes = attributes.with_min_inner_size(LogicalSize::new(min.width, min.height));
                }
                if let Some(grid) = grid {
                    attributes = attributes.with_resize_increments(LogicalSize::new(grid.width, grid.height));
                }
                match event_loop.create_window(attributes) {
                    Ok(window) => {
                        #[cfg(windows)]
                        constraints::apply(&window, min_size, grid);
                        let window = Rc::new(window);
                        #[cfg(target_os = "linux")]
                        self.paint.add(event_loop, &window);
                        let reply = Reply::frame(outer_frame(&window));
                        self.windows.insert(id, window);
                        reply
                    }
                    Err(error) => Reply::error(error.to_string()),
                }
            }
            Request::Frame { id } => self.with_window(&id, |window| Reply::frame(outer_frame(window))),
            Request::Focus { id } => self.with_window(&id, |window| {
                window.focus_window();
                Reply::frame(outer_frame(window))
            }),
            Request::FullScreen { id, on } => self.with_window(&id, |window| {
                window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
                Reply::frame(outer_frame(window))
            }),
            Request::Close { id } => match self.windows.remove(&id) {
                Some(_window) => {
                    #[cfg(target_os = "linux")]
                    self.paint.remove(_window.id());
                    Reply::default()
                }
                None => Reply::error(format!("no window {id}")),
            },
            Request::Quit => {
                event_loop.exit();
                Reply::default()
            }
        }
    }

    fn with_window(&self, id: &str, action: impl FnOnce(&Window) -> Reply) -> Reply {
        match self.windows.get(id) {
            Some(window) => action(window),
            None => Reply::error(format!("no window {id}")),
        }
    }
}

/// The window's frame including its title bar, in points from the primary display's top
/// left, the way Snappy measures windows.
#[cfg(not(windows))]
fn outer_frame(window: &Window) -> Rect {
    let scale = window.scale_factor();
    let size = window.outer_size().to_logical::<f64>(scale);
    let origin = window
        .outer_position()
        .map(|position| position.to_logical::<f64>(scale))
        .unwrap_or_default();
    Rect::new(origin.x, origin.y, size.width, size.height)
}

/// On Windows, winit applies minimum sizes only to the user's resizing and ignores resize
/// increments. Real apps enforce both against every size change, Snappy's included, by
/// adjusting `WM_WINDOWPOSCHANGING`; the fixture does the same through a subclass.
#[cfg(windows)]
mod constraints {
    use std::collections::BTreeMap;
    use std::sync::{Mutex, PoisonError};

    use snappy_core::geometry::Size;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, GWLP_WNDPROC, GetClientRect, GetWindowRect, SWP_NOSIZE, SetWindowLongPtrW,
        WINDOWPOS, WM_WINDOWPOSCHANGING, WNDPROC,
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    #[derive(Clone, Copy)]
    struct Limits {
        previous: isize,
        min: Option<Size>,
        cell: Option<Size>,
    }

    static LIMITS: Mutex<BTreeMap<isize, Limits>> = Mutex::new(BTreeMap::new());

    pub fn apply(window: &Window, min: Option<Size>, cell: Option<Size>) {
        if min.is_none() && cell.is_none() {
            return;
        }
        let Some(RawWindowHandle::Win32(handle)) = window.window_handle().ok().map(|handle| handle.as_raw())
        else {
            return;
        };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        // SAFETY: replaces the procedure of a window this thread owns; the old one is
        // kept and called for every message.
        let previous = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, procedure as *const () as isize) };
        LIMITS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(hwnd.0 as isize, Limits { previous, min, cell });
    }

    /// The client size the limits allow for a requested client size.
    fn allowed(client: f64, min: Option<f64>, cell: Option<f64>) -> f64 {
        let client = min.map_or(client, |min| client.max(min));
        cell.filter(|cell| *cell >= 1.0)
            .map_or(client, |cell| (client / cell).floor() * cell)
    }

    unsafe extern "system" fn procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let limits = LIMITS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(window.0 as isize))
            .copied();
        let Some(limits) = limits else {
            return LRESULT(0);
        };
        if message == WM_WINDOWPOSCHANGING {
            // SAFETY: for this message, lparam points to the pending WINDOWPOS.
            let position = unsafe { &mut *(lparam.0 as *mut WINDOWPOS) };
            let (mut outer, mut inner) = (RECT::default(), RECT::default());
            // SAFETY: plain reads of this window's rectangles.
            let read = unsafe {
                GetWindowRect(window, &mut outer).is_ok() && GetClientRect(window, &mut inner).is_ok()
            };
            if read && !position.flags.contains(SWP_NOSIZE) {
                let extra_width = (outer.right - outer.left) - (inner.right - inner.left);
                let extra_height = (outer.bottom - outer.top) - (inner.bottom - inner.top);
                let width = allowed(
                    f64::from(position.cx - extra_width),
                    limits.min.map(|min| min.width),
                    limits.cell.map(|cell| cell.width),
                );
                let height = allowed(
                    f64::from(position.cy - extra_height),
                    limits.min.map(|min| min.height),
                    limits.cell.map(|cell| cell.height),
                );
                position.cx = width as i32 + extra_width;
                position.cy = height as i32 + extra_height;
            }
        }
        // SAFETY: the previous procedure was a valid window procedure for this window.
        unsafe {
            let previous: WNDPROC = std::mem::transmute::<isize, WNDPROC>(limits.previous);
            CallWindowProcW(previous, window, message, wparam, lparam)
        }
    }
}

/// The window's visible frame in physical pixels, as Snappy measures windows on Windows:
/// the window rectangle includes invisible resize borders.
#[cfg(windows)]
fn outer_frame(window: &Window) -> Rect {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().ok().map(|handle| handle.as_raw());
    let frame = match handle {
        Some(RawWindowHandle::Win32(handle)) => snappy_windows::reference::visible_frame(handle.hwnd.get()),
        _ => None,
    };
    frame.unwrap_or_default()
}

impl ApplicationHandler<Request> for Fixture {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _window: WindowId, _event: WindowEvent) {
        #[cfg(target_os = "linux")]
        if matches!(_event, WindowEvent::RedrawRequested | WindowEvent::Resized(_)) {
            self.paint.draw(_window);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, request: Request) {
        let answer = self.handle(event_loop, request);
        reply(&answer);
    }
}

/// Plain gray content. X11 shows a window that never draws; Wayland shows one only after it
/// has drawn a frame.
#[cfg(target_os = "linux")]
mod paint {
    use std::collections::HashMap;
    use std::num::NonZeroU32;
    use std::rc::Rc;

    use softbuffer::{Context, Surface};
    use winit::event_loop::{ActiveEventLoop, OwnedDisplayHandle};
    use winit::window::{Window, WindowId};

    const GRAY: u32 = 0x00d0_d0d0;

    type WindowSurface = Surface<OwnedDisplayHandle, Rc<Window>>;

    #[derive(Default)]
    pub struct Painter {
        context: Option<Context<OwnedDisplayHandle>>,
        surfaces: HashMap<WindowId, WindowSurface>,
    }

    impl Painter {
        pub fn add(&mut self, event_loop: &ActiveEventLoop, window: &Rc<Window>) {
            if self.context.is_none() {
                self.context = Context::new(event_loop.owned_display_handle()).ok();
            }
            let Some(context) = &self.context else {
                eprintln!("snappy-fixture: no drawing context");
                return;
            };
            match Surface::new(context, Rc::clone(window)) {
                Ok(surface) => {
                    self.surfaces.insert(window.id(), surface);
                    self.draw(window.id());
                }
                Err(error) => eprintln!("snappy-fixture: can't draw a window: {error}"),
            }
        }

        pub fn remove(&mut self, window: WindowId) {
            self.surfaces.remove(&window);
        }

        pub fn draw(&mut self, window: WindowId) {
            let Some(surface) = self.surfaces.get_mut(&window) else {
                return;
            };
            let size = surface.window().inner_size();
            let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
            else {
                return;
            };
            if surface.resize(width, height).is_err() {
                return;
            }
            if let Ok(mut buffer) = surface.buffer_mut() {
                buffer.fill(GRAY);
                let _ = buffer.present();
            }
        }
    }
}
