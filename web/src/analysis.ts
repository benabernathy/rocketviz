// Running an analysis in the desktop app. The flight appears as soon as the first nominal flight is done, and the
// Monte Carlo and wind-sweep landings fill in as each case finishes (applied in batches a few times a second).
import { Channel } from "@tauri-apps/api/core";
import { CANCELLED, desktop, type AnalysisEvent, type AnalysisSettings, type ConfigInfo, type DesignInfo } from "./desktop";
import { baseName, nanify, normalizeSim, validate } from "./data";
import { useStore, type RunProgress } from "./store";
import type { FlightData, Sim } from "./types";

const FLUSH_MS = 200;

/** "Simulation 1 (A8-5)". */
export const configLabel = (c: ConfigInfo) => {
  const motor = c.motor.replace(/[[\]]/g, "").trim();
  return motor ? `${c.name} (${motor})` : c.name;
};

/** Overall fraction done: configs in turn; within one, the nominal flight, Monte Carlo, then the sweep. */
export function runFraction(r: RunProgress) {
  const within = r.stage === "nominal" ? 0.02
    : r.stage === "monte_carlo" ? 0.05 + 0.85 * (r.total ? r.done / r.total : 0)
    : 0.9 + 0.1 * (r.total ? r.done / r.total : 0);
  return Math.min(1, (r.index + within) / Math.max(1, r.configs.length));
}

/**
 * Simulates the chosen configs of the open design. Resolves when done (the result replaces the flight shown) or
 * cancelled (the previous flight comes back); rejects with the engine's error otherwise, also restoring it.
 */
export async function simulate(design: DesignInfo, configIds: string[], settings: AnalysisSettings): Promise<"done" | "cancelled"> {
  const before = useStore.getState();
  const previous = { data: before.data, dataName: before.dataName, sourcePath: before.sourcePath, dirty: before.dirty, simIdx: before.simIdx };
  const configs = design.configs.filter(c => configIds.includes(c.id));
  const order = configs.map(c => c.id);
  const sims: Sim[] = [];
  const slot = new Map<string, number>();
  let header: Omit<FlightData, "sims"> | null = null;
  let timer = 0;

  const progress = (p: Partial<RunProgress>) => useStore.setState(s => ({ run: s.run && { ...s.run, ...p } }));
  const flush = () => {
    timer = 0;
    if (header) useStore.getState().updateData({ ...header, sims: [...sims] });
  };
  const later = () => { if (!timer) timer = window.setTimeout(flush, FLUSH_MS); };

  const channel = new Channel<AnalysisEvent>();
  channel.onmessage = ev => {
    const index = Math.max(0, order.indexOf(ev.config_id));
    switch (ev.stage) {
      case "nominal":
        progress({ index, stage: "nominal", done: 0, total: 0 });
        break;
      case "nominal_done": {
        slot.set(ev.config_id, sims.length);
        sims.push(normalizeSim(ev.sim));
        if (!header) {
          // First result: show it (this frames the flight and moves the pad to its site).
          const c = ev.sim.conditions;
          header = { version: 1, rocket: design.name, pad: { lat: c.lat, lon: c.lon }, site_alt: c.site_alt };
          useStore.getState().setData({ ...header, sims: [...sims] }, baseName(design.path));
          useStore.setState({ sourcePath: design.path, dirty: true });
        } else {
          flush();
        }
        break;
      }
      case "monte_carlo":
      case "sweep": {
        progress({ index, stage: ev.stage, done: ev.done, total: ev.total });
        const k = slot.get(ev.config_id);
        if (k == null || !ev.landing) break;
        const s = sims[k];
        sims[k] = ev.stage === "monte_carlo"
          ? { ...s, mc: [...s.mc, nanify(ev.landing)] }
          : { ...s, sweep: [...s.sweep, nanify(ev.landing)].sort((a, b) => a.wind - b.wind) };
        later();
        break;
      }
      case "config_done":
        break;
    }
  };

  useStore.setState({
    run: { configs: configs.map(configLabel), index: 0, stage: "nominal", done: 0, total: 0 },
  });
  try {
    const json = await desktop.runAnalysis(configIds, settings, channel);
    clearTimeout(timer);
    useStore.getState().updateData(validate(JSON.parse(json)));
    useStore.setState({ sourcePath: design.path, dirty: true });
    return "done";
  } catch (e) {
    clearTimeout(timer);
    if (header) {
      useStore.setState({ ...previous, t: 0, playing: false });
      if (previous.data) useStore.getState().setSim(Math.min(previous.simIdx, previous.data.sims.length - 1));
    }
    if (String(e) === CANCELLED) return "cancelled";
    throw e instanceof Error ? e : new Error(String(e));
  } finally {
    useStore.setState({ run: null });
  }
}

export const cancelSimulation = () => desktop.cancelAnalysis();
