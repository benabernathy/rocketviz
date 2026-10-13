// Where map imagery comes from. In the desktop app, tiles go through the Rust tile proxy (tiles:// URL scheme),
// which picks the provider, applies API keys and caches. In a plain browser (vite dev, rocketviz.py --serve) the
// app loads Esri's public tile endpoint directly.
import { isTauri } from "@tauri-apps/api/core";

export interface Imagery {
  /** Tile URL template with {z}, {x}, {y}. */
  url: string;
  attribution: string;
  maxZoom: number;
}

const ATTRIBUTION: Record<string, string> = {
  esri: "Imagery © Esri, Maxar, Earthstar Geographics",
  usgs: "USDA, USGS The National Map: Orthoimagery",
};

const DIRECT: Record<string, string> = {
  esri: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
  usgs: "https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/{z}/{y}/{x}",
};

export function imagery(provider = "esri"): Imagery {
  const attribution = ATTRIBUTION[provider] ?? "";
  if (isTauri()) {
    // Tauri serves custom schemes as tiles://localhost on macOS/Linux and http://tiles.localhost on Windows.
    const base = /Windows/.test(navigator.userAgent) ? "http://tiles.localhost" : "tiles://localhost";
    return { url: `${base}/${provider}/{z}/{x}/{y}`, attribution, maxZoom: 19 };
  }
  return { url: DIRECT[provider] ?? DIRECT.esri, attribution, maxZoom: 19 };
}
