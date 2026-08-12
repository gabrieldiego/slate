# 0001: Initial Slate Architecture

Status: accepted

Slate starts as a safe Rust workspace with explicit crate boundaries for browser chrome, apps, browser-core, rendering, networking, routing, protocols, privacy, storage, platform integration, and developer automation.

The first runnable UI uses a portable framebuffer window through `minifb`. Slate does not use a full GUI toolkit at this stage. The chrome mockup is drawn by Slate-owned safe Rust code and is inspired by the initial Slate concept screenshot: left app rail, top tab strip, navigation toolbar, address bar, and a quiet home viewport.

Servo is vendored as a submodule at `third_party/servo` from `https://github.com/gabrieldiego/servo`. The initial `slate-rendering` crate exposes the `ServoBackend` boundary and records the vendored path. Full Servo embedding remains a follow-up implementation behind that boundary.

