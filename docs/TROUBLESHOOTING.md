# RMK local build troubleshooting

This file records build problems seen during PG1KB RMK bring-up and their fixes.

## bindgen cannot find libclang

Typical errors:

```text
failed to run custom build command for `nrf-mpsl-sys`
Unable to find libclang
```

Install clang and libclang:

```bash
sudo apt update
sudo apt install -y clang libclang-dev
```

Normally that is enough on Ubuntu/WSL. If bindgen still cannot find libclang:

```bash
find /usr/lib -name 'libclang.so*' -o -name 'libclang-*.so*' 2>/dev/null
```

Then point bindgen at the directory containing the library, for example:

```bash
export LIBCLANG_PATH=/usr/lib/llvm-18/lib
```

Use the actual LLVM directory installed on the system.

## Vial Cargo feature disabled but keyboard.toml still defaults to Vial enabled

Typical errors:

```text
custom attribute panicked
If the "vial" Cargo feature is disabled, `host.vial_enabled` must be set to false in keyboard.toml.

error: `#[panic_handler]` function required, but not found
```

During the initial PG1KB bring-up, both Vial and Rynk are intentionally disabled in `Cargo.toml`. RMK's `keyboard.toml` defaults `host.vial_enabled` to `true` when the field is omitted, so the configuration must explicitly match the Cargo features:

```toml
[host]
vial_enabled = false
rynk_enabled = false
```

The `#[panic_handler]` error may appear as a secondary error because the `#[rmk_central]` macro failed before it could generate the normal firmware entry point. Fix the host-protocol mismatch first and rebuild before diagnosing the panic-handler message separately.

After pulling the fix:

```bash
cd ~/rmk-dev/rmk-pg1kb-proto-ph3
git pull --ff-only
cargo build --release --bin central
```

`build.rs` watches `keyboard.toml`, so future changes to this configuration should trigger recompilation automatically.
