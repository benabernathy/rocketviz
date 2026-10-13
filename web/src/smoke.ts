// Desktop self-test (ROCKETVIZ_SMOKE=1): open the sample flight, wait for imagery, play it while measuring frame
// rates, then report to the Rust shell, which prints the report and exits. Used to compare webviews per platform.
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Viewer } from "cesium";
import type { Map as MapLibreMap } from "maplibre-gl";
import { SAMPLE_URL, loadFromUrl, openPath } from "./data";
import { desktop } from "./desktop";
import { simulate } from "./analysis";
import { useStore } from "./store";
import { collectDiagnostics } from "./components/Diagnostics";

/** Live handles registered by the 3D and 2D panels. */
export const debugHooks: { viewer?: Viewer; map?: MapLibreMap; imageryErrors: number } = { imageryErrors: 0 };

// Errors are recorded from startup (capped) so the self-test can report anything that went wrong while loading.
const errors: string[] = [];
// Development builds only: React StrictMode mounts twice, and Fluent's keyboard tracker (keyborg) complains about the
// second dispose. The check that logs it is compiled out of production builds.
const DEV_NOISE = import.meta.env.DEV ? [/^console\.error: Keyborg instance \w+ is being disposed incorrectly/] : [];
const record = (msg: string) => {
  if (errors.length < 100 && !DEV_NOISE.some(re => re.test(msg))) errors.push(msg.slice(0, 300));
};
addEventListener("error", e => record(`error: ${e.message}`));
addEventListener("unhandledrejection", e => record(`rejection: ${String(e.reason)}`));
const consoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  record(`console.error: ${args.map(a => (a instanceof Error ? a.message : String(a))).join(" ")}`);
  consoleError(...args);
};

const sleep = (ms: number) => new Promise(r => setTimeout(r, ms));

async function waitFor(cond: () => boolean, timeoutMs: number) {
  const t0 = performance.now();
  while (!cond()) {
    if (performance.now() - t0 > timeoutMs) return null;
    await sleep(100);
  }
  return (performance.now() - t0) / 1000;
}

/** Counts frames for `ms` while the flight plays: Cesium renders, MapLibre renders, and requestAnimationFrame. */
async function measure(ms: number) {
  const { viewer, map } = debugHooks;
  let globe = 0, map2d = 0, raf = 0, running = true;
  const offGlobe = viewer?.scene.postRender.addEventListener(() => { globe++; });
  const onMap = () => { map2d++; };
  map?.on("render", onMap);
  const tick = () => { if (running) { raf++; requestAnimationFrame(tick); } };
  requestAnimationFrame(tick);
  await sleep(ms);
  running = false;
  offGlobe?.();
  map?.off("render", onMap);
  const s = ms / 1000;
  return { globe: globe / s, map2d: map2d / s, raf: raf / s };
}

/** With ROCKETVIZ_SMOKE_DESIGN: open the design, simulate it while counting live updates, cancel a run, and save. */
async function simulationChecks(lines: string[], checks: [string, boolean][]) {
  const smoke = await desktop.smokeDesign();
  if (!smoke) return;
  const sep = smoke.out_dir.includes("\\") ? "\\" : "/";
  lines.push("", `Simulating ${smoke.design}`);

  let t0 = performance.now();
  await openPath(smoke.design);
  const design = useStore.getState().design;
  checks.push(["Design opened (engine started)", !!design && design.configs.length > 0]);
  if (!design) return;
  lines.push(`Opened with ${design.engine.name} ${design.engine.version} in ${((performance.now() - t0) / 1000).toFixed(1)} s: ${design.configs.length} configs`);
  const report = await desktop.engineReport();
  lines.push(`Engine: ${design.engine.dir}`, `Java: ${report.java ?? report.java_error}`);
  useStore.setState({ dialog: null });

  // Live updates: count data changes while the analysis runs, and when the first flight appeared.
  let updates = 0, firstAt: number | null = null;
  const off = useStore.subscribe(s => s.data, d => {
    if (!useStore.getState().run) return;
    updates++;
    if (firstAt == null && d) firstAt = performance.now();
  });
  const runs = 40, sweep = 4;
  const settings = { overrides: {}, seed: 42, mc_runs: runs, dispersions: null, mc_time_step: 0.05, sweep_max: sweep };
  t0 = performance.now();
  const outcome = await simulate(design, design.configs.map(c => c.id), settings);
  const total = (performance.now() - t0) / 1000;
  off();
  const data = useStore.getState().data!;
  lines.push(`Analysis (${design.configs.length} configs × ${runs} Monte Carlo + ${sweep + 1} sweep) in ${total.toFixed(1)} s; ` +
    `first flight shown after ${firstAt == null ? "never" : ((firstAt - t0) / 1000).toFixed(1)} s; ${updates} live updates`);
  checks.push(["Analysis finished", outcome === "done"]);
  checks.push(["Every config simulated", data.sims.length === design.configs.length]);
  checks.push(["All Monte Carlo and sweep landings present", data.sims.every(s => s.mc.length === runs && s.sweep.length === sweep + 1)]);
  checks.push(["Results streamed while running", firstAt != null && firstAt - t0 < total * 1000 * 0.8 && updates >= 3]);
  checks.push(["Flight marked unsaved", useStore.getState().dirty]);

  // Cancel: the result just computed comes back.
  const before = useStore.getState().data;
  const cancelOnFirstLanding = useStore.subscribe(s => s.run?.done ?? 0, done => { if (done > 0) desktop.cancelAnalysis(); });
  t0 = performance.now();
  const cancelled = await simulate(design, [design.configs[0].id], { ...settings, mc_runs: 2000, seed: 7 });
  cancelOnFirstLanding();
  lines.push(`Cancel took effect after ${((performance.now() - t0) / 1000).toFixed(1)} s`);
  checks.push(["Cancel stops the run and restores the previous flight", cancelled === "cancelled" && useStore.getState().data === before]);

  // Save and re-open; export KML.
  const saved = `${smoke.out_dir}${sep}smoke.rocketviz.json`;
  await desktop.saveFlight(saved);
  const reread = JSON.parse(await desktop.readFlight(saved));
  checks.push(["Saved flight reads back", reread.sims?.length === data.sims.length && reread.sims[0].mc.length === runs]);
  await desktop.exportKml(`${smoke.out_dir}${sep}smoke.kml`);
  checks.push(["KML exported", true]);
}

/** With ROCKETVIZ_SMOKE_UPDATE=1: find an update at the configured endpoint, download it, verify and install it. */
async function updateChecks(lines: string[], checks: [string, boolean][]) {
  if (!(await invoke<boolean>("smoke_update"))) return;
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check();
  lines.push("", update ? `Update found: ${update.currentVersion} -> ${update.version}` : "No update found");
  checks.push(["Update found", !!update]);
  if (!update) return;
  let bytes = 0;
  await update.downloadAndInstall(e => { if (e.event === "Progress") bytes += e.data.chunkLength; });
  lines.push(`Downloaded ${(bytes / 1e6).toFixed(1)} MB, signature verified and installed (restart to run it)`);
  checks.push(["Update downloaded, verified and installed", bytes > 0]);
}

let started = false;

export const smokeRequested = () => invoke<boolean>("smoke_enabled").catch(() => false);

export async function runSmokeIfRequested() {
  // Once per page, even when React StrictMode runs the startup effect twice (development).
  if (started) return;
  started = true;
  if (!isTauri() || !(await smokeRequested())) return;
  const lines: string[] = ["=== rocketviz smoke test ==="];
  const checks: [string, boolean][] = [];
  try {
    await loadFromUrl(SAMPLE_URL);
    const ready = await waitFor(() => !!debugHooks.viewer && !!debugHooks.map, 10000);
    checks.push(["3D and 2D views created", ready != null]);

    // The globe starts from orbit and flies to the pad, so give the camera time to arrive before timing imagery.
    const t0 = performance.now();
    await sleep(2500);
    const globeOk = await waitFor(() => !!debugHooks.viewer?.scene.globe.tilesLoaded, 30000);
    const globeT = globeOk == null ? null : (performance.now() - t0) / 1000;
    const mapOk = await waitFor(() => !!debugHooks.map?.loaded() && !!debugHooks.map?.areTilesLoaded(), 30000);
    const mapT = mapOk == null ? null : (performance.now() - t0) / 1000;
    checks.push(["3D imagery loaded", globeT != null], ["2D map loaded", mapT != null]);
    lines.push(`Imagery settled after: 3D ${globeT?.toFixed(1) ?? "timeout"} s · 2D ${mapT?.toFixed(1) ?? "timeout"} s (includes 2.5 s camera flight)`);
    checks.push(["Trajectory drawn in 3D", (debugHooks.viewer?.entities.values.length ?? 0) > 10]);
    checks.push(["Plots drawn", document.querySelectorAll(".uplot canvas").length > 0]);
    checks.push(["No imagery tile errors", debugHooks.imageryErrors === 0]);

    // Playback with continuous 3D rendering (the frame-rate overlay turns off render-on-demand).
    const s = useStore.getState();
    useStore.setState({ showFps: true });
    s.setSpeed(4);
    s.setT(0);
    s.setPlaying(true);
    await sleep(500);
    const fps = await measure(5000);
    useStore.getState().setPlaying(false);
    useStore.setState({ showFps: false });
    lines.push(`Frame rate during playback: 3D ${fps.globe.toFixed(0)} fps · 2D map ${fps.map2d.toFixed(0)} fps · page ${fps.raf.toFixed(0)} fps`);
    checks.push(["3D renders at least 20 fps", fps.globe >= 20]);
    checks.push(["Playback advanced", useStore.getState().t > 5]);
    await simulationChecks(lines, checks);
    await updateChecks(lines, checks);
  } catch (e) {
    checks.push([`Exception: ${e instanceof Error ? e.message : String(e)}`, false]);
  }

  checks.push(["No page errors", errors.length === 0]);
  lines.push("", ...checks.map(([name, ok]) => `${ok ? "PASS" : "FAIL"}  ${name}`));
  if (errors.length) lines.push("", "Errors:", ...errors.slice(0, 20).map(e => `  ${e}`));
  lines.push("", "Environment:", ...collectDiagnostics().map(([k, v]) => `  ${k}: ${v}`));
  const ok = checks.every(([, pass]) => pass);
  lines.push("", ok ? "RESULT: PASS" : "RESULT: FAIL");
  await invoke("smoke_report", { report: lines.join("\n"), ok });
}
