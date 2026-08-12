# Servo Integration

Servo is vendored at `third_party/servo` from the project fork:

```text
git@github.com:gabrieldiego/servo.git
```

The submodule should also keep upstream Servo configured as:

```text
https://github.com/servo/servo.git
```

Slate-owned code should use `crates/rendering/` as the normal boundary into Servo. The current implementation exposes `ServoBackend`, records the vendored Servo path, creates a Servo `WebView`, and captures Servo's software-rendered bitmap for display inside the Slate chrome. Browser chrome and app code should continue to call browser-core/rendering boundaries instead of importing Servo directly.

Current navigation covers deterministic internal pages such as `slate://tests/hello`, local `file://` HTML pages, HTTP(S) pages, and registered broadweb schemes. HTML, CSS, and JavaScript execution are delegated to Servo. Slate does not parse or render web content locally.

The first broadweb hook is a Servo custom protocol registry for `ipfs`, `ipns`, `i2p`, `gemini`, and `magnet`. It returns a local placeholder page until real protocol adapters and routing plans exist. Hostname-based private routes such as `.onion` and `.i2p` remain blocked before normal HTTP(S) routing because Servo does not allow overriding the `http` or `https` protocol handlers.

This embedding layer is still early. It gives Slate a working address-bar navigation path, tab title updates, local-file loading, web tests, CSS/JS smoke tests, and rendered-page tests while full compositor integration and event forwarding are introduced behind the same boundary.

GitHub deploy keys are repository-scoped. Use the Slate deploy key for `gabrieldiego/slate` and a separate Servo deploy key for `gabrieldiego/servo`.
