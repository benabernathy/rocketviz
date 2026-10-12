// 2D map (MapLibre GL): ground track, recovery, Monte Carlo zones, wind sweep, and the rocket's ground position.
import { useEffect, useRef } from "react";
import * as maplibregl from "maplibre-gl";
import type { GeoJSONSource, LngLatBoundsLike } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import maplibreWorker from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import type { Feature, FeatureCollection, Geometry } from "geojson";
import { currentSim, useStore } from "../store";
import { bearing, compass, enuToLonLat, lerp, phaseRanges, sigmaEllipse, PHASE_COLOR, SIGMA_COLOR, type Pad } from "../flight";
import type { Sim } from "../types";

// MapLibre looks for its worker next to its own module; once bundled that's wrong, so point it at the emitted asset.
maplibregl.setWorkerUrl(new URL(maplibreWorker, document.baseURI).href);

type Props = Record<string, string | number | boolean>;
const fc = (features: Feature<Geometry, Props>[]): FeatureCollection<Geometry, Props> => ({ type: "FeatureCollection", features });
const feat = (geometry: Geometry, properties: Props = {}): Feature<Geometry, Props> => ({ type: "Feature", geometry, properties });

const STYLE: maplibregl.StyleSpecification = {
  version: 8,
  glyphs: "https://demotiles.maplibre.org/font/{fontstack}/{range}.pbf",
  sources: {
    imagery: {
      type: "raster",
      tiles: ["https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}"],
      tileSize: 256,
      maxzoom: 19,
      attribution: "Imagery © Esri, Maxar, Earthstar Geographics",
    },
  },
  layers: [{ id: "imagery", type: "raster", source: "imagery" }],
};

/** Static overlays for one sim at one pad location. */
function overlays(sim: Sim, pad: Pad) {
  const ll = (e: number, n: number) => enuToLonLat(pad, e, n);
  const N = sim.t.length - 1;
  const track = phaseRanges(sim).filter(([, a, b]) => b > a).map(([phase, a, b]) =>
    feat({ type: "LineString", coordinates: sim.e.slice(a, b + 1).map((e, j) => ll(e, sim.n[a + j])) }, { color: PHASE_COLOR[phase] }));

  const le = sim.e[N], ln = sim.n[N];
  const points = [
    feat({ type: "Point", coordinates: ll(0, 0) }, { kind: "pad", label: "PAD" }),
    feat({ type: "Point", coordinates: ll(le, ln) }, { kind: "landing", label: `LANDING ${Math.round(Math.hypot(le, ln))} m ${compass(bearing(le, ln))}` }),
  ];
  const walk = feat({ type: "LineString", coordinates: [ll(0, 0), ll(le, ln)] });

  const pts = sim.mc.map(m => [m.e, m.n] as [number, number]);
  const ellipses = pts.length > 2 ? ([3, 2, 1] as const).map(k =>
    feat({ type: "Polygon", coordinates: [sigmaEllipse(pts, k).ring.map(([e, n]) => ll(e, n))] }, { color: SIGMA_COLOR[k] })) : [];
  const mc = sim.mc.map(m => feat({ type: "Point", coordinates: ll(m.e, m.n) }));

  const sweep = [
    feat({ type: "LineString", coordinates: [ll(0, 0), ...sim.sweep.map(w => ll(w.e, w.n))] }),
    ...sim.sweep.filter(w => w.wind > 0).map(w =>
      feat({ type: "Point", coordinates: ll(w.e, w.n) }, { label: `${w.wind} m/s → ${Math.round(Math.hypot(w.e, w.n))} m` })),
  ];
  return { track: fc(track), points: fc(points), walk: fc([walk]), ellipses: fc(ellipses), mc: fc(mc), sweep: fc(sweep) };
}

function boundsOf(sim: Sim, pad: Pad): LngLatBoundsLike {
  const b = new maplibregl.LngLatBounds();
  b.extend(enuToLonLat(pad, 0, 0));
  sim.e.forEach((e, i) => b.extend(enuToLonLat(pad, e, sim.n[i])));
  sim.mc.forEach(m => b.extend(enuToLonLat(pad, m.e, m.n)));
  return b;
}

function rocketPoint(t: number | null) {
  const s = useStore.getState(), sim = currentSim(s);
  if (!sim || t == null) return fc([]);
  return fc([feat({ type: "Point", coordinates: enuToLonLat(s.pad, lerp(sim, sim.e, t), lerp(sim, sim.n, t)) },
    { alt: Math.round(lerp(sim, sim.alt, t)) })]);
}

export function Map2D() {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const { pad } = useStore.getState();
    const map = new maplibregl.Map({
      container: host.current!,
      style: STYLE,
      center: [pad.lon, pad.lat],
      zoom: 15,
      doubleClickZoom: false,
      attributionControl: { compact: true },
    });
    map.addControl(new maplibregl.NavigationControl({ visualizePitch: true }), "top-right");
    map.addControl(new maplibregl.ScaleControl({ unit: "metric" }), "bottom-left");

    const unsubs: (() => void)[] = [];
    map.on("load", () => {
      const empty = fc([]);
      for (const id of ["ellipses", "mc", "sweep", "walk", "track", "points", "rocket", "ghost"]) map.addSource(id, { type: "geojson", data: empty });

      map.addLayer({ id: "ellipses-fill", type: "fill", source: "ellipses", paint: { "fill-color": ["get", "color"], "fill-opacity": 0.1 } });
      map.addLayer({ id: "ellipses-line", type: "line", source: "ellipses", paint: { "line-color": ["get", "color"], "line-width": 2 } });
      map.addLayer({ id: "mc", type: "circle", source: "mc", paint: {
        "circle-radius": 3, "circle-color": "#ffffff", "circle-opacity": 0.85, "circle-stroke-color": "#000000", "circle-stroke-width": 1 } });
      map.addLayer({ id: "sweep-line", type: "line", source: "sweep", filter: ["==", ["geometry-type"], "LineString"],
        paint: { "line-color": "#00ffff", "line-width": 2, "line-opacity": 0.7 } });
      map.addLayer({ id: "sweep-pts", type: "circle", source: "sweep", filter: ["==", ["geometry-type"], "Point"],
        paint: { "circle-radius": 4, "circle-color": "#00ffff" } });
      map.addLayer({ id: "sweep-labels", type: "symbol", source: "sweep", filter: ["==", ["geometry-type"], "Point"],
        layout: { "text-field": ["get", "label"], "text-size": 11, "text-offset": [0.8, 0], "text-anchor": "left", "text-font": ["Open Sans Regular"] },
        paint: { "text-color": "#ffffff", "text-halo-color": "#000000", "text-halo-width": 1.5 } });
      map.addLayer({ id: "walk", type: "line", source: "walk", paint: { "line-color": "#ff3b3b", "line-width": 3, "line-dasharray": [2, 1.5] } });
      map.addLayer({ id: "track", type: "line", source: "track", layout: { "line-cap": "round", "line-join": "round" },
        paint: { "line-color": ["get", "color"], "line-width": 3 } });
      map.addLayer({ id: "points", type: "circle", source: "points", paint: {
        "circle-radius": 6, "circle-color": ["match", ["get", "kind"], "pad", "#32cd32", "#ff3b3b"],
        "circle-stroke-color": ["match", ["get", "kind"], "pad", "#000000", "#ffffff"], "circle-stroke-width": 2 } });
      map.addLayer({ id: "point-labels", type: "symbol", source: "points",
        layout: { "text-field": ["get", "label"], "text-size": 12, "text-offset": [0, -1.4], "text-font": ["Open Sans Semibold"] },
        paint: { "text-color": "#ffffff", "text-halo-color": "#000000", "text-halo-width": 1.5 } });
      map.addLayer({ id: "ghost", type: "circle", source: "ghost", paint: {
        "circle-radius": 5, "circle-color": "#ffffff", "circle-opacity": 0.6, "circle-stroke-color": "#000000", "circle-stroke-width": 1 } });
      map.addLayer({ id: "rocket", type: "circle", source: "rocket", paint: {
        "circle-radius": 7, "circle-color": "#ffb547", "circle-stroke-color": "#000000", "circle-stroke-width": 2 } });
      map.addLayer({ id: "rocket-label", type: "symbol", source: "rocket",
        layout: { "text-field": ["concat", ["to-string", ["get", "alt"]], " m"], "text-size": 11, "text-offset": [1, 0], "text-anchor": "left", "text-font": ["Open Sans Semibold"] },
        paint: { "text-color": "#ffb547", "text-halo-color": "#000000", "text-halo-width": 1.5 } });

      const src = (id: string) => map.getSource(id) as GeoJSONSource;
      const rebuild = () => {
        const s = useStore.getState(), sim = currentSim(s);
        if (!sim) return;
        const o = overlays(sim, s.pad);
        (Object.keys(o) as (keyof typeof o)[]).forEach(k => src(k).setData(o[k]));
        onTime();
      };
      const applyLayers = () => {
        const { layers } = useStore.getState();
        const vis = (on: boolean) => (on ? "visible" : "none");
        map.setLayoutProperty("mc", "visibility", vis(layers.mc));
        ["ellipses-fill", "ellipses-line"].forEach(l => map.setLayoutProperty(l, "visibility", vis(layers.ellipses)));
        ["sweep-line", "sweep-pts", "sweep-labels"].forEach(l => map.setLayoutProperty(l, "visibility", vis(layers.sweep)));
      };
      const onTime = () => {
        const s = useStore.getState();
        src("rocket").setData(rocketPoint(s.t));
        src("ghost").setData(rocketPoint(s.hoverT));
        if (s.layers.follow) {
          const p = rocketPoint(s.t).features[0];
          if (p) map.setCenter((p.geometry as GeoJSON.Point).coordinates as [number, number]);
        }
      };
      const frame = () => {
        const s = useStore.getState(), sim = currentSim(s);
        if (sim) map.fitBounds(boundsOf(sim, s.pad), { padding: 40, duration: 800, maxZoom: 18 });
      };

      rebuild();
      applyLayers();
      map.fitBounds(boundsOf(currentSim(useStore.getState())!, useStore.getState().pad), { padding: 40, duration: 0, maxZoom: 18 });

      unsubs.push(
        useStore.subscribe(s => [s.data, s.simIdx, s.pad] as const, rebuild, { equalityFn: (a, b) => a.every((x, i) => x === b[i]) }),
        useStore.subscribe(s => s.layers, applyLayers),
        useStore.subscribe(s => [s.t, s.hoverT] as const, onTime, { equalityFn: (a, b) => a[0] === b[0] && a[1] === b[1] }),
        useStore.subscribe(s => s.frameTick, frame),
      );
    });

    map.on("dblclick", e => useStore.getState().setPad({ lat: e.lngLat.lat, lon: e.lngLat.lng }));

    const ro = new ResizeObserver(() => map.resize());
    ro.observe(host.current!);
    return () => { ro.disconnect(); unsubs.forEach(u => u()); map.remove(); };
  }, []);

  return <div ref={host} style={{ position: "absolute", inset: 0 }} />;
}
