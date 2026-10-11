# 0001: Initial Slate Architecture

Status: accepted initial direction; UI implementation details below are historical.

The initial decision established a safe Rust policy and explicit crate boundaries
for browser chrome, apps, browser-core, rendering, networking, routing, protocols,
privacy, storage, platform integration, and developer automation.

The first runnable UI used a portable framebuffer window through `minifb`, with
chrome inspired by the initial Slate concept: left app rail, top tab strip,
navigation toolbar, address bar, and a quiet home viewport.

Servo remains a pinned submodule at `third_party/servo` from
`https://github.com/gabrieldiego/servo`. The initial `slate-rendering` adapter
created a Servo `WebView` and captured a software-rendered frame.

## Current Implementation

The desktop UI has since moved to a Servo-derived shell using `egui`, `winit`,
and OpenGL. It embeds Servo directly and forwards input to interactive webviews.
`ServoBackend` remains a separate software-rendered adapter. See the
[current UI architecture](../architecture/ui.md) and
[Servo integration](../architecture/servo.md) for these boundaries.

The copied shell still contains unsafe code; the
[chrome migration TODO](../roadmap.md#chrome-safe-rust-migration) tracks bringing
it into compliance. The original safe Rust and narrow-boundary goals remain;
this implementation record does not declare those migrations complete.
