Updating QuickJS
================

Slate embeds QuickJS from the `projects/quickjs` dependency checkout. The
checkout is pinned by `QUICKJS_REF` in `scripts/deps-config.sh` so CI and local
benchmark runs do not drift with upstream `HEAD`.

1.  Update the QuickJS checkout to the revision Slate should consume.

2.  Update the default `QUICKJS_REF` in `scripts/deps-config.sh` to the selected
    commit.

3.  Keep any Slate-specific QuickJS source changes in
    `content/handlers/javascript/quickjs/quickjs-slate-compat.patch`. The
    dependency checkout under `projects/quickjs` is not part of Slate's git
    history, so edits made there must be folded into this patch before they are
    reproducible in CI or on another machine.

4.  Rebuild the local dependencies and the `jotter` target.

5.  Run the benchmark report in memory and coverage mode so the JavaScript
    engine footprint and exercised source lines can be compared against the
    previous baseline.

The DOM bindings use Slate's stack-style JavaScript API in
`content/handlers/javascript/quickjs/api.h`; QuickJS internals should stay
inside that implementation layer unless a binding explicitly needs runtime
state.
