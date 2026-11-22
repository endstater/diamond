# Wayland responsiveness hotfix 0.3.1

The reported ANR dialog appeared when switching away from Diamond. The user
confirmed it did not appear under XWayland. The sampled UI thread was waiting in
Mesa EGL -> wl_display_dispatch_queue -> ppoll; the application resumed normally
when visible again. Empty stdout/stderr did not indicate a crash.

eframe 0.33.0 translates NativeOptions.vsync=true to glutin SwapInterval::Wait.
glutin documents that this may block a hidden Wayland window until it is visible
again. Current upstream eframe avoids a blocking swap on Wayland for this same
reason. That newer upstream integration also uses compositor-frame-callback
pacing; the pinned 0.33 integration is not claimed to include that newer code.

This hotfix sets NativeOptions.vsync=false for the Wayland backend, detected
using the nonempty WAYLAND_DISPLAY/WAYLAND_SOCKET environment variables used by
the app's default winit backend selection. X11 retains vsync=true. No repaint
timer, busy loop, global compositor setting, dependency upgrade or new library
is introduced. The actual resolution of the user's warning still needs testing
on their machine.

One startup stderr line reports version, selected backend and blocking-swap
policy, so future logs are no longer empty at startup. For an explicit A/B test:

```sh
DIAMOND_VSYNC=off ./target/release/diamond
DIAMOND_VSYNC=on ./target/release/diamond
```

The default is automatic; normal startup needs no environment overrides.

From the existing project directory, keeping target and Cargo.lock:

```sh
tar -xzf ~/Downloads/diamond-0.3.1.tar.gz --strip-components=1
cargo build --release -p diamond-app
./target/release/diamond
```

Only diamond-app is versioned to 0.3.1; library crates remain at 0.3.0 to avoid
invalidating their existing build cache. There are no new dependencies.

References:

- https://docs.rs/glutin/0.32.3/glutin/surface/enum.SwapInterval.html
- https://github.com/emilk/egui/blob/0.33.0/crates/eframe/src/native/glow_integration.rs
- https://github.com/emilk/egui/blob/main/crates/eframe/src/native/glow_integration.rs
- https://github.com/rust-windowing/winit/blob/v0.30.12/src/platform_impl/linux/mod.rs

Validation in this environment: source/asset checks and lexical review only.
Rust toolchain and a compositor session are unavailable, so compilation, the
two new Rust policy tests, background/foreground switching, CPU usage and ANR
suppression have not been executed here.
