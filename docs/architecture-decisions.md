# rocketviz architecture decisions

rocketviz is a free, open-source flight visualizer for hobby rocketry. It is moving from a Python CLI plus a web app to a **single packaged desktop app built with Rust and Tauri**. The React UI (Fluent UI 9, dockview, Cesium, MapLibre, uPlot) stays as it is.

Decided 2026-10-12.

## Shape of the system

```
engines/openrocket/   Java bridge + OpenRocket JAR + jlink'd JRE   (GPLv3, separate process)
engines/<other>/      any simulator that speaks the engine protocol
crates/core/          Rust: data model, engine host, Monte Carlo, analysis, KML export, tile proxy
src-tauri/            Tauri v2 shell: commands, progress events, bundling, updater
web/                  React UI (data layer moves from fetch() to Tauri invoke + events)
```

## Decisions

### 1. Licensing
- **Core app:** Apache-2.0.
- **OpenRocket engine plugin:** GPLv3, kept in its own folder and run as its own process. The installer ships the two side by side, which the GPL treats as aggregation, not linking.
- **Distribution:** installers include OpenRocket's license text and a link to its source.
- **Support:** optional donations (Patreon, Buy Me a Coffee). No commercial plans.

### 2. Simulation engines are pluggable
- **Process model:** each engine is a separate process that speaks a published JSON-lines protocol over stdin/stdout.
- **Manifest:** each engine ships one, declaring:
  - its name and the input formats it reads (`.ork`, `.rkt`, …)
  - its capabilities (batch runs, parallelism)
  - the series it outputs
  - the override parameters it accepts
- **Bundled default:** OpenRocket is the engine that ships in the box. The core never imports OpenRocket.
- **Possible future engines:** RocketPy, a native Rust simulator, flight-log importers (altimeter CSV).

### 3. Platforms
- **v1 targets:** macOS arm64, Windows x64 and Linux x64, built on a CI matrix from the start.
- **Not built:**
  - Intel Mac: can't be tested, and the market is shrinking.
  - Windows on ARM: uses the x64 build under emulation.
- **Linux packages:** AppImage and `.deb`.

### 4. App size and the Java runtime
- **v1 ships the stock OpenRocket 24.12 JAR:** 83 MB compressed, 182 MB unpacked. Only about 8 MB of that is OpenRocket itself.
- **Trimming comes later,** and only after a parity test suite exists. The suite runs OpenRocket's example `.ork` files plus real designs through both the stock and trimmed JARs, and the results must match.
- **Runtime:** Eclipse Temurin 21 LTS, cut down with jlink (`--strip-debug --no-man-pages --no-header-files`, includes `java.desktop`).

### 5. Code signing and updates
- **CI:** has signing and notarization steps from day one, gated on secrets being present.
- **macOS:** signed and notarized early, using the existing Apple Developer account. Every native library in the bundled JRE gets signed with the hardened runtime. The JVM needs the `allow-jit` and `allow-unsigned-executable-memory` entitlements.
- **Windows:** unsigned for early testers. Before the public 1.0, sign through SignPath Foundation (free for open source) or Azure Trusted Signing.
- **Linux:** SHA-256 checksums.
- **Updates:** the Tauri updater is built in from v1, using its own signing key.

### 6. Linux webview
- **Status:** 3D on Linux is best effort. Linux is a small user base.
- **Launch setting:** `WEBKIT_DISABLE_DMABUF_RENDERER=1` is set by default (works around a known NVIDIA rendering bug).
- **Test machine:** Ubuntu 24.04 with NVIDIA proprietary drivers, the worst-case setup.
- **Fallback order if 3D is poor:**
  1. Tune Cesium's quality settings.
  2. Use 2D, plots and the flight panel only.
  3. Only with data: switch Linux to a different webview runtime.

### 7. Monte Carlo
- **The core (Rust) owns Monte Carlo.** It generates the perturbed cases, sends them to the engine as batches, and computes the statistics: sigma ellipses, percentiles, drift.
- **Engine manifests declare which override parameters they accept.**
  - Standard: wind speed and direction, turbulence, rod angle and direction, launch site.
  - Engine-specific, for example: motor impulse scale, Cd scale.
- **The Monte Carlo UI adapts** to whatever the active engine supports.
- **Parallelism:** engines parallelize batches when they can. For single-threaded engines, the core runs several instances of the engine.
- **Progress:** streamed per completed run, so the landing scatter fills in live.

### 8. Imagery
- **Imagery providers are pluggable.**
  - **Default:** Esri World Imagery, using the project's ArcGIS Location Platform key, with attribution shown.
  - **Built-in key-free fallback:** USGS NAIP (public domain, US). Most users are in the US.
  - **Bring your own key:** Esri, MapTiler, Mapbox or Cesium ion.
- **Terrain is a follow-up after v1.** Cesium ion world terrain (free non-commercial tier) is the candidate.

### 9. Offline use
- **Tile proxy in v1:** a Rust tile proxy behind a Tauri custom URI scheme. Cesium and MapLibre fetch tiles through it. That gives one place for:
  - switching providers
  - keeping API keys out of the JavaScript
  - caching tiles according to each provider's rules
- **First update after v1: offline site packs.**
  - Downloads USGS imagery around a pad (about a 3 km radius at zoom 18, roughly 50 MB per site).
  - Only public-domain USGS imagery goes into persistent packs.
- **Offline without packs:** simulation, plots and saved results still work. Only the imagery needs a connection.

## Startup and runtime (engineering defaults, measured 2026-10-12)
- **Measured costs:**
  - JVM + OpenRocket init: 2.5 s
  - loading a `.ork`: 0.35 s
  - a warm simulation: about 57 ms
- **Defaults:**
  - The engine starts at app launch, in parallel with the UI, and stays warm.
  - An AppCDS class cache is generated at build time to cut JVM startup.
  - `-Xmx512m`.
  - The engine restarts automatically if it crashes.
- **Monte Carlo:** 150 runs take about 8.5 s single-threaded. The target is under 2 s with a parallel batch on 8 cores. That needs a check, in phase 1, that OpenRocket runs safely in parallel when each run uses its own `sim.copy()`.

## Plan
1. **Tauri spike:** wrap the current React app in a minimal Tauri shell and test it on macOS, Windows and Ubuntu/NVIDIA (3D frame rate, playback, rendering).
2. **Engine protocol + OpenRocket Java bridge:** checked against today's Python output for Fizz (same apogee, events and landing).
3. **Rust core:** engine host, Monte Carlo, analysis, KML export, tile proxy.
4. **Tauri app:** UI data layer on invoke and events; open `.ork`, re-run, Monte Carlo progress, imagery settings.
5. **Packaging:** jlink per platform, CI matrix, macOS signing and notarization, updater.
6. **Retire Python** once the Rust path matches it.
7. **After v1:** offline USGS site packs, trimmed OpenRocket JAR, terrain, Windows signing.
