# Routing

Slate uses multiaddr as the internal representation for broadweb routing targets and protocol service endpoints.

User-facing navigation stays URL-like:

- `https://example.org`
- `ipfs://<cid>`
- `ipns://<name>`
- `/ipfs/<cid>/...`
- `/ipns/<name>/...`
- `<cid>`
- `slate://home`

Slate normalizes path-style IPFS/IPNS inputs to canonical `ipfs://` and
`ipns://` URLs before routing. Bare CIDv0 and CIDv1 values are normalized to
`ipfs://<cid>`. This keeps pasted gateway-style content paths and copied CIDs
on the broadweb adapter path instead of treating them as local files or search
terms.

Protocol adapters convert those inputs into routing plans with explicit endpoints, for example:

```text
/ip4/127.0.0.1/tcp/8080/http
/ip4/127.0.0.1/tcp/9050/socks5
/ip4/127.0.0.1/tcp/4444/http
```

Malformed or unsupported routes must be rejected before network activity.
