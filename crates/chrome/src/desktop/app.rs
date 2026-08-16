/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Application entry point, runs the event loop.

use std::path::Path;
use std::rc::Rc;
use std::time::Instant;
use std::{env, fs};

use servo::protocol_handler::ProtocolRegistry;
use servo::{
    EventLoopWaker, Opts, Preferences, ServoBuilder, ServoUrl, UserContentManager, UserScript,
};
use slate_storage::SlateProfileDatabase;
use url::Url;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::window::WindowId;

use super::event_loop::AppEvent;
use crate::desktop::event_loop::ServoShellEventLoop;
use crate::desktop::headed_window::HeadedWindow;
use crate::desktop::headless_window::HeadlessWindow;
use crate::desktop::page_scripts::SLATE_TEXT_SELECTION_SCRIPT;
use crate::desktop::protocols;
use crate::desktop::tracing::trace_winit_event;
use crate::parser::get_default_url;
use crate::prefs::ServoShellPreferences;
use crate::running_app_state::RunningAppState;
#[cfg(feature = "gamepad")]
use crate::running_app_state::ServoshellGamepadDelegate;
use crate::window::{PlatformWindow, ServoShellWindowId};

pub(crate) enum AppState {
    Initializing,
    Running(Rc<RunningAppState>),
    ShuttingDown,
}

const SLATE_DOWNLOAD_LINK_SCRIPT: &str = r#"
(() => {
  const supportedSchemes = new Set(["http:", "https:", "ipfs:", "ipns:", "tor+http:", "tor+https:"]);
  const downloadExtensions = new Set([
    "7z", "apk", "bin", "bz2", "csv", "deb", "dmg", "doc", "docx", "exe",
    "gz", "iso", "json", "m4a", "mkv", "mov", "mp3", "mp4", "msi", "odp",
    "ods", "odt", "pdf", "ppt", "pptx", "rar", "rpm", "tar", "tgz", "txt",
    "wasm", "webm", "xls", "xlsx", "xz", "zip"
  ]);

  function closestAnchor(node) {
    let current = node && node.nodeType === Node.ELEMENT_NODE ? node : node && node.parentElement;
    while (current) {
      if (current.localName === "a" && current.href) {
        return current;
      }
      current = current.parentElement;
    }
    return null;
  }

  function targetUrl(anchor) {
    try {
      return new URL(anchor.href, document.baseURI);
    } catch (_) {
      return null;
    }
  }

  function hasPrimaryPlainClick(event) {
    return event.button === 0 && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey;
  }

  function extensionFromPath(pathname) {
    const lastSegment = pathname.split("/").filter(Boolean).pop() || "";
    const dot = lastSegment.lastIndexOf(".");
    if (dot <= 0 || dot === lastSegment.length - 1) {
      return "";
    }
    return lastSegment.slice(dot + 1).toLowerCase();
  }

  function shouldDownload(anchor, url) {
    if (anchor.hasAttribute("download")) {
      return true;
    }
    return downloadExtensions.has(extensionFromPath(url.pathname));
  }

  function isPlainOnionUrl(url) {
    const protocol = url.protocol.toLowerCase();
    const hostname = url.hostname.toLowerCase();
    return (protocol === "http:" || protocol === "https:") && hostname.endsWith(".onion");
  }

  function routedHref(url) {
    if (isPlainOnionUrl(url)) {
      return `tor+${url.href}`;
    }
    return url.href;
  }

  function filenameFromLink(anchor, url) {
    const requested = anchor.getAttribute("download");
    if (requested && requested.trim()) {
      return requested.trim();
    }
    try {
      return decodeURIComponent(url.pathname.split("/").filter(Boolean).pop() || "");
    } catch (_) {
      return url.pathname.split("/").filter(Boolean).pop() || "";
    }
  }

  document.addEventListener("click", (event) => {
    if (event.defaultPrevented || !hasPrimaryPlainClick(event)) {
      return;
    }

    const anchor = closestAnchor(event.target);
    if (!anchor || (anchor.target && anchor.target !== "_self")) {
      return;
    }

    const url = targetUrl(anchor);
    if (!url || !supportedSchemes.has(url.protocol) && !isPlainOnionUrl(url)) {
      return;
    }
    if (!shouldDownload(anchor, url)) {
      if (isPlainOnionUrl(url)) {
        event.preventDefault();
        window.location.href = routedHref(url);
      }
      return;
    }

    const params = new URLSearchParams();
    params.set("url", routedHref(url));
    const filename = filenameFromLink(anchor, url);
    if (filename) {
      params.set("filename", filename);
    }

    event.preventDefault();
    window.location.href = `slate://download?${params.toString()}`;
  }, true);
})();
"#;

pub struct App {
    opts: Opts,
    preferences: Preferences,
    servoshell_preferences: ServoShellPreferences,
    waker: Box<dyn EventLoopWaker>,
    event_loop_proxy: Option<EventLoopProxy<AppEvent>>,
    initial_url: ServoUrl,
    profile_database: SlateProfileDatabase,
    t_start: Instant,
    t: Instant,
    state: AppState,
}

impl App {
    pub fn new(
        opts: Opts,
        preferences: Preferences,
        servo_shell_preferences: ServoShellPreferences,
        event_loop: &ServoShellEventLoop,
    ) -> Self {
        let initial_url = get_default_url(
            servo_shell_preferences.url.as_deref(),
            env::current_dir().unwrap(),
            |path| fs::metadata(path).is_ok(),
            &servo_shell_preferences,
        );
        let profile_database =
            SlateProfileDatabase::open(servo_shell_preferences.settings_database_path.clone())
                .expect("failed to open Slate settings database");

        let t = Instant::now();
        App {
            opts,
            preferences,
            servoshell_preferences: servo_shell_preferences,
            waker: event_loop.create_event_loop_waker(),
            event_loop_proxy: event_loop.event_loop_proxy(),
            initial_url,
            profile_database,
            t_start: t,
            t,
            state: AppState::Initializing,
        }
    }

    /// Initialize Application once event loop start running.
    pub fn init(&mut self, active_event_loop: Option<&ActiveEventLoop>) {
        let mut protocol_registry = ProtocolRegistry::default();
        let _ = protocol_registry.register(
            "urlinfo",
            protocols::urlinfo::UrlInfoProtocolHander::default(),
        );
        let _ =
            protocol_registry.register("servo", protocols::servo::ServoProtocolHandler::default());
        let _ = protocol_registry.register(
            "slate",
            protocols::slate::SlateProtocolHandler::new(self.profile_database.clone()),
        );
        let _ = protocol_registry.register(
            "ipfs",
            protocols::broadweb::BroadwebProtocolHandler::default(),
        );
        let _ = protocol_registry.register(
            "ipns",
            protocols::broadweb::BroadwebProtocolHandler::default(),
        );
        let _ = protocol_registry.register(
            "tor+http",
            protocols::broadweb::BroadwebProtocolHandler::default(),
        );
        let _ = protocol_registry.register(
            "tor+https",
            protocols::broadweb::BroadwebProtocolHandler::default(),
        );
        let _ = protocol_registry.register(
            "resource",
            protocols::resource::ResourceProtocolHandler::default(),
        );

        let servo_builder = ServoBuilder::default()
            .opts(self.opts.clone())
            .preferences(self.preferences.clone())
            .protocol_registry(protocol_registry)
            .event_loop_waker(self.waker.clone());

        let url = self.initial_url.as_url().clone();

        let servo = servo_builder.build();
        let platform_window = self.create_platform_window(url, active_event_loop);

        #[cfg(feature = "webxr")]
        servo.register_webxr_registry(super::webxr::XrDiscoveryWebXrRegistry::new_boxed(
            platform_window.clone(),
            active_event_loop,
            &self.preferences,
        ));

        servo.setup_logging();

        let user_content_manager = Rc::new(UserContentManager::new(&servo));
        user_content_manager.add_script(Rc::new(UserScript::from(SLATE_DOWNLOAD_LINK_SCRIPT)));
        user_content_manager.add_script(Rc::new(UserScript::from(SLATE_TEXT_SELECTION_SCRIPT)));
        for script in load_userscripts(self.servoshell_preferences.userscripts_directory.as_deref())
            .expect("Loading userscripts failed")
        {
            user_content_manager.add_script(Rc::new(script));
        }

        for user_stylesheet in &self.servoshell_preferences.user_stylesheets {
            user_content_manager.add_stylesheet(user_stylesheet.clone());
        }

        let running_state = Rc::new(RunningAppState::new(
            servo,
            self.servoshell_preferences.clone(),
            self.waker.clone(),
            user_content_manager,
            self.preferences.clone(),
            self.profile_database.clone(),
            #[cfg(feature = "gamepad")]
            self.event_loop_proxy
                .clone()
                .map(ServoshellGamepadDelegate::new)
                .map(Rc::new),
        ));
        running_state.open_window(platform_window, self.initial_url.as_url().clone());

        self.state = AppState::Running(running_state);
    }

    #[servo::servo_tracing::instrument(level = "debug", skip_all)]
    fn create_platform_window(
        &self,
        url: Url,
        active_event_loop: Option<&ActiveEventLoop>,
    ) -> Rc<dyn PlatformWindow> {
        assert_eq!(
            self.servoshell_preferences.headless,
            active_event_loop.is_none()
        );

        let Some(active_event_loop) = active_event_loop else {
            return HeadlessWindow::new(&self.servoshell_preferences);
        };

        HeadedWindow::new(
            &self.servoshell_preferences,
            active_event_loop,
            self.event_loop_proxy
                .clone()
                .expect("Should always have event loop proxy in headed mode."),
            url,
        )
    }

    pub fn pump_servo_event_loop(&mut self, active_event_loop: Option<&ActiveEventLoop>) -> bool {
        let AppState::Running(state) = &self.state else {
            return false;
        };

        let create_platform_window = |url: Url| self.create_platform_window(url, active_event_loop);
        if !state.spin_event_loop(Some(&create_platform_window)) {
            self.state = AppState::ShuttingDown;
            return false;
        }
        true
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.init(Some(event_loop));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        window_event: WindowEvent,
    ) {
        let now = Instant::now();
        trace_winit_event!(
            window_event,
            "@{:?} (+{:?}) {window_event:?}",
            now - self.t_start,
            now - self.t
        );
        self.t = now;

        let AppState::Running(state) = &self.state else {
            return;
        };

        if let Some(window) = state.window(ServoShellWindowId::from(u64::from(window_id)))
            && let Some(headed_window) = window.platform_window().as_headed_window()
        {
            headed_window.handle_winit_window_event(state.clone(), window, window_event);
        }

        if !self.pump_servo_event_loop(event_loop.into()) {
            event_loop.exit();
        }
        // Block until the window gets an event
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, app_event: AppEvent) {
        let AppState::Running(state) = &self.state else {
            return;
        };

        match app_event {
            AppEvent::Waker => (),
            AppEvent::Accessibility(ref event) => {
                if let Some(window) =
                    state.window(ServoShellWindowId::from(u64::from(event.window_id)))
                    && let Some(headed_window) = window.platform_window().as_headed_window()
                {
                    headed_window.handle_winit_app_event(state.clone(), app_event);
                }
            }
            #[cfg(feature = "gamepad")]
            AppEvent::Gamepad(event, gamepad_name, gamepad_index) => {
                state.handle_gamepad_events(event, gamepad_name, gamepad_index);
            }
        }

        if !self.pump_servo_event_loop(event_loop.into()) {
            event_loop.exit();
        }

        // Block until the window gets an event
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

fn load_userscripts(userscripts_directory: Option<&Path>) -> std::io::Result<Vec<UserScript>> {
    let mut userscripts = Vec::new();
    if let Some(userscripts_directory) = &userscripts_directory {
        let mut files = std::fs::read_dir(userscripts_directory)?
            .map(|e| e.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        files.sort_unstable();
        for file in files {
            let script = std::fs::read_to_string(&file)?;
            userscripts.push(UserScript::new(script, Some(file)));
        }
    }
    Ok(userscripts)
}
