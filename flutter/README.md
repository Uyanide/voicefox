# Voicefox Flutter frontend

Flutter is intentionally not bootstrapped against TUI internals. The Rust ABI is exposed through `voicefox-ffi` and `flutter_rust_bridge`.

Phase 3 starts with the stable command/state/event boundary. Native playback bootstrap remains outside the Flutter ABI so desktop can keep libmpv while Android/iOS select their native backends.

## Generation

Use `flutter_rust_bridge_codegen generate` from the repository root after installing the matching 2.x codegen tool. Generated Dart bindings belong under `flutter/lib/src/bridge/` and are not handwritten.
