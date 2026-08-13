#![forbid(unsafe_code)]

use core::fmt;
use minifb::{
    CursorStyle, InputCallback, Key, KeyRepeat, MouseButton, MouseMode, ScaleMode, Window,
    WindowOptions,
};
use slate_chrome::{ChromeKeyCommand, ChromeView, Frame, WindowCommand, WindowVisualState};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

const MINIMIZED_WIDTH: usize = 440;
const MINIMIZED_HEIGHT: usize = 136;
const FALLBACK_MAXIMIZED_WIDTH: usize = 1600;
const FALLBACK_MAXIMIZED_HEIGHT: usize = 900;
const IDLE_SLEEP: Duration = Duration::from_millis(50);
const INPUT_SLEEP: Duration = Duration::from_millis(16);
const DRAG_SLEEP: Duration = Duration::from_millis(1);
const VIEWPORT_REFRESH_DEBOUNCE: Duration = Duration::from_millis(140);
const NATIVE_STATE_CONFIRMATION_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_EWMH_SUPPORTED_ATOMS: u32 = 1024;
const KEY_REPEAT_DELAY: f32 = 0.18;
const KEY_REPEAT_RATE: f32 = 0.025;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PointerPosition {
    x: isize,
    y: isize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowDrag {
    grab_x: isize,
    grab_y: isize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowGeometry {
    x: isize,
    y: isize,
    width: usize,
    height: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingNativeWindowState {
    state: WindowVisualState,
    size_at_request: (usize, usize),
    requested_at: Instant,
}

#[derive(Clone, Debug, Default)]
struct TextInputQueue {
    events: Rc<RefCell<Vec<TextInputEvent>>>,
    ctrl_down: Rc<RefCell<bool>>,
}

impl TextInputQueue {
    fn callback(&self) -> TextInputCallback {
        TextInputCallback {
            events: Rc::clone(&self.events),
            ctrl_down: Rc::clone(&self.ctrl_down),
        }
    }

    fn drain(&self) -> Vec<TextInputEvent> {
        self.events.borrow_mut().drain(..).collect()
    }
}

#[derive(Clone, Debug)]
struct TextInputCallback {
    events: Rc<RefCell<Vec<TextInputEvent>>>,
    ctrl_down: Rc<RefCell<bool>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextInputEvent {
    Char(char),
    Command(ChromeKeyCommand),
}

impl InputCallback for TextInputCallback {
    fn add_char(&mut self, uni_char: u32) {
        if *self.ctrl_down.borrow() {
            return;
        }

        if let Some(ch) = char::from_u32(uni_char) {
            self.events.borrow_mut().push(TextInputEvent::Char(ch));
        }
    }

    fn set_key_state(&mut self, key: Key, state: bool) {
        if matches!(key, Key::LeftCtrl | Key::RightCtrl) {
            *self.ctrl_down.borrow_mut() = state;
        }

        if state && let Some(command) = key_command(key) {
            self.events
                .borrow_mut()
                .push(TextInputEvent::Command(command));
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowConfig {
    pub title: String,
    pub width: usize,
    pub height: usize,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Slate".to_string(),
            width: 1280,
            height: 720,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformError(String);

impl fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PlatformError {}

struct ClipboardDriver {
    system: Option<arboard::Clipboard>,
    fallback: String,
}

impl ClipboardDriver {
    fn new() -> Self {
        Self {
            system: arboard::Clipboard::new().ok(),
            fallback: String::new(),
        }
    }

    fn set_text(&mut self, text: String) {
        self.fallback = text.clone();
        if let Some(system) = &mut self.system
            && system.set_text(text).is_err()
        {
            self.system = None;
        }
    }

    fn text(&mut self) -> String {
        if let Some(system) = &mut self.system {
            match system.get_text() {
                Ok(text) => return text,
                Err(_) => self.system = None,
            }
        }

        self.fallback.clone()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditShortcut {
    SelectAll,
    Copy,
    Paste,
}

pub fn run_browser_window(mut view: ChromeView, config: WindowConfig) -> Result<(), PlatformError> {
    let normal_size = (config.width, config.height);
    let mut visual_state = WindowVisualState::Normal;
    let text_input = TextInputQueue::default();
    let mut clipboard = ClipboardDriver::new();
    let mut window = create_window(&config.title, normal_size.0, normal_size.1)?;
    install_text_input(&mut window, &text_input);
    let mut window_position = window.get_position();
    let mut drag = None;
    let mut previous_left_down = false;
    let mut cached_frame = None;
    let mut cached_size = (0, 0);
    let mut frame_dirty = true;
    let mut viewport_refresh_pending = false;
    let mut last_resize_at = None;
    let mut pending_native_state = None;

    while window.is_open() {
        if pending_native_state.is_some() {
            window.update();
        }

        let (width, height) = window.get_size();
        if let Some(pending) = pending_native_state {
            if native_state_request_confirmed(pending, (width, height)) {
                pending_native_state = None;
            } else if native_state_request_timed_out(pending.requested_at.elapsed()) {
                window_position = current_window_position(&window, window_position);
                window = recreate_window_for_visual_state(
                    &config.title,
                    pending.state,
                    normal_size,
                    window_position,
                    &text_input,
                )?;
                pending_native_state = None;
                previous_left_down = false;
                drag = None;
                cached_frame = None;
                cached_size = (0, 0);
                frame_dirty = true;
                continue;
            }
        }

        if cached_size != (width, height) {
            frame_dirty = true;
            if view.update_web_viewport(width.max(1), height.max(1)) {
                viewport_refresh_pending = true;
            }
            last_resize_at = Some(Instant::now());
        }

        let left_down = window.get_mouse_down(MouseButton::Left);
        let mut window_recreated = false;
        let mut close_requested = false;

        if left_down
            && let Some(pointer) = window
                .get_mouse_pos(MouseMode::Pass)
                .and_then(pointer_position)
        {
            if !previous_left_down {
                if let Some((x, y)) = hit_position(pointer) {
                    match view.handle_click(x, y, width.max(1), height.max(1)) {
                        WindowCommand::None => {
                            if view.is_draggable_chrome(x, y, width.max(1), height.max(1)) {
                                window_position = current_window_position(&window, window_position);
                                drag = Some(WindowDrag {
                                    grab_x: pointer.x,
                                    grab_y: pointer.y,
                                });
                                window.set_cursor_style(CursorStyle::ClosedHand);
                            } else {
                                frame_dirty = true;
                            }
                        }
                        WindowCommand::Close => break,
                        WindowCommand::Minimize => {
                            visual_state = WindowVisualState::Minimized;
                            view.set_window_state(visual_state);
                            window_position = current_window_position(&window, window_position);
                            window =
                                create_window(&config.title, MINIMIZED_WIDTH, MINIMIZED_HEIGHT)?;
                            install_text_input(&mut window, &text_input);
                            window.set_position(window_position.0, window_position.1);
                            drag = None;
                            cached_frame = None;
                            cached_size = (0, 0);
                            frame_dirty = true;
                            window_recreated = true;
                        }
                        WindowCommand::ToggleMaximize => {
                            let previous_visual_state = visual_state;
                            visual_state = next_window_visual_state(visual_state);

                            view.set_window_state(visual_state);
                            window_position = current_window_position(&window, window_position);
                            let native_state_requested = previous_visual_state
                                != WindowVisualState::Minimized
                                && request_native_maximized(
                                    &window,
                                    visual_state == WindowVisualState::Maximized,
                                );

                            if native_state_requested {
                                pending_native_state = Some(PendingNativeWindowState {
                                    state: visual_state,
                                    size_at_request: (width, height),
                                    requested_at: Instant::now(),
                                });
                                drag = None;
                                frame_dirty = true;
                            } else {
                                window = recreate_window_for_visual_state(
                                    &config.title,
                                    visual_state,
                                    normal_size,
                                    window_position,
                                    &text_input,
                                )?;
                                if visual_state == WindowVisualState::Maximized {
                                    pending_native_state = request_native_maximized(&window, true)
                                        .then_some(PendingNativeWindowState {
                                            state: visual_state,
                                            size_at_request: window.get_size(),
                                            requested_at: Instant::now(),
                                        });
                                }
                                drag = None;
                                cached_frame = None;
                                cached_size = (0, 0);
                                frame_dirty = true;
                                window_recreated = true;
                            }
                        }
                    }
                }
            } else if let Some(active_drag) = drag {
                window_position = current_window_position(&window, window_position);
                window_position = dragged_window_position(window_position, active_drag, pointer);
                window.set_position(window_position.0, window_position.1);
            }
        }

        if !left_down {
            if drag.is_some() {
                window.set_cursor_style(CursorStyle::Arrow);
            }
            window_position = current_window_position(&window, window_position);
            drag = None;
        }

        previous_left_down = if window_recreated { false } else { left_down };

        if process_edit_shortcuts(&window, &mut view, &mut clipboard)
            && !update_cached_toolbar(&mut window, &view, &mut cached_frame, cached_size)?
        {
            frame_dirty = true;
        }

        match process_keyboard_input(&text_input, &mut view) {
            KeyboardOutcome::Changed => {
                if !update_cached_toolbar(&mut window, &view, &mut cached_frame, cached_size)? {
                    frame_dirty = true;
                }
            }
            KeyboardOutcome::CloseRequested => close_requested = true,
            KeyboardOutcome::Idle => {}
        }

        if close_requested {
            break;
        }

        if viewport_refresh_ready(
            last_resize_at.map(|instant| instant.elapsed()),
            viewport_refresh_pending,
            view.is_address_bar_focused(),
        ) {
            if view.refresh_web_viewport() {
                frame_dirty = true;
            }
            viewport_refresh_pending = view.web_viewport_needs_refresh();
        }

        let dragging = drag.is_some();
        if frame_dirty || cached_frame.is_none() {
            cached_frame = Some(view.render(width.max(1), height.max(1)));
            cached_size = (width, height);
            frame_dirty = false;
            update_window_buffer(&mut window, cached_frame.as_ref())?;
        } else {
            window.update();
        }

        if process_edit_shortcuts(&window, &mut view, &mut clipboard)
            && !update_cached_toolbar(&mut window, &view, &mut cached_frame, cached_size)?
        {
            let (width, height) = window.get_size();
            cached_frame = Some(view.render(width.max(1), height.max(1)));
            cached_size = (width, height);
            frame_dirty = false;
            update_window_buffer(&mut window, cached_frame.as_ref())?;
        }

        match process_keyboard_input(&text_input, &mut view) {
            KeyboardOutcome::Changed => {
                if !update_cached_toolbar(&mut window, &view, &mut cached_frame, cached_size)? {
                    let (width, height) = window.get_size();
                    cached_frame = Some(view.render(width.max(1), height.max(1)));
                    cached_size = (width, height);
                    frame_dirty = false;
                    update_window_buffer(&mut window, cached_frame.as_ref())?;
                }
            }
            KeyboardOutcome::CloseRequested => break,
            KeyboardOutcome::Idle => {}
        }

        std::thread::sleep(loop_sleep(dragging, view.is_address_bar_focused()));
    }

    Ok(())
}

fn create_window(title: &str, width: usize, height: usize) -> Result<Window, PlatformError> {
    let mut window = Window::new(
        title,
        width,
        height,
        WindowOptions {
            resize: true,
            scale_mode: ScaleMode::Stretch,
            ..WindowOptions::default()
        },
    )
    .map_err(|error| PlatformError(format!("failed to open Slate window: {error}")))?;
    window.set_target_fps(0);
    window.set_key_repeat_delay(KEY_REPEAT_DELAY);
    window.set_key_repeat_rate(KEY_REPEAT_RATE);
    Ok(window)
}

fn install_text_input(window: &mut Window, input: &TextInputQueue) {
    window.set_input_callback(Box::new(input.callback()));
}

fn update_window_buffer(window: &mut Window, frame: Option<&Frame>) -> Result<(), PlatformError> {
    let Some(frame) = frame else {
        return Ok(());
    };

    window
        .update_with_buffer(frame.pixels(), frame.width(), frame.height())
        .map_err(|error| PlatformError(format!("failed to update Slate window: {error}")))
}

fn next_window_visual_state(state: WindowVisualState) -> WindowVisualState {
    match state {
        WindowVisualState::Maximized => WindowVisualState::Normal,
        WindowVisualState::Normal | WindowVisualState::Minimized => WindowVisualState::Maximized,
    }
}

fn fallback_window_size(state: WindowVisualState, normal_size: (usize, usize)) -> (usize, usize) {
    match state {
        WindowVisualState::Normal => normal_size,
        WindowVisualState::Minimized => (MINIMIZED_WIDTH, MINIMIZED_HEIGHT),
        WindowVisualState::Maximized => (FALLBACK_MAXIMIZED_WIDTH, FALLBACK_MAXIMIZED_HEIGHT),
    }
}

fn recreate_window_for_visual_state(
    title: &str,
    state: WindowVisualState,
    normal_size: (usize, usize),
    position: (isize, isize),
    text_input: &TextInputQueue,
) -> Result<Window, PlatformError> {
    let geometry = fallback_window_geometry(state, normal_size, position);
    let mut window = create_window(title, geometry.width, geometry.height)?;
    install_text_input(&mut window, text_input);
    window.set_position(geometry.x, geometry.y);
    Ok(window)
}

fn fallback_window_geometry(
    state: WindowVisualState,
    normal_size: (usize, usize),
    position: (isize, isize),
) -> WindowGeometry {
    fallback_window_geometry_with_work_area(
        state,
        normal_size,
        position,
        native_window::work_area(),
    )
}

fn fallback_window_geometry_with_work_area(
    state: WindowVisualState,
    normal_size: (usize, usize),
    position: (isize, isize),
    work_area: Option<WindowGeometry>,
) -> WindowGeometry {
    let (width, height) = fallback_window_size(state, normal_size);
    match state {
        WindowVisualState::Maximized => work_area.unwrap_or(WindowGeometry {
            x: position.0,
            y: position.1,
            width,
            height,
        }),
        WindowVisualState::Normal | WindowVisualState::Minimized => WindowGeometry {
            x: position.0,
            y: position.1,
            width,
            height,
        },
    }
}

fn native_state_request_confirmed(
    pending: PendingNativeWindowState,
    current_size: (usize, usize),
) -> bool {
    current_size != pending.size_at_request
}

fn native_state_request_timed_out(elapsed: Duration) -> bool {
    elapsed >= NATIVE_STATE_CONFIRMATION_TIMEOUT
}

fn request_native_maximized(window: &Window, maximized: bool) -> bool {
    native_window::request_maximized(window, maximized)
}

fn ewmh_supports_maximize(
    supported_atoms: &[u32],
    wm_state: u32,
    maximized_horz: u32,
    maximized_vert: u32,
) -> bool {
    supported_atoms.contains(&wm_state)
        && supported_atoms.contains(&maximized_horz)
        && supported_atoms.contains(&maximized_vert)
}

fn ewmh_maximize_data(maximized: bool, maximized_horz: u32, maximized_vert: u32) -> [u32; 5] {
    let action = if maximized { 1 } else { 0 };
    [action, maximized_horz, maximized_vert, 1, 0]
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "redox", target_arch = "wasm32"))
))]
mod native_window {
    use super::{
        MAX_EWMH_SUPPORTED_ATOMS, PlatformError, WindowGeometry, ewmh_maximize_data,
        ewmh_supports_maximize,
    };
    use minifb::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, Window as X11Window,
    };

    pub(super) fn request_maximized(window: &Window, maximized: bool) -> bool {
        native_window_id(window)
            .is_some_and(|window_id| send_maximize_request(window_id, maximized).is_ok())
    }

    pub(super) fn work_area() -> Option<WindowGeometry> {
        let (connection, screen_num) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots[screen_num].root;
        let current_desktop = intern_atom(&connection, b"_NET_CURRENT_DESKTOP").ok()?;
        let work_area = intern_atom(&connection, b"_NET_WORKAREA").ok()?;
        let desktop = first_property_u32(&connection, root, current_desktop, AtomEnum::CARDINAL)
            .ok()
            .flatten()
            .unwrap_or(0);
        let offset = desktop.saturating_mul(4);
        let reply = connection
            .get_property(false, root, work_area, AtomEnum::CARDINAL, offset, 4)
            .ok()?
            .reply()
            .ok()?;
        let mut values = reply.value32()?;
        let x = isize::try_from(values.next()?).ok()?;
        let y = isize::try_from(values.next()?).ok()?;
        let width = usize::try_from(values.next()?).ok()?.max(1);
        let height = usize::try_from(values.next()?).ok()?.max(1);

        Some(WindowGeometry {
            x,
            y,
            width,
            height,
        })
    }

    fn native_window_id(window: &Window) -> Option<X11Window> {
        let handle = window.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok(),
            RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
            _ => None,
        }
    }

    fn send_maximize_request(
        window: X11Window,
        maximized: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (connection, screen_num) = x11rb::connect(None)?;
        let root = connection.setup().roots[screen_num].root;
        let wm_state = intern_atom(&connection, b"_NET_WM_STATE")?;
        let maximized_horz = intern_atom(&connection, b"_NET_WM_STATE_MAXIMIZED_HORZ")?;
        let maximized_vert = intern_atom(&connection, b"_NET_WM_STATE_MAXIMIZED_VERT")?;
        if !root_supports_ewmh_maximize(
            &connection,
            root,
            wm_state,
            maximized_horz,
            maximized_vert,
        )? {
            return Err(Box::new(PlatformError(
                "window manager does not advertise EWMH maximize support".to_string(),
            )));
        }

        let event = ClientMessageEvent::new(
            32,
            window,
            wm_state,
            ewmh_maximize_data(maximized, maximized_horz, maximized_vert),
        );

        connection.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )?;
        connection.flush()?;
        Ok(())
    }

    fn intern_atom<C: Connection>(
        connection: &C,
        name: &[u8],
    ) -> Result<Atom, Box<dyn std::error::Error>> {
        Ok(connection.intern_atom(false, name)?.reply()?.atom)
    }

    fn first_property_u32<C: Connection>(
        connection: &C,
        window: X11Window,
        property: Atom,
        property_type: AtomEnum,
    ) -> Result<Option<u32>, Box<dyn std::error::Error>> {
        let reply = connection
            .get_property(false, window, property, property_type, 0, 1)?
            .reply()?;
        Ok(reply.value32().and_then(|mut values| values.next()))
    }

    fn root_supports_ewmh_maximize<C: Connection>(
        connection: &C,
        root: X11Window,
        wm_state: Atom,
        maximized_horz: Atom,
        maximized_vert: Atom,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let net_supported = intern_atom(connection, b"_NET_SUPPORTED")?;
        let reply = connection
            .get_property(
                false,
                root,
                net_supported,
                AtomEnum::ATOM,
                0,
                MAX_EWMH_SUPPORTED_ATOMS,
            )?
            .reply()?;
        let supported_atoms = reply
            .value32()
            .map(|atoms| atoms.collect::<Vec<_>>())
            .unwrap_or_default();

        Ok(ewmh_supports_maximize(
            &supported_atoms,
            wm_state,
            maximized_horz,
            maximized_vert,
        ))
    }
}

#[cfg(not(all(
    unix,
    not(any(target_os = "macos", target_os = "redox", target_arch = "wasm32"))
)))]
mod native_window {
    use super::WindowGeometry;
    use minifb::Window;

    pub(super) fn request_maximized(_window: &Window, _maximized: bool) -> bool {
        false
    }

    pub(super) fn work_area() -> Option<WindowGeometry> {
        None
    }
}

fn update_cached_toolbar(
    window: &mut Window,
    view: &ChromeView,
    cached_frame: &mut Option<Frame>,
    cached_size: (usize, usize),
) -> Result<bool, PlatformError> {
    let (width, height) = window.get_size();
    if cached_size != (width, height) || !view.is_address_bar_focused() {
        return Ok(false);
    }

    let Some(frame) = cached_frame.as_mut() else {
        return Ok(false);
    };
    if !view.render_toolbar_update(frame) {
        return Ok(false);
    }

    update_window_buffer(window, Some(frame))?;
    Ok(true)
}

fn process_edit_shortcuts(
    window: &Window,
    view: &mut ChromeView,
    clipboard: &mut ClipboardDriver,
) -> bool {
    if !view.is_address_bar_focused() || !ctrl_modifier_down(window) {
        return false;
    }

    let mut changed = false;
    for key in window.get_keys_pressed(KeyRepeat::No) {
        match edit_shortcut(true, key) {
            Some(EditShortcut::SelectAll) => {
                changed |= view.select_all_address_text();
            }
            Some(EditShortcut::Copy) => {
                if let Some(text) = view.copy_address_text() {
                    clipboard.set_text(text);
                }
            }
            Some(EditShortcut::Paste) => {
                let text = clipboard.text();
                changed |= view.paste_address_text(&text);
            }
            None => {}
        }
    }

    changed
}

fn ctrl_modifier_down(window: &Window) -> bool {
    window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl)
}

fn edit_shortcut(ctrl_down: bool, key: Key) -> Option<EditShortcut> {
    if !ctrl_down {
        return None;
    }

    match key {
        Key::A => Some(EditShortcut::SelectAll),
        Key::C => Some(EditShortcut::Copy),
        Key::V => Some(EditShortcut::Paste),
        _ => None,
    }
}

fn viewport_refresh_ready(
    resize_elapsed: Option<Duration>,
    pending: bool,
    address_bar_focused: bool,
) -> bool {
    pending
        && !address_bar_focused
        && resize_elapsed.is_some_and(|elapsed| elapsed >= VIEWPORT_REFRESH_DEBOUNCE)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyboardOutcome {
    Idle,
    Changed,
    CloseRequested,
}

fn process_keyboard_input(input: &TextInputQueue, view: &mut ChromeView) -> KeyboardOutcome {
    let mut changed = false;

    for event in input.drain() {
        match event {
            TextInputEvent::Char(ch) => {
                changed |= view.handle_text_input(ch);
            }
            TextInputEvent::Command(ChromeKeyCommand::Escape) => {
                if view.handle_key_command(ChromeKeyCommand::Escape) {
                    changed = true;
                } else {
                    return KeyboardOutcome::CloseRequested;
                }
            }
            TextInputEvent::Command(command) => {
                changed |= view.handle_key_command(command);
            }
        }
    }

    if changed {
        KeyboardOutcome::Changed
    } else {
        KeyboardOutcome::Idle
    }
}

fn key_command(key: Key) -> Option<ChromeKeyCommand> {
    match key {
        Key::Enter | Key::NumPadEnter => Some(ChromeKeyCommand::Enter),
        Key::Backspace => Some(ChromeKeyCommand::Backspace),
        Key::Delete => Some(ChromeKeyCommand::Delete),
        Key::Escape => Some(ChromeKeyCommand::Escape),
        _ => None,
    }
}

fn pointer_position((mouse_x, mouse_y): (f32, f32)) -> Option<PointerPosition> {
    Some(PointerPosition {
        x: coord_to_isize(mouse_x)?,
        y: coord_to_isize(mouse_y)?,
    })
}

fn hit_position(pointer: PointerPosition) -> Option<(usize, usize)> {
    let x = usize::try_from(pointer.x).ok()?;
    let y = usize::try_from(pointer.y).ok()?;
    if x > 100_000 || y > 100_000 {
        return None;
    }

    Some((x, y))
}

fn current_window_position(window: &Window, fallback: (isize, isize)) -> (isize, isize) {
    let position = window.get_position();
    if position == (0, 0) {
        fallback
    } else {
        position
    }
}

fn dragged_window_position(
    window_position: (isize, isize),
    drag: WindowDrag,
    pointer: PointerPosition,
) -> (isize, isize) {
    let delta_x = pointer.x.saturating_sub(drag.grab_x);
    let delta_y = pointer.y.saturating_sub(drag.grab_y);
    (
        window_position.0.saturating_add(delta_x),
        window_position.1.saturating_add(delta_y),
    )
}

fn coord_to_isize(value: f32) -> Option<isize> {
    if !value.is_finite() || !(-100_000.0..=100_000.0).contains(&value) {
        return None;
    }

    format!("{:.0}", value.round()).parse().ok()
}

fn loop_sleep(dragging: bool, text_input_active: bool) -> Duration {
    if dragging {
        DRAG_SLEEP
    } else if text_input_active {
        INPUT_SLEEP
    } else {
        IDLE_SLEEP
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DRAG_SLEEP, EditShortcut, FALLBACK_MAXIMIZED_HEIGHT, FALLBACK_MAXIMIZED_WIDTH, IDLE_SLEEP,
        INPUT_SLEEP, MINIMIZED_HEIGHT, MINIMIZED_WIDTH, NATIVE_STATE_CONFIRMATION_TIMEOUT,
        PendingNativeWindowState, PointerPosition, TextInputEvent, TextInputQueue,
        VIEWPORT_REFRESH_DEBOUNCE, WindowDrag, WindowGeometry, coord_to_isize,
        dragged_window_position, edit_shortcut, ewmh_maximize_data, ewmh_supports_maximize,
        fallback_window_geometry_with_work_area, fallback_window_size, key_command, loop_sleep,
        native_state_request_confirmed, native_state_request_timed_out, next_window_visual_state,
        viewport_refresh_ready,
    };
    use minifb::{InputCallback, Key};
    use slate_chrome::{ChromeKeyCommand, WindowVisualState};
    use std::time::{Duration, Instant};

    #[test]
    fn converts_finite_coordinates_for_dragging() {
        assert_eq!(coord_to_isize(10.4), Some(10));
        assert_eq!(coord_to_isize(10.6), Some(11));
        assert_eq!(coord_to_isize(f32::NAN), None);
    }

    #[test]
    fn dragged_position_tracks_pointer_delta() {
        let drag = WindowDrag {
            grab_x: 20,
            grab_y: 12,
        };
        let pointer = PointerPosition { x: 45, y: 30 };

        assert_eq!(dragged_window_position((100, 80), drag, pointer), (125, 98));
    }

    #[test]
    fn dragged_position_can_move_up() {
        let drag = WindowDrag {
            grab_x: 20,
            grab_y: 20,
        };
        let pointer = PointerPosition { x: 20, y: -8 };

        assert_eq!(dragged_window_position((100, 80), drag, pointer), (100, 52));
    }

    #[test]
    fn maximize_toggle_tracks_visual_state_without_fixed_size_assumption() {
        assert_eq!(
            next_window_visual_state(WindowVisualState::Normal),
            WindowVisualState::Maximized
        );
        assert_eq!(
            next_window_visual_state(WindowVisualState::Maximized),
            WindowVisualState::Normal
        );
        assert_eq!(
            next_window_visual_state(WindowVisualState::Minimized),
            WindowVisualState::Maximized
        );
    }

    #[test]
    fn synthetic_maximize_size_is_only_a_fallback() {
        assert_eq!(
            fallback_window_size(WindowVisualState::Normal, (1280, 720)),
            (1280, 720)
        );
        assert_eq!(
            fallback_window_size(WindowVisualState::Minimized, (1280, 720)),
            (MINIMIZED_WIDTH, MINIMIZED_HEIGHT)
        );
        assert_eq!(
            fallback_window_size(WindowVisualState::Maximized, (1280, 720)),
            (FALLBACK_MAXIMIZED_WIDTH, FALLBACK_MAXIMIZED_HEIGHT)
        );
    }

    #[test]
    fn maximize_fallback_geometry_prefers_native_work_area() {
        let work_area = WindowGeometry {
            x: 8,
            y: 32,
            width: 1912,
            height: 1040,
        };

        assert_eq!(
            fallback_window_geometry_with_work_area(
                WindowVisualState::Maximized,
                (1280, 720),
                (100, 80),
                Some(work_area)
            ),
            work_area
        );
    }

    #[test]
    fn maximize_fallback_geometry_uses_synthetic_size_without_work_area() {
        assert_eq!(
            fallback_window_geometry_with_work_area(
                WindowVisualState::Maximized,
                (1280, 720),
                (100, 80),
                None
            ),
            WindowGeometry {
                x: 100,
                y: 80,
                width: FALLBACK_MAXIMIZED_WIDTH,
                height: FALLBACK_MAXIMIZED_HEIGHT,
            }
        );
    }

    #[test]
    fn pending_native_state_is_confirmed_by_size_change() {
        let pending = PendingNativeWindowState {
            state: WindowVisualState::Maximized,
            size_at_request: (1280, 720),
            requested_at: Instant::now(),
        };

        assert!(!native_state_request_confirmed(pending, (1280, 720)));
        assert!(native_state_request_confirmed(pending, (1920, 1040)));
    }

    #[test]
    fn pending_native_state_times_out_after_confirmation_window() {
        assert!(!native_state_request_timed_out(
            NATIVE_STATE_CONFIRMATION_TIMEOUT - Duration::from_millis(1)
        ));
        assert!(native_state_request_timed_out(
            NATIVE_STATE_CONFIRMATION_TIMEOUT
        ));
    }

    #[test]
    fn ewmh_maximize_message_sets_and_clears_both_axes() {
        assert_eq!(ewmh_maximize_data(true, 11, 12), [1, 11, 12, 1, 0]);
        assert_eq!(ewmh_maximize_data(false, 11, 12), [0, 11, 12, 1, 0]);
    }

    #[test]
    fn ewmh_maximize_requires_window_manager_support() {
        assert!(ewmh_supports_maximize(&[10, 11, 12], 10, 11, 12));
        assert!(!ewmh_supports_maximize(&[10, 11], 10, 11, 12));
        assert!(!ewmh_supports_maximize(&[], 10, 11, 12));
    }

    #[test]
    fn dragging_uses_fast_loop_sleep() {
        assert_eq!(loop_sleep(true, false), DRAG_SLEEP);
        assert_eq!(loop_sleep(false, true), INPUT_SLEEP);
        assert_eq!(loop_sleep(false, false), IDLE_SLEEP);
        assert!(loop_sleep(true, false) < loop_sleep(false, true));
        assert!(loop_sleep(false, true) < loop_sleep(false, false));
    }

    #[test]
    fn viewport_refresh_waits_for_resize_to_settle_and_idle_text_input() {
        assert!(!viewport_refresh_ready(None, true, false));
        assert!(!viewport_refresh_ready(
            Some(VIEWPORT_REFRESH_DEBOUNCE - Duration::from_millis(1)),
            true,
            false
        ));
        assert!(!viewport_refresh_ready(
            Some(VIEWPORT_REFRESH_DEBOUNCE),
            true,
            true
        ));
        assert!(!viewport_refresh_ready(
            Some(VIEWPORT_REFRESH_DEBOUNCE),
            false,
            false
        ));
        assert!(viewport_refresh_ready(
            Some(VIEWPORT_REFRESH_DEBOUNCE),
            true,
            false
        ));
    }

    #[test]
    fn maps_ctrl_edit_shortcuts() {
        assert_eq!(edit_shortcut(true, Key::A), Some(EditShortcut::SelectAll));
        assert_eq!(edit_shortcut(true, Key::C), Some(EditShortcut::Copy));
        assert_eq!(edit_shortcut(true, Key::V), Some(EditShortcut::Paste));
        assert_eq!(edit_shortcut(false, Key::V), None);
        assert_eq!(edit_shortcut(true, Key::Enter), None);
    }

    #[test]
    fn maps_editing_keys_to_chrome_commands() {
        assert_eq!(key_command(Key::Enter), Some(ChromeKeyCommand::Enter));
        assert_eq!(key_command(Key::NumPadEnter), Some(ChromeKeyCommand::Enter));
        assert_eq!(
            key_command(Key::Backspace),
            Some(ChromeKeyCommand::Backspace)
        );
        assert_eq!(key_command(Key::Delete), Some(ChromeKeyCommand::Delete));
        assert_eq!(key_command(Key::Escape), Some(ChromeKeyCommand::Escape));
        assert_eq!(key_command(Key::A), None);
    }

    #[test]
    fn text_callback_queues_text_and_pressed_commands() {
        let queue = TextInputQueue::default();
        let mut callback = queue.callback();

        callback.add_char(u32::from('s'));
        callback.set_key_state(Key::Enter, true);
        callback.set_key_state(Key::Enter, false);

        assert_eq!(
            queue.drain(),
            [
                TextInputEvent::Char('s'),
                TextInputEvent::Command(ChromeKeyCommand::Enter)
            ]
        );
    }

    #[test]
    fn text_callback_suppresses_ctrl_modified_text() {
        let queue = TextInputQueue::default();
        let mut callback = queue.callback();

        callback.set_key_state(Key::LeftCtrl, true);
        callback.add_char(u32::from('v'));
        callback.set_key_state(Key::LeftCtrl, false);
        callback.add_char(u32::from('s'));

        assert_eq!(queue.drain(), [TextInputEvent::Char('s')]);
    }
}
