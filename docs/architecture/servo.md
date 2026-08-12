# Servo Integration

Servo is vendored at `third_party/servo` from the project fork:

```text
git@github.com:gabrieldiego/servo.git
```

The submodule should also keep upstream Servo configured as:

```text
https://github.com/servo/servo.git
```

Slate-owned code should use `crates/rendering/` as the normal boundary into Servo. The current implementation exposes `ServoBackend`, records the vendored Servo path, and loads deterministic HTML shims for local test addresses such as `slate://tests/hello` plus local `file://` HTML pages. Browser chrome and app code should continue to call browser-core/rendering boundaries instead of importing Servo directly.

This HTML-shim layer is intentionally temporary. It gives Slate a working address-bar navigation path, tab title updates, and rendered-page tests while the real Servo compositor, event forwarding, and network document loading are introduced behind the same boundary.

GitHub deploy keys are repository-scoped. Use the Slate deploy key for `gabrieldiego/slate` and a separate Servo deploy key for `gabrieldiego/servo`.
