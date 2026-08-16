# Routing

Slate uses multiaddr as the internal representation for broadweb routing targets and protocol service endpoints.

User-facing navigation stays URL-like:

- `https://example.org`
- `ipfs://<cid>`
- `ipns://<name>`
- `example.onion`
- `http://example.onion`
- `tor+http://example.onion`
- `/ipfs/<cid>/...`
- `/ipns/<name>/...`
- `<cid>`
- `slate://home`

Slate normalizes path-style IPFS/IPNS inputs to canonical `ipfs://` and
`ipns://` URLs before routing. Bare CIDv0 and CIDv1 values are normalized to
`ipfs://<cid>`. This keeps pasted gateway-style content paths and copied CIDs
on the broadweb adapter path instead of treating them as local files or search
terms.

IPFS routing must preserve the raw authority casing for `ipfs://` URLs. CIDv0
values are Base58 and case-sensitive, so adapter code must not rely on
URL-host normalization for the content name.

Slate normalizes bare `.onion` hosts and explicit `http://*.onion` URLs to
`tor+http://` before routing. Explicit `https://*.onion` URLs normalize to
`tor+https://`, but that scheme must fail closed until Slate implements TLS
over the embedded Arti stream. `.onion` names must never fall through to normal
DNS or the direct HTTP transport.

Protocol adapters convert those inputs into routing plans with explicit endpoints, for example:

```text
/ip4/127.0.0.1/tcp/8080/http
/ip4/127.0.0.1/tcp/9050/socks5
/ip4/127.0.0.1/tcp/4444/http
```

Malformed or unsupported routes must be rejected before network activity.
