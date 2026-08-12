# Servo Integration

Servo is vendored at `third_party/servo` from the project fork:

```text
git@github.com:gabrieldiego/servo.git
```

The submodule should also keep upstream Servo configured as:

```text
https://github.com/servo/servo.git
```

Slate-owned code should use `crates/rendering/` as the normal boundary into Servo. The current implementation exposes a `ServoBackend` placeholder that records the vendored Servo path and supplies mock home content. Replacing the placeholder with real Servo embedding should not require browser chrome or app code to import Servo directly.

GitHub deploy keys are repository-scoped. Use the Slate deploy key for `gabrieldiego/slate` and a separate Servo deploy key for `gabrieldiego/servo`.

