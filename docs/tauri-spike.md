# Tauri spike: testing the desktop shell

The spike wraps the existing React app in a minimal Tauri shell. It answers one question before we build on Tauri: does the 3D/2D/plots UI run well in each platform's webview?

| Platform | Webview |
|---|---|
| macOS | WKWebView |
| Windows | WebView2 |
| Linux | WebKitGTK |

The OpenRocket engine is not part of the spike. The app opens `.rocketviz.json` files and includes a sample flight.

## Get a build

**From CI (once the repo is on GitHub):** the `desktop` workflow builds unsigned installers for each platform and attaches them as artifacts on the workflow run:
- `rocketviz-macos-arm64`: `.dmg`
- `rocketviz-windows-x64`: `.exe` and `.msi`
- `rocketviz-linux-x64`: `.AppImage` and `.deb`

**Or build locally** (needs Node 22+ and Rust stable):
```
npm ci --prefix web
npx --prefix web tauri build
```
- **Windows:** also needs the Visual Studio C++ Build Tools. WebView2 ships with Windows 10 and 11.
- **Ubuntu:** also needs:
  ```
  sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf libxdo-dev libssl-dev build-essential
  ```

Output goes to `target/release/bundle/`.

**Development** (hot reload): `npx --prefix web tauri dev`

## Run the self-test

This opens a window, loads the sample flight, waits for imagery, plays the flight for 5 seconds while counting frames, then prints a report and exits. The exit code is 0 on PASS and 1 on FAIL.

| Platform | Command |
|---|---|
| macOS | `ROCKETVIZ_SMOKE=1 /Applications/rocketviz.app/Contents/MacOS/rocketviz` (or the built `target/release/bundle/macos/rocketviz.app/...`) |
| Windows (PowerShell) | `$env:ROCKETVIZ_SMOKE=1; & "C:\Program Files\rocketviz\rocketviz.exe"` (or `target\release\rocketviz.exe`). Read the report from the temp file (below). |
| Linux | `ROCKETVIZ_SMOKE=1 ./rocketviz_0.1.0_amd64.AppImage` (or `target/release/rocketviz`) |

To also test simulation, set `ROCKETVIZ_SMOKE_DESIGN` to an `.ork` file, for example `ROCKETVIZ_SMOKE_DESIGN=$PWD/Fizz.ork`. The test then:
1. Opens the design, which starts the engine.
2. Runs 40 Monte Carlo cases plus the wind sweep for every configuration, and checks that results streamed in while it ran.
3. Cancels a long run and checks that the previous flight comes back.
4. Saves the flight, reads it back, and exports KML to `rocketviz-smoke/` in the temp folder.

This needs the OpenRocket engine, built or bundled, and Java.

The report is also saved to the temp folder as `rocketviz-smoke.txt`:
- macOS and Linux: `$TMPDIR` or `/tmp`
- Windows: `%TEMP%`. Release builds there don't attach a console, so read the report from this file.

**Linux:** the app sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` automatically, as a workaround for NVIDIA. To compare against WebKit's default renderer, run with `WEBKIT_DISABLE_DMABUF_RENDERER=0`.

## Then check by hand
1. Open **View → Show 3D frame rate**. Orbit and zoom the globe (drag, right-drag, scroll) and play the flight. The overlay should stay near the display's refresh rate.
2. Open **View → Diagnostics…** and check **GPU renderer**. If it says `llvmpipe`, `SwiftShader` or "software", WebGL isn't using the GPU.
3. Drag tabs to re-dock panels, then maximize and restore the Plots panel. Scrub a plot and confirm both maps follow.
4. Open a `.rocketviz.json` with **Open**. It should use the native file dialog.

## Results

| Platform | Machine | 3D fps | Imagery | Self-test | Notes |
|---|---|---|---|---|---|
| macOS 26 (arm64) | dev Mac, Apple GPU, 3440×1440 | 60 | ok | PASS | |
| Windows x64 | user's workstation | | ok | PASS | No problems (2026-10-12, CI build 38328610926) |
| Ubuntu 24.04 + NVIDIA | user's workstation, proprietary drivers | | ok | PASS | No problems with the default DMABUF workaround (2026-10-12, CI build 38328610926) |

**Conclusion:** Tauri works on all three v1 platforms, and the Linux 3D concern (decision 6) didn't show up on the worst-case setup. We proceed on Tauri.
