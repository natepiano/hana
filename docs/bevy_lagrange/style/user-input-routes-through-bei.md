## User Input Routes Through BEI

Any input a caller can bind or configure must be implemented as a bevy_enhanced_input action, not a direct `ButtonInput<KeyCode>` read. bevy_lagrange's rebindability contract is built on BEI; bypassing it breaks context gating, modifier normalization, and `OrbitCamInputInternalSet` scheduling — and silently fails for OS-intercepted keys like CapsLock on macOS.

### Anti-pattern

```rust
// Wrong — not rebindable, bypasses BEI scheduling, fails for modifier keys
if keyboard.just_pressed(config.toggle_key) { ... }
```

### Correct pattern

1. Define an action in `input/actions.rs` (`#[derive(InputAction)]`)
2. Add a binding descriptor field to the relevant type in `input/bindings/descriptor.rs`
3. Install the binding in the `Installation` set alongside orbit/pan/zoom
4. Read `ActionState::Fired` in the appropriate set

### Scope

Applies to any input controlled by a field on a descriptor, preset config, or binding type. Internal modifier-guard reads inside inject systems are exempt.
