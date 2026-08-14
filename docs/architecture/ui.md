# UI Architecture

Slate's desktop chrome currently starts from Servo's default desktop shell and uses `egui` for browser UI. `winit` owns the native window and event loop, while `egui_glow` paints the chrome through OpenGL in the headed application.

Current layers:

- `crates/chrome/`: Slate browser chrome model, egui layout, desktop shell integration, and chrome rendering tests.
- `crates/platform/`: isolated OS shims when the browser needs platform-specific behavior.
- `crates/rendering/`: Slate-owned boundary into Servo rendering.
- `crates/slate/`: binary composition root.

The first screen should keep the core Slate shape visible: left app rail, top browser tabs, navigation controls, address bar, and a home viewport. Native OS shims may replace or extend platform-specific behavior later, but browser state should remain outside platform code.

Headless GUI rendering is a focus feature for Slate. The chrome should be renderable without opening a desktop window so layout, visual regressions, interaction timing, and eventually page rendering can be tested repeatably in automation. Headless output should reuse the same Slate-owned chrome drawing path as the headed UI wherever practical, rather than maintaining a separate mock UI just for tests.

The preferred shape is one chrome layout path that emits egui output, with interchangeable framebuffer backends. A software framebuffer is useful for deterministic and sandbox-friendly tests. An OpenGL framebuffer object backend should be added when higher visual fidelity is needed, because it can let `egui_glow` paint into an offscreen target and read back the pixels that the headed renderer would produce.
