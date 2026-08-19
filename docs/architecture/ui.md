# UI Architecture

Slate's desktop chrome currently starts from Servo's default desktop shell and uses `egui` for browser UI. `winit` owns the native window and event loop, while `egui_glow` paints the chrome through OpenGL in the headed application.

Current layers:

- `crates/chrome/`: Slate browser chrome model, egui layout, desktop shell integration, and chrome rendering tests.
- `crates/platform/`: isolated OS shims when the browser needs platform-specific behavior.
- `crates/rendering/`: Slate-owned boundary into Servo rendering.
- `crates/slate/`: binary composition root.

The first screen should keep the core Slate shape visible: left app rail,
navigation controls, address bar, rail-scoped Web tab previews, and a home
viewport. Home is the user's static personal start page, Web is a singleton
broadweb front page for local discovery and history-driven exploration, and
blank tabs are transient browsing surfaces created by the `+` Web row. Native
OS shims may replace or extend platform-specific behavior later, but browser
state should remain outside platform code.

Rail apps should render their page content as internal HTML resources served
through `slate://` URLs. Chrome owns the rail, navigation, tab previews, and
other browser affordances; app page bodies should not be hardcoded as egui
surfaces unless the code is explicitly chrome UI rather than page content.

Headless GUI rendering is a focus feature for Slate. The chrome should be renderable without opening a desktop window so layout, visual regressions, interaction timing, and eventually page rendering can be tested repeatably in automation. Headless output should reuse the same Slate-owned chrome drawing path as the headed UI wherever practical, rather than maintaining a separate mock UI just for tests.

Run `make chrome-verify` to render the canonical chrome fixture into `target/slate-chrome-verification/`. The command writes complete, loading-state, and toolbar-hover full images, stable per-element crops, and `report.json` with crop metrics plus pass/warn/fail monitoring for the tracked rail, Web tab previews, toolbar, address field, and footer regions. The report keeps manual-review notes for artwork and alignment qualities that threshold metrics cannot fully judge, while previously fixed chrome issues remain represented as regression crops.

The preferred shape is one chrome layout path that emits egui output, with interchangeable framebuffer backends. A software framebuffer is useful for deterministic and sandbox-friendly tests. An OpenGL framebuffer object backend should be added when higher visual fidelity is needed, because it can let `egui_glow` paint into an offscreen target and read back the pixels that the headed renderer would produce.
