// Trajectory math shared by the 3D globe, 2D map, plots and panels.
import type { Sim } from "./types";

const R = 6371000;
const COMPASS = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];

export type LonLat = [number, number];
export interface Pad { lat: number; lon: number }

/** Local east/north offset (m) from the pad -> [lon, lat] degrees. */
export function enuToLonLat(pad: Pad, e: number, n: number): LonLat {
  return [
    pad.lon + (e / (R * Math.cos((pad.lat * Math.PI) / 180))) * (180 / Math.PI),
    pad.lat + (n / R) * (180 / Math.PI),
  ];
}

export const bearing = (e: number, n: number) => ((Math.atan2(e, n) * 180) / Math.PI + 360) % 360;
export const compass = (b: number) => COMPASS[Math.floor(((b + 11.25) % 360) / 22.5)];
export const feet = (m: number) => Math.round(m * 3.28084).toLocaleString();

export const endTime = (s: Sim) => s.t[s.t.length - 1];
export const evTime = (s: Sim, type: string) => s.events.find(e => e.type === type)?.t ?? null;

/** First sample index with t[i] >= t. */
export function idxAt(s: Sim, t: number) {
  let lo = 0, hi = s.t.length - 1;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    if (s.t[m] < t) lo = m + 1; else hi = m;
  }
  return lo;
}

/** Linearly interpolated value of a per-sample array at time t. */
export function lerp(s: Sim, arr: number[], t: number) {
  const i = idxAt(s, t);
  if (i === 0) return arr[0];
  const t0 = s.t[i - 1], t1 = s.t[i], f = t1 > t0 ? (t - t0) / (t1 - t0) : 0;
  return arr[i - 1] + (arr[i] - arr[i - 1]) * f;
}

export interface State { e: number; n: number; alt: number; v: number }

export function stateAt(s: Sim, t: number): State {
  const v = s.series.find(x => x.key === "total_velocity")?.data as number[] | undefined;
  return { e: lerp(s, s.e, t), n: lerp(s, s.n, t), alt: lerp(s, s.alt, t), v: v ? lerp(s, v, t) : 0 };
}

export type PhaseKey = "boost" | "coast" | "descent";
export const PHASE_COLOR: Record<PhaseKey, string> = { boost: "#ff6f1f", coast: "#ffd700", descent: "#40c0ff" };

/** Sample index ranges for each flight phase. */
export function phaseRanges(s: Sim): [PhaseKey, number, number][] {
  const tl = evTime(s, "LIFTOFF") ?? 0;
  const tb = evTime(s, "BURNOUT") ?? tl;
  const td = evTime(s, "RECOVERY_DEVICE_DEPLOYMENT") ?? evTime(s, "APOGEE") ?? endTime(s);
  const N = s.t.length - 1;
  return [["boost", idxAt(s, tl), idxAt(s, tb)], ["coast", idxAt(s, tb), idxAt(s, td)], ["descent", idxAt(s, td), N]];
}

export function phaseAt(s: Sim, t: number): { label: string; color: string } {
  if (t <= (evTime(s, "LIFTOFF") ?? 0)) return { label: "On pad", color: "#93a3b4" };
  if (t < (evTime(s, "BURNOUT") ?? 0)) return { label: "Boost", color: PHASE_COLOR.boost };
  if (t < (evTime(s, "RECOVERY_DEVICE_DEPLOYMENT") ?? evTime(s, "APOGEE") ?? 0)) return { label: "Coast", color: PHASE_COLOR.coast };
  if (t < endTime(s)) return { label: "Under chute", color: PHASE_COLOR.descent };
  return { label: "Landed", color: "#4fd18b" };
}

export const EVENT_LABEL: Record<string, string> = {
  LAUNCH: "Ignition",
  LIFTOFF: "Liftoff",
  LAUNCHROD: "Clears rod",
  BURNOUT: "Burnout",
  EJECTION_CHARGE: "Ejection",
  APOGEE: "Apogee",
  RECOVERY_DEVICE_DEPLOYMENT: "Deploy",
  GROUND_HIT: "Landing",
  SIMULATION_END: "End",
};

/** Events worth marking on plots and the transport (deduplicated, labelled). */
export function keyEvents(s: Sim) {
  const seen = new Set<string>();
  return s.events
    .filter(e => ["LIFTOFF", "LAUNCHROD", "BURNOUT", "APOGEE", "RECOVERY_DEVICE_DEPLOYMENT", "GROUND_HIT"].includes(e.type))
    .filter(e => (seen.has(e.type) ? false : (seen.add(e.type), true)))
    .map(e => ({ ...e, label: EVENT_LABEL[e.type] ?? e.type }));
}

/** k-sigma covariance ellipse of 2D points, as a ring of [e, n] plus its full axis lengths. */
export function sigmaEllipse(pts: [number, number][], k: number) {
  const n = pts.length;
  const me = pts.reduce((a, p) => a + p[0], 0) / n, mn = pts.reduce((a, p) => a + p[1], 0) / n;
  let a = 0, b = 0, c = 0;
  for (const [e, m] of pts) { a += (e - me) ** 2; b += (e - me) * (m - mn); c += (m - mn) ** 2; }
  a /= n - 1; b /= n - 1; c /= n - 1;
  const tr = a + c, det = a * c - b * b, d = Math.sqrt(Math.max(tr * tr / 4 - det, 0));
  const l1 = tr / 2 + d, l2 = Math.max(tr / 2 - d, 0);
  const ang = Math.atan2(l1 - a, b);
  const ring: [number, number][] = [];
  for (let i = 0; i <= 72; i++) {
    const th = (i / 72) * 2 * Math.PI, x = k * Math.sqrt(l1) * Math.cos(th), y = k * Math.sqrt(l2) * Math.sin(th);
    ring.push([me + x * Math.cos(ang) - y * Math.sin(ang), mn + x * Math.sin(ang) + y * Math.cos(ang)]);
  }
  return { ring, axes: [2 * k * Math.sqrt(l1), 2 * k * Math.sqrt(l2)] as const };
}

export const SIGMA_COLOR: Record<1 | 2 | 3, string> = { 1: "#4fd18b", 2: "#ffd23f", 3: "#ff5a5a" };

/** Short motor label from a sim name like "Simulation 1 — [A8-5]". */
export const motorLabel = (s: Sim) => s.config.replace(/[[\]]/g, "").trim() || s.name;
