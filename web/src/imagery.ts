// Where map imagery comes from. In the desktop app, tiles go through the Rust tile proxy (tiles:// URL scheme),
// which picks the provider, applies API keys and caches. In a plain browser (vite dev, rocketviz.py --serve) the
// app loads the providers' public tile endpoints directly.
import { isTauri } from "@tauri-apps/api/core";
import type { ImageryProviderInfo } from "./desktop";
import { useStore } from "./store";

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

/** The providers a browser build can use (the desktop app asks the tile proxy). */
export const BROWSER_PROVIDERS: ImageryProviderInfo[] = [
  { id: "esri", name: "Esri World Imagery", attribution: ATTRIBUTION.esri, max_zoom: 19, coverage: null, keyed: false, has_key: false },
  { id: "usgs", name: "USGS Imagery (public domain)", attribution: ATTRIBUTION.usgs, max_zoom: 19, coverage: "United States", keyed: false, has_key: false },
];

/** Imagery for the chosen provider. `epoch` changes the URL after a key change so the web view doesn't reuse tiles. */
export function imagery(): Imagery {
  const { imagery: chosen, imageryEpoch } = useStore.getState();
  const provider = chosen in DIRECT ? chosen : "esri";
  const attribution = ATTRIBUTION[provider];
  if (isTauri()) {
    // Tauri serves custom schemes as tiles://localhost on macOS/Linux and http://tiles.localhost on Windows.
    const base = /Windows/.test(navigator.userAgent) ? "http://tiles.localhost" : "tiles://localhost";
    return { url: `${base}/${provider}/{z}/{x}/{y}?v=${imageryEpoch}`, attribution, maxZoom: 19 };
  }
  return { url: DIRECT[provider], attribution, maxZoom: 19 };
}

/** Calls `apply` whenever the imagery source changes (provider chosen, or a key saved). */
export function onImageryChange(apply: () => void) {
  return useStore.subscribe(s => [s.imagery, s.imageryEpoch] as const, apply, { equalityFn: (a, b) => a[0] === b[0] && a[1] === b[1] });
}
