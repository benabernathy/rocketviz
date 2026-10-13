import { isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { FlightData, Sim } from "./types";
import { useStore } from "./store";
import { desktop } from "./desktop";

/** Bundled demo flight, so the app has something to show without a simulation. */
export const SAMPLE_URL = "samples/fizz.rocketviz.json";

const NOT_A_FLIGHT = isTauri()
  ? "Not a rocketviz flight file. Open an OpenRocket design (.ork) to simulate it."
  : "Not a rocketviz flight file. Generate one with: uv run rocketviz.py MyRocket.ork --app";

/** JSON has no NaN, so flight files store missing values as null; numbers that may be missing become NaN. */
export const nanify = <T extends object>(o: T) =>
  Object.fromEntries(Object.entries(o).map(([k, v]) => [k, v === null ? NaN : v])) as T;

export function normalizeSim(sim: Sim): Sim {
  const nan = nanify;
  return {
    ...sim,
    summary: nan(sim.summary),
    conditions: nan(sim.conditions),
    mc: sim.mc.map(nan),
    sweep: sim.sweep.map(nan),
  };
}

export function validate(d: unknown): FlightData {
  const f = d as FlightData;
  if (!f || f.version !== 1 || !Array.isArray(f.sims) || !f.sims.length) throw new Error(NOT_A_FLIGHT);
  return { ...f, sims: f.sims.map(normalizeSim) };
}

export const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
const errorOf = (e: unknown) => (e instanceof Error ? e : new Error(String(e)));

/** Shows a flight; in the desktop app the Rust side also keeps it, for Save and Export KML. */
async function show(text: string, name: string, sourcePath: string | null, adopt: boolean) {
  const data = validate(JSON.parse(text));
  if (adopt && isTauri()) await desktop.adoptFlight(text);
  useStore.getState().setData(data, name);
  useStore.setState({ sourcePath, dirty: false });
}

export async function loadFromUrl(url: string) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  await show(await r.text(), baseName(url), null, true);
}

export async function loadFromFile(file: File) {
  await show(await file.text(), file.name, null, true);
}

/** Opens a design (simulated by an engine) or a flight file, by extension. */
export async function openPath(path: string) {
  if (/\.json$/i.test(path)) {
    await show(await desktop.readFlight(path), baseName(path), path, false);
    return;
  }
  if (useStore.getState().run) throw new Error("A simulation is running. Cancel it or let it finish first.");
  useStore.setState({ busy: `Opening ${baseName(path)}…` });
  try {
    const design = await desktop.openDesign(path);
    useStore.setState({ design, dialog: "simulate" });
  } finally {
    useStore.setState({ busy: null });
  }
}

/** Open: the native dialog in the desktop app (designs and flight files), a file input in a browser. */
export function pickFile(onError: (e: Error) => void) {
  if (isTauri()) {
    (async () => {
      const designs = await desktop.designExtensions().catch(() => [] as string[]);
      const filters = [
        ...(designs.length ? [{ name: "Designs and flights", extensions: [...designs, "json"] }] : []),
        ...(designs.length ? [{ name: "Rocket designs", extensions: designs }] : []),
        { name: "rocketviz flights", extensions: ["json"] },
      ];
      const path = await open({ multiple: false, directory: false, filters });
      if (typeof path === "string") await openPath(path);
    })().catch(e => onError(errorOf(e)));
    return;
  }
  const input = document.createElement("input");
  input.type = "file";
  input.accept = ".json,application/json";
  input.onchange = () => { const f = input.files?.[0]; if (f) loadFromFile(f).catch(onError); };
  input.click();
}

/** Suggested file name next to the source: Fizz.ork -> Fizz.rocketviz.json / Fizz.kml. */
function suggest(ext: string) {
  const { sourcePath, data } = useStore.getState();
  const stem = (sourcePath ?? data?.rocket ?? "flight").replace(/(\.rocketviz)?\.[^.\\/]+$/i, "");
  return `${stem}${ext}`;
}

export async function saveFlightAs() {
  const path = await save({ defaultPath: suggest(".rocketviz.json"), filters: [{ name: "rocketviz flight", extensions: ["json"] }] });
  if (!path) return;
  await desktop.saveFlight(path);
  useStore.setState({ sourcePath: path, dataName: baseName(path), dirty: false });
}

export async function exportKml() {
  const path = await save({ defaultPath: suggest(".kml"), filters: [{ name: "Google Earth KML", extensions: ["kml"] }] });
  if (path) await desktop.exportKml(path);
}
