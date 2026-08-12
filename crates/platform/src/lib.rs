#![forbid(unsafe_code)]

use core::fmt;
use minifb::{CursorStyle, Key, MouseButton, MouseMode, ScaleMode, Window, WindowOptions};
use slate_chrome::{ChromeView, Frame, WindowCommand, WindowVisualState};
use std::time::Duration;

const MINIMIZED_WIDTH: usize = 440;
const MINIMIZED_HEIGHT: usize = 136;
const MAXIMIZED_WIDTH: usize = 1600;
const MAXIMIZED_HEIGHT: usize = 900;
const IDLE_SLEEP: Duration = Duration::from_millis(16);
const DRAG_SLEEP: Duration = Duration::from_millis(1);

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

pub fn run_browser_window(mut view: ChromeView, config: WindowConfig) -> Result<(), PlatformError> {
    let normal_size = (config.width, config.height);
    let mut visual_state = WindowVisualState::Normal;
    let mut window = create_window(&config.title, normal_size.0, normal_size.1)?;
    let mut window_position = window.get_position();
    let mut drag = None;
    let mut previous_left_down = false;
    let mut cached_frame = None;
    let mut cached_size = (0, 0);
    let mut frame_dirty = true;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let (width, height) = window.get_size();
        if cached_size != (width, height) {
            frame_dirty = true;
        }

        let left_down = window.get_mouse_down(MouseButton::Left);
        let mut window_recreated = false;

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
                            window.set_position(window_position.0, window_position.1);
                            drag = None;
                            cached_frame = None;
                            cached_size = (0, 0);
                            frame_dirty = true;
                            window_recreated = true;
                        }
                        WindowCommand::ToggleMaximize => {
                            visual_state = match visual_state {
                                WindowVisualState::Maximized => WindowVisualState::Normal,
                                WindowVisualState::Normal | WindowVisualState::Minimized => {
                                    WindowVisualState::Maximized
                                }
                            };

                            view.set_window_state(visual_state);
                            let (next_width, next_height) = match visual_state {
                                WindowVisualState::Normal => normal_size,
                                WindowVisualState::Minimized => (MINIMIZED_WIDTH, MINIMIZED_HEIGHT),
                                WindowVisualState::Maximized => (MAXIMIZED_WIDTH, MAXIMIZED_HEIGHT),
                            };
                            window_position = current_window_position(&window, window_position);
                            window = create_window(&config.title, next_width, next_height)?;
                            window.set_position(window_position.0, window_position.1);
                            drag = None;
                            cached_frame = None;
                            cached_size = (0, 0);
                            frame_dirty = true;
                            window_recreated = true;
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

        let dragging = drag.is_some();
        if dragging && cached_frame.is_some() && !frame_dirty {
            window.update();
        } else {
            if frame_dirty || cached_frame.is_none() {
                cached_frame = Some(view.render(width.max(1), height.max(1)));
                cached_size = (width, height);
                frame_dirty = false;
            }

            update_window_buffer(&mut window, cached_frame.as_ref())?;
        }

        std::thread::sleep(loop_sleep(dragging));
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
    Ok(window)
}

fn update_window_buffer(window: &mut Window, frame: Option<&Frame>) -> Result<(), PlatformError> {
    let Some(frame) = frame else {
        return Ok(());
    };

    window
        .update_with_buffer(frame.pixels(), frame.width(), frame.height())
        .map_err(|error| PlatformError(format!("failed to update Slate window: {error}")))
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

fn loop_sleep(dragging: bool) -> Duration {
    if dragging { DRAG_SLEEP } else { IDLE_SLEEP }
}

#[cfg(test)]
mod tests {
    use super::{
        DRAG_SLEEP, IDLE_SLEEP, PointerPosition, WindowDrag, coord_to_isize,
        dragged_window_position, loop_sleep,
    };

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
    fn dragging_uses_fast_loop_sleep() {
        assert_eq!(loop_sleep(true), DRAG_SLEEP);
        assert_eq!(loop_sleep(false), IDLE_SLEEP);
        assert!(loop_sleep(true) < loop_sleep(false));
    }
}
