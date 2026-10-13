// Desktop self-test (ORVIZ_SMOKE=1): open the sample flight, wait for imagery, play it while measuring frame
// rates, then report to the Rust shell, which prints the report and exits. Used to compare webviews per platform.
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Viewer } from "cesium";
import type { Map as MapLibreMap } from "maplibre-gl";
import { SAMPLE_URL, loadFromUrl } from "./data";
import { useStore } from "./store";
import { collectDiagnostics } from "./components/Diagnostics";

/** Live handles registered by the 3D and 2D panels. */
export const debugHooks: { viewer?: Viewer; map?: MapLibreMap; imageryErrors: number } = { imageryErrors: 0 };

// Errors are recorded from startup (capped) so the self-test can report anything that went wrong while loading.
const errors: string[] = [];
const record = (msg: string) => { if (errors.length < 100) errors.push(msg.slice(0, 300)); };
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

export async function runSmokeIfRequested() {
  if (!isTauri() || !(await invoke<boolean>("smoke_enabled").catch(() => false))) return;
  const lines: string[] = ["=== orviz smoke test ==="];
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
