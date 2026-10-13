# rocketviz: OpenRocket → Google Earth / 3D globe

Runs the simulations in your `.ork` with OpenRocket itself (headless), then writes:

- **`<rocket>.kml`**: for Google Earth. Contains:
  - an animated track you play with the time slider
  - a boost/coast/descent path with a curtain down to the ground
  - event markers
  - a recovery line from the pad to the landing spot, with distance and bearing
  - Monte Carlo landing scatter with 1σ/2σ/3σ landing zones
  - a wind sweep showing where it lands at 0–8 m/s, on the same wind heading
- **`<rocket>_globe.html`**: a 3D viewer (CesiumJS + Esri imagery). It needs an internet connection and must be served over http (browsers block Cesium's web workers on `file://` pages), so open it with `--serve`. Features:
  - play and scrub the flight
  - switch motors
  - toggle layers
  - double-click the map to move the launch site

## Setup
```
uv sync
curl -LO https://github.com/openrocket/openrocket/releases/download/release-24.12/OpenRocket-24.12.jar
```
Put the JAR next to `orsim.py`. Needs Java 17+.

## Use
```
uv run rocketviz.py MyRocket.ork --serve                # simulate, write the viewer, open it in a browser
uv run rocketviz.py MyRocket_globe.html --serve         # reopen an existing viewer without re-simulating
uv run rocketviz.py MyRocket.ork --lat 40.3 --lon -104.9 --wind 4 --wind-from 270 --mc 300
```
Options:
- `--sims 0,2`: pick which simulations to run
- `--sigma-wind`, `--sigma-dir`, `--sigma-rod`: set the Monte Carlo spread
- `--sweep-max`: set the top of the wind sweep
- `--json`: dump the raw trajectories

## Flight app (React)
`web/` is a docking workspace built with React, Fluent UI 9 and dockview. It has these panels:
- **3D view:** CesiumJS.
- **Map:** a 2D map (MapLibre).
- **Plots:** pick any of the ~57 series OpenRocket records (altitude, velocities, accelerations, forces, stability, Mach, …).
- **Flight:** recovery numbers and Monte Carlo statistics.

All of them follow one playback clock:
- Clicking or dragging a plot scrubs every view.
- Hovering a plot previews that moment on both maps.
- Panels can be dragged, tabbed, resized or maximized; the layout is remembered.

Build it once:
```
cd web && npm install && npm run build
```
Then simulate and open:
```
uv run rocketviz.py MyRocket.ork --app --serve      # writes MyRocket.rocketviz.json and opens the app
uv run rocketviz.py MyRocket.rocketviz.json             # reopen saved results without re-simulating
```
For development, run `cd web && npm run dev`. It serves the newest `*.rocketviz.json` in the repo, or `$ROCKETVIZ_DATA`, at `/data.json`. The app can also open a `.rocketviz.json` by file picker or drag-and-drop.

The single-file viewer (`--html`) remains available.

## Desktop app (Rust + Tauri, in development)
This app replaces the Python pipeline. It has the same workspace as the React app, plus simulation:
- **File → Open** takes an OpenRocket design (`.ork`) or a saved flight (`.rocketviz.json`).
- **Simulate** shows each configuration in the design. You can:
  - change the launch conditions (a blank field keeps the design's value)
  - set the Monte Carlo runs, seed, wind sweep and time step
  - customize the dispersions
- While the analysis runs, the nominal flight appears first, and the Monte Carlo landings fill in on both maps.
- **Cancel** stops the run and restores the previous flight.
- **File → Save flight as…** writes a `.rocketviz.json`. **Export KML** writes the Google Earth file.
- **File → Settings…** chooses the map imagery (Esri or USGS) and stores an optional ArcGIS API key. The key is checked before it is saved.

Simulation engines are separate programs; see `docs/engine-protocol.md`. Until the installers bundle OpenRocket and Java, build the engine once. This needs Java 21:
```
cd engines/openrocket && ./gradlew assembleEngine
npx --prefix web tauri dev
```
Environment variables:
- `ROCKETVIZ_ENGINES`: extra engine folders (a path list).
- `JAVA_HOME`: the Java to run engines with.
- `ROCKETVIZ_ESRI_KEY`: an ArcGIS key for this run only, without saving it.

## Notes
- **Wind direction is the direction the wind blows from**, as in OpenRocket. With 90° (east wind), Fizz drifts west.
- Heights are shown above ground level at the pad. Terrain isn't modeled yet. OpenRocket's *site altitude* only affects air density.
- **What the Monte Carlo varies:**
  - wind speed
  - wind direction
  - turbulence seed
  - rod tilt error (random direction)
- **Not varied yet:** motor impulse, Cd, mass, and deployment timing.
