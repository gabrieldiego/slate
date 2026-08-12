#![forbid(unsafe_code)]

use slate_browser_core::BrowserState;
use slate_chrome::ChromeView;
use slate_platform::{WindowConfig, run_browser_window};
use slate_rendering::ServoBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = ServoBackend;
    let state = BrowserState::new(&backend);
    let view = ChromeView::new(state);
    run_browser_window(view, WindowConfig::default())?;
    Ok(())
}
