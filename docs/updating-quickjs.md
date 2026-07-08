Updating QuickJS
================

Slate embeds QuickJS from the `projects/quickjs` dependency checkout.

1.  Update the QuickJS checkout to the revision Slate should consume.

2.  Rebuild the local dependencies and the `jotter` target.

3.  Run the benchmark report in memory and coverage mode so the JavaScript
    engine footprint and exercised source lines can be compared against the
    previous baseline.

The DOM bindings use Slate's stack-style JavaScript API in
`content/handlers/javascript/quickjs/api.h`; QuickJS internals should stay
inside that implementation layer unless a binding explicitly needs runtime
state.
