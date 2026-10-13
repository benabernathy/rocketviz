// One store drives every view. Playback time `t` changes every frame, so the 3D globe, 2D map and
// plots subscribe to it imperatively (store.subscribe) instead of re-rendering React each frame.
import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import type { FlightData, Sim } from "./types";
import { endTime, type Pad } from "./flight";

export type ThemeMode = "system" | "light" | "dark";
export interface PlotSeries { key: string; slot: number }
export interface PlotRow { id: string; series: PlotSeries[] }
export interface Layers { mc: boolean; ellipses: boolean; sweep: boolean; follow: boolean }

const PREFS_KEY = "rocketviz.prefs.v1";
const DEFAULT_PLOTS: PlotRow[] = [
  { id: "p1", series: [{ key: "altitude", slot: 0 }] },
  { id: "p2", series: [{ key: "total_velocity", slot: 0 }, { key: "vertical_velocity", slot: 1 }] },
  { id: "p3", series: [{ key: "total_acceleration", slot: 0 }] },
];

interface Prefs { theme: ThemeMode; speed: number; plots: PlotRow[]; layers: Layers }
function loadPrefs(): Partial<Prefs> {
  try { return JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}"); } catch { return {}; }
}
const prefs = loadPrefs();

interface RocketvizState {
  data: FlightData | null;
  dataName: string;
  simIdx: number;
  /** Playback time (s since sim start). */
  t: number;
  playing: boolean;
  speed: number;
  /** Time under the pointer on a plot; maps show a ghost marker there. */
  hoverT: number | null;
  pad: Pad;
  layers: Layers;
  plots: PlotRow[];
  theme: ThemeMode;
  /** Bumped to ask the 3D and 2D views to frame the whole flight. */
  frameTick: number;
  /** Dock panels currently open (maintained by DockLayout). */
  openPanels: string[];
  /** Cesium's frame-rate overlay, for performance testing. */
  showFps: boolean;

  setData(data: FlightData, name: string): void;
  setSim(i: number): void;
  setT(t: number): void;
  setPlaying(on: boolean): void;
  togglePlay(): void;
  setSpeed(x: number): void;
  setHoverT(t: number | null): void;
  setPad(p: Pad): void;
  resetPad(): void;
  setLayer(k: keyof Layers, on: boolean): void;
  addPlot(key: string): void;
  removePlot(id: string): void;
  addSeries(id: string, key: string): void;
  removeSeries(id: string, key: string): void;
  setTheme(m: ThemeMode): void;
  frame(): void;
}

let nextId = Date.now();

export const useStore = create<RocketvizState>()(subscribeWithSelector((set, get) => ({
  data: null,
  dataName: "",
  simIdx: 0,
  t: 0,
  playing: false,
  speed: prefs.speed ?? 4,
  hoverT: null,
  pad: { lat: 0, lon: 0 },
  layers: { mc: true, ellipses: true, sweep: false, follow: false, ...prefs.layers },
  plots: prefs.plots ?? DEFAULT_PLOTS,
  theme: prefs.theme ?? "system",
  frameTick: 0,
  openPanels: [],
  showFps: false,

  setData: (data, name) => set(s => ({ data, dataName: name, simIdx: 0, t: 0, playing: false, pad: { ...data.pad }, frameTick: s.frameTick + 1 })),
  setSim: i => set(s => ({ simIdx: i, t: 0, playing: false, frameTick: s.frameTick + 1 })),
  setT: t => {
    const sim = currentSim(get());
    set({ t: sim ? Math.max(0, Math.min(t, endTime(sim))) : 0 });
  },
  setPlaying: on => {
    const s = get(), sim = currentSim(s);
    if (on && sim && s.t >= endTime(sim)) set({ t: 0 });
    set({ playing: on });
  },
  togglePlay: () => get().setPlaying(!get().playing),
  setSpeed: speed => set({ speed }),
  setHoverT: hoverT => set({ hoverT }),
  setPad: pad => set(s => ({ pad, frameTick: s.frameTick + 1 })),
  resetPad: () => set(s => (s.data ? { pad: { ...s.data.pad }, frameTick: s.frameTick + 1 } : {})),
  setLayer: (k, on) => set(s => ({ layers: { ...s.layers, [k]: on } })),
  addPlot: key => set(s => ({ plots: [...s.plots, { id: `p${nextId++}`, series: [{ key, slot: 0 }] }] })),
  removePlot: id => set(s => ({ plots: s.plots.filter(p => p.id !== id) })),
  addSeries: (id, key) => set(s => ({
    plots: s.plots.map(p => {
      if (p.id !== id || p.series.some(x => x.key === key)) return p;
      // Colour follows the series: take the first free slot, never re-number the others.
      const used = new Set(p.series.map(x => x.slot));
      const slot = [0, 1, 2, 3].find(i => !used.has(i)) ?? 0;
      return { ...p, series: [...p.series, { key, slot }] };
    }),
  })),
  removeSeries: (id, key) => set(s => ({
    plots: s.plots
      .map(p => (p.id === id ? { ...p, series: p.series.filter(x => x.key !== key) } : p))
      .filter(p => p.series.length > 0),
  })),
  setTheme: theme => set({ theme }),
  frame: () => set(s => ({ frameTick: s.frameTick + 1 })),
})));

export function currentSim(s: Pick<RocketvizState, "data" | "simIdx">): Sim | null {
  return s.data?.sims[s.simIdx] ?? null;
}
export const useSim = () => useStore(s => currentSim(s));

/** Max series sharing one plot (one unit, one axis, one legend). */
export const MAX_SERIES_PER_PLOT = 4;

// Persist preferences (not the flight data or playback position).
useStore.subscribe(
  s => ({ theme: s.theme, speed: s.speed, plots: s.plots, layers: s.layers }),
  p => { try { localStorage.setItem(PREFS_KEY, JSON.stringify(p)); } catch { /* storage unavailable */ } },
  { equalityFn: (a, b) => a.theme === b.theme && a.speed === b.speed && a.plots === b.plots && a.layers === b.layers },
);

// Playback clock: one requestAnimationFrame loop advances t for everyone.
let last: number | null = null;
function tick(now: number) {
  const s = useStore.getState(), sim = currentSim(s);
  if (s.playing && sim && last != null) {
    const t = s.t + ((now - last) / 1000) * s.speed;
    if (t >= endTime(sim)) useStore.setState({ t: endTime(sim), playing: false });
    else useStore.setState({ t });
  }
  last = now;
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);
