# Routing

Slate uses multiaddr as the internal representation for broadweb routing targets and protocol service endpoints.

User-facing navigation stays URL-like:

- `https://example.org`
- `ipfs://<cid>`
- `ipns://<name>`
- `slate://home`

Protocol adapters convert those inputs into routing plans with explicit endpoints, for example:

```text
/ip4/127.0.0.1/tcp/8080/http
/ip4/127.0.0.1/tcp/9050/socks5
/ip4/127.0.0.1/tcp/4444/http
```

Malformed or unsupported routes must be rejected before network activity.

