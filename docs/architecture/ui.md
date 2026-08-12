# UI Architecture

The initial UI is a code-native mockup rendered into a pixel buffer. It intentionally avoids a full GUI toolkit.

Current layers:

- `crates/chrome/`: Slate browser chrome model and drawing code.
- `crates/platform/`: portable OS window/framebuffer host.
- `crates/slate/`: binary composition root.

Text is rasterized from system fonts through pure Rust drawing code and falls back to a tiny bitmap font if no font can be loaded. The first screen should keep the core Slate shape visible: left app rail, top browser tabs, navigation controls, address bar, and a home viewport. Later native OS shims can replace or extend the platform layer without moving browser state into platform code.
