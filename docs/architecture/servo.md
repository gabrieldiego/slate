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

Current navigation covers deterministic internal pages such as `slate://tests/hello`, local `file://` HTML pages, broadwebd-fetched HTTP(S) pages, IPFS/IPNS gateway routes, and registered placeholder broadweb schemes. HTML, CSS, and JavaScript execution are delegated to Servo. Slate does not parse or render web content locally.

Top-level HTTP(S), `ipfs://`, and `ipns://` navigations first pass through Slate's in-process `broadwebd` service, then Servo renders HTML responses through the existing generated-document path. Fetched IPFS/IPNS HTML receives a document `<base>` pointing at the effective `ipfs://` or `ipns://` document URL when the page did not define one, so relative page assets resolve back through broadweb routing. Kubo directory fallbacks use the effective `<path>/index.html` URL for this base. Servo's custom protocol registry now fetches `ipfs://` and `ipns://` subresources through `broadwebd` as subresource requests, so CSS, JavaScript, images, and fonts do not create user download records. Non-2xx top-level responses become local response-error pages. Successful top-level non-HTML responses are spooled by broadwebd into profile-temporary download records and surfaced as download-ready pages with the saved path and size until the full downloads UI is connected. Remaining broadweb schemes such as `i2p`, `gemini`, and `magnet` still use a Servo custom protocol registry that returns a local placeholder page until real protocol adapters exist. Hostname-based private routes such as `.onion` and `.i2p` remain blocked before normal HTTP(S) routing because Servo does not allow overriding the `http` or `https` protocol handlers.

Top-level broadweb fetches copy broadwebd route context into render metrics:
profile id, selected transport id, and a short privacy-boundary label. Chrome
and future permission UI should prefer these broadwebd-supplied values over
guessing routing behavior from the displayed URL.

This embedding layer is still early. It gives Slate a working address-bar navigation path, tab title updates, local-file loading, web tests, CSS/JS smoke tests, and rendered-page tests while full compositor integration and event forwarding are introduced behind the same boundary. The rendering crate keeps Servo-backed tests in one headless smoke test because Servo process-global options can only be initialized once per test process. IPFS and IPNS rendering coverage uses a local fake gateway through a test `broadwebd` instance, including an IPFS relative stylesheet subresource, so those tests do not depend on live IPFS, IPNS, public gateways, or external network availability.

GitHub deploy keys are repository-scoped. Use the Slate deploy key for `gabrieldiego/slate` and a separate Servo deploy key for `gabrieldiego/servo`.
