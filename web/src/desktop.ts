// The desktop app's Rust commands (src-tauri/src/lib.rs), typed. Only available when isTauri().
import { invoke, type Channel } from "@tauri-apps/api/core";
import type { McLanding, Sim, SweepLanding } from "./types";

/** A launch parameter an engine accepts (engine protocol `initialize`). */
export interface ParameterDef {
  key: string;
  name: string;
  unit: string;
  kind: "standard" | "engine";
  min?: number | null;
  max?: number | null;
  description?: string | null;
}

/** One runnable setup in a design (OpenRocket: a simulation and its motor configuration). */
export interface ConfigInfo {
  id: string;
  name: string;
  motor: string;
  /** Launch conditions by parameter key (numbers, plus engine extras such as launch_into_wind). */
  defaults: Record<string, number | boolean | string>;
}

export interface EngineEntry {
  id: string;
  name: string;
  version: string;
  license: string | null;
  extensions: string[];
  dir: string;
}

export interface DesignInfo {
  name: string;
  path: string;
  engine: EngineEntry;
  configs: ConfigInfo[];
  parameters: ParameterDef[];
}

export interface EngineReport {
  engines: EngineEntry[];
  problems: string[];
  java: string | null;
  java_error: string | null;
}

export type Dispersion =
  | { kind: "normal"; sigma: number }
  | { kind: "half_normal"; sigma: number }
  | { kind: "uniform"; min: number; max: number };
export type ParamDispersion = { key: string } & Dispersion;

export interface AnalysisSettings {
  overrides: Record<string, number>;
  seed: number;
  mc_runs: number;
  /** null: rocketviz's defaults, scaled to each config's wind. */
  dispersions: ParamDispersion[] | null;
  mc_time_step: number;
  sweep_max: number;
}

export type AnalysisEvent =
  | { stage: "nominal"; config_id: string }
  | { stage: "nominal_done"; config_id: string; sim: Sim }
  | { stage: "monte_carlo"; config_id: string; done: number; total: number; landing: McLanding | null }
  | { stage: "sweep"; config_id: string; done: number; total: number; landing: SweepLanding | null }
  | { stage: "config_done"; config_id: string };

export interface ImageryProviderInfo {
  id: string;
  name: string;
  attribution: string;
  max_zoom: number;
  coverage: string | null;
  keyed: boolean;
  has_key: boolean;
}

/** What run_analysis rejects with when the user cancels. */
export const CANCELLED = "cancelled";

export const desktop = {
  readFlight: (path: string) => invoke<string>("read_flight", { path }),
  adoptFlight: (text: string) => invoke<void>("adopt_flight", { text }),
  saveFlight: (path: string) => invoke<void>("save_flight", { path }),
  exportKml: (path: string) => invoke<void>("export_kml", { path }),
  engineReport: () => invoke<EngineReport>("engine_report"),
  designExtensions: () => invoke<string[]>("design_extensions"),
  openDesign: (path: string) => invoke<DesignInfo>("open_design", { path }),
  defaultDispersions: (windSpeed: number) => invoke<ParamDispersion[]>("default_dispersions", { windSpeed }),
  runAnalysis: (configIds: string[], settings: AnalysisSettings, onEvent: Channel<AnalysisEvent>) =>
    invoke<string>("run_analysis", { configIds, settings, onEvent }),
  cancelAnalysis: () => invoke<void>("cancel_analysis"),
  imageryProviders: () => invoke<ImageryProviderInfo[]>("imagery_providers"),
  setImageryKey: (provider: string, key: string) => invoke<void>("set_imagery_key", { provider, key }),
  smokeDesign: () => invoke<{ design: string; out_dir: string } | null>("smoke_design"),
};
