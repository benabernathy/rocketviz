// Shape of <rocket>.orviz.json, written by `orviz.py --app`.

export interface Series {
  key: string;
  name: string;
  symbol: string;
  unit: string;
  group: string;
  data: (number | null)[];
}

export interface FlightEvent {
  type: string;
  t: number;
}

export interface Conditions {
  wind_speed: number;
  wind_dir_deg: number;
  turbulence: number;
  rod_angle_deg: number;
  rod_dir_deg: number;
  rod_length: number;
  launch_into_wind: boolean;
  lat: number;
  lon: number;
  site_alt: number;
}

export interface Summary {
  max_alt: number;
  max_v: number;
  apogee_t: number;
  flight_t: number;
  ground_v: number;
  rod_v: number;
}

export interface McLanding {
  e: number;
  n: number;
  wind: number;
  dir: number;
  apogee: number;
}

export interface SweepLanding {
  wind: number;
  e: number;
  n: number;
  apogee: number;
}

export interface Sim {
  name: string;
  config: string;
  conditions: Conditions;
  summary: Summary;
  events: FlightEvent[];
  /** Time (s), east / north of pad (m), altitude AGL (m) — one entry per sample. */
  t: number[];
  e: number[];
  n: number[];
  alt: number[];
  series: Series[];
  mc: McLanding[];
  sweep: SweepLanding[];
}

export interface FlightData {
  version: 1;
  rocket: string;
  pad: { lat: number; lon: number };
  site_alt: number;
  sims: Sim[];
}
