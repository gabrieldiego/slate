# Tor Adapter

Slate routes Tor browsing through broadwebd instead of allowing `.onion`
addresses to fall through to ordinary DNS or direct HTTP.

## Initial Scope

The first Tor adapter is retrieval-focused:

- `TorService` registers the protocol service boundary.
- `tor-arti-http` owns the embedded Arti client transport.
- Bare `example.onion` address-bar input becomes `tor+http://example.onion/`.
- Explicit `http://example.onion/...` input becomes
  `tor+http://example.onion/...`.
- Explicit `https://example.onion/...` input becomes
  `tor+https://example.onion/...`.
- `tor+http://` pages are fetched through Arti and returned as HTTP-like
  responses to Servo.

`tor+https://` is intentionally routed to the Tor adapter but rejected for now.
Slate must add TLS over the Arti stream before enabling onion HTTPS. It must
not fall back to the direct HTTPS stack.

## Privacy Boundary

The `.onion` host, path, request timing, and response metadata are handled by
the embedded Arti client and the Tor network. Slate-owned code must not resolve
`.onion` hostnames through normal DNS and must not hand them to the direct HTTP
transport.

Top-level normal-web URLs remain direct by default. Tor mode for normal
`https://` sites is a separate product decision and should require an explicit
profile or per-container routing policy.

## Rendering Boundary

Servo cannot currently override its normal `http` and `https` protocol
handlers from Slate. To keep onion traffic on the broadwebd path, Slate uses
the custom schemes `tor+http` and `tor+https` inside the browser shell and
custom protocol registry.

Relative resources on a `tor+http://` document resolve back through the Tor
custom protocol. Absolute `http://*.onion` resources inside page markup may
need a deeper renderer/network hook or a controlled rewrite step before complex
sites are fully seamless.

## State And Future Work

The initial Arti transport uses Arti's default client configuration. Before
production use, Slate should make Tor state profile-scoped under broadwebd's
state root, expose bootstrap status in detail, add stream isolation policy,
support HTTPS over Arti streams, and add external/manual onion smoke tests.
