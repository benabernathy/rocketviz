// 3D globe (CesiumJS): trajectory, events, recovery, Monte Carlo scatter, wind sweep, and the animated rocket.
import { useEffect, useRef } from "react";
import * as Cesium from "cesium";
import "cesium/Build/Cesium/Widgets/widgets.css";
import { currentSim, useStore } from "../store";
import {
  bearing, compass, enuToLonLat, keyEvents, lerp, phaseRanges, sigmaEllipse, PHASE_COLOR, SIGMA_COLOR, type Pad,
} from "../flight";
import type { Sim } from "../types";

const css = (c: string) => Cesium.Color.fromCssColorString(c);

function label(text: string, dy = 0, dx = 0): Cesium.LabelGraphics.ConstructorOptions {
  return {
    text, font: "500 13px system-ui, sans-serif", fillColor: Cesium.Color.WHITE, outlineColor: Cesium.Color.BLACK,
    outlineWidth: 3, style: Cesium.LabelStyle.FILL_AND_OUTLINE, pixelOffset: new Cesium.Cartesian2(dx, dy),
    horizontalOrigin: dx ? Cesium.HorizontalOrigin.LEFT : Cesium.HorizontalOrigin.CENTER,
    verticalOrigin: Cesium.VerticalOrigin.BOTTOM, disableDepthTestDistance: Number.POSITIVE_INFINITY, scale: 0.9,
  };
}

interface Built { all: Cesium.Entity[]; mc: Cesium.Entity[]; ellipses: Cesium.Entity[]; sweep: Cesium.Entity[] }

function build(viewer: Cesium.Viewer, sim: Sim, pad: Pad): Built {
  const out: Built = { all: [], mc: [], ellipses: [], sweep: [] };
  const add = (o: Cesium.Entity.ConstructorOptions, group?: Cesium.Entity[]) => {
    const e = viewer.entities.add(o);
    out.all.push(e);
    group?.push(e);
    return e;
  };
  const at = (e: number, n: number, h = 0) => Cesium.Cartesian3.fromDegrees(...enuToLonLat(pad, e, n), Math.max(h, 0));
  const N = sim.t.length - 1;

  // Trajectory by phase, with a translucent curtain to the ground.
  for (const [phase, a, b] of phaseRanges(sim)) {
    if (b <= a) continue;
    const pos: Cesium.Cartesian3[] = [];
    for (let i = a; i <= b; i++) pos.push(at(sim.e[i], sim.n[i], sim.alt[i]));
    const col = css(PHASE_COLOR[phase]);
    add({ polyline: { positions: pos, width: 3, material: col } });
    const step = Math.max(1, Math.floor(pos.length / 300));
    add({ wall: { positions: pos.filter((_, j) => j % step === 0), material: col.withAlpha(0.13), outline: false } });
  }

  // Pad, landing and the walk between them.
  const le = sim.e[N], ln = sim.n[N], dist = Math.hypot(le, ln), brg = bearing(le, ln);
  add({ position: at(0, 0), point: { pixelSize: 11, color: Cesium.Color.LIME, outlineColor: Cesium.Color.BLACK, outlineWidth: 2 }, label: label("PAD", -14) });
  add({ position: at(le, ln), point: { pixelSize: 12, color: Cesium.Color.RED, outlineColor: Cesium.Color.WHITE, outlineWidth: 2 },
    label: label(`LANDING  ${Math.round(dist)} m ${compass(brg)}`, -14) });
  add({ polyline: { positions: [at(0, 0), at(le, ln)], width: 3, clampToGround: true,
    material: new Cesium.PolylineDashMaterialProperty({ color: Cesium.Color.RED, dashLength: 14 }) } });

  // Flight events (apogee and an at-apogee deploy share one marker).
  const tApo = sim.events.find(e => e.type === "APOGEE")?.t;
  const tDep = sim.events.find(e => e.type === "RECOVERY_DEVICE_DEPLOYMENT")?.t;
  const together = tApo != null && tDep != null && Math.abs(tApo - tDep) < 2;
  for (const ev of keyEvents(sim)) {
    if (!["BURNOUT", "APOGEE", "RECOVERY_DEVICE_DEPLOYMENT"].includes(ev.type)) continue;
    if (ev.type === "RECOVERY_DEVICE_DEPLOYMENT" && together) continue;
    const name = ev.type === "APOGEE" && together ? "Apogee + deploy" : ev.label;
    const h = lerp(sim, sim.alt, ev.t);
    add({ position: at(lerp(sim, sim.e, ev.t), lerp(sim, sim.n, ev.t), h), point: { pixelSize: 7, color: Cesium.Color.WHITE },
      label: label(`${name} T+${ev.t.toFixed(1)}s · ${Math.round(h)} m`, 0, 12) });
  }

  // Wind arrow at the pad, pointing downwind (1 m/s = 25 m).
  const c = sim.conditions, wl = Math.max(30, c.wind_speed * 25), dto = ((c.wind_dir_deg + 180) * Math.PI) / 180;
  add({ polyline: { positions: [at(0, 0), at(Math.sin(dto) * wl, Math.cos(dto) * wl)], width: 12, clampToGround: true,
    material: new Cesium.PolylineArrowMaterialProperty(css("#ffb547").withAlpha(0.85)) } });

  // Monte Carlo scatter and sigma zones.
  const pts = sim.mc.map(m => [m.e, m.n] as [number, number]);
  if (pts.length > 2) {
    for (const k of [3, 2, 1] as const) {
      const ring = sigmaEllipse(pts, k).ring.map(([e, n]) => at(e, n));
      const col = css(SIGMA_COLOR[k]);
      add({ polygon: { hierarchy: new Cesium.PolygonHierarchy(ring), material: col.withAlpha(0.1), classificationType: Cesium.ClassificationType.TERRAIN }, }, out.ellipses);
      add({ polyline: { positions: ring, width: 2, clampToGround: true, material: col } }, out.ellipses);
    }
    for (const m of sim.mc) {
      add({ position: at(m.e, m.n), point: { pixelSize: 5, color: Cesium.Color.WHITE.withAlpha(0.85), outlineColor: Cesium.Color.BLACK, outlineWidth: 1 } }, out.mc);
    }
  }

  // Wind sweep: where it lands at 0..N m/s.
  add({ polyline: { positions: [at(0, 0), ...sim.sweep.map(w => at(w.e, w.n))], width: 2, clampToGround: true, material: Cesium.Color.CYAN.withAlpha(0.7) } }, out.sweep);
  for (const w of sim.sweep) {
    if (w.wind > 0) add({ position: at(w.e, w.n), point: { pixelSize: 6, color: Cesium.Color.CYAN },
      label: label(`${w.wind} m/s → ${Math.round(Math.hypot(w.e, w.n))} m`, 0, 10) }, out.sweep);
  }
  return out;
}

/** Position of the sim at time t, relative to the current pad. */
function rocketAt(t: number) {
  const s = useStore.getState(), sim = currentSim(s);
  if (!sim) return undefined;
  return Cesium.Cartesian3.fromDegrees(...enuToLonLat(s.pad, lerp(sim, sim.e, t), lerp(sim, sim.n, t)), Math.max(lerp(sim, sim.alt, t), 0));
}

function frameFlight(viewer: Cesium.Viewer, sim: Sim, pad: Pad, duration = 1.2) {
  viewer.camera.lookAtTransform(Cesium.Matrix4.IDENTITY);
  const N = sim.t.length - 1;
  const pts: [number, number, number][] = [[0, 0, 0], [sim.e[N], sim.n[N], 0]];
  for (let i = 0; i < sim.t.length; i += 10) pts.push([sim.e[i], sim.n[i], sim.alt[i]]);
  sim.mc.forEach(m => pts.push([m.e, m.n, 0]));
  const bs = Cesium.BoundingSphere.fromPoints(pts.map(([e, n, h]) => Cesium.Cartesian3.fromDegrees(...enuToLonLat(pad, e, n), h)));
  viewer.camera.flyToBoundingSphere(bs, { duration, offset: new Cesium.HeadingPitchRange(-0.35, -0.42, bs.radius * 2.6) });
}

export function Globe3D() {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const viewer = new Cesium.Viewer(host.current!, {
      baseLayer: new Cesium.ImageryLayer(new Cesium.UrlTemplateImageryProvider({
        url: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
        maximumLevel: 19, credit: "Imagery © Esri, Maxar, Earthstar Geographics",
      })),
      terrainProvider: new Cesium.EllipsoidTerrainProvider(),
      baseLayerPicker: false, geocoder: false, timeline: false, animation: false, homeButton: false,
      sceneModePicker: false, navigationHelpButton: false, fullscreenButton: false,
      infoBox: false, selectionIndicator: false, shouldAnimate: false,
      requestRenderMode: true, maximumRenderTimeChange: Infinity,
    });
    const scene = viewer.scene;
    scene.globe.depthTestAgainstTerrain = false;
    viewer.cesiumWidget.screenSpaceEventHandler.removeInputAction(Cesium.ScreenSpaceEventType.LEFT_DOUBLE_CLICK);

    // Rocket and plot-hover ghost follow the store through callbacks; we just request a render when time changes.
    viewer.entities.add({
      position: new Cesium.CallbackPositionProperty(() => rocketAt(useStore.getState().t), false),
      point: { pixelSize: 14, color: css("#ffb547"), outlineColor: Cesium.Color.BLACK, outlineWidth: 2, disableDepthTestDistance: Number.POSITIVE_INFINITY },
    });
    const ghost = viewer.entities.add({
      position: new Cesium.CallbackPositionProperty(() => rocketAt(useStore.getState().hoverT ?? 0), false),
      point: { pixelSize: 10, color: Cesium.Color.WHITE.withAlpha(0.6), outlineColor: Cesium.Color.BLACK, outlineWidth: 1, disableDepthTestDistance: Number.POSITIVE_INFINITY },
      show: false,
    });

    let built: Built | null = null;
    const rebuild = () => {
      const s = useStore.getState(), sim = currentSim(s);
      built?.all.forEach(e => viewer.entities.remove(e));
      built = sim ? build(viewer, sim, s.pad) : null;
      applyLayers();
    };
    const applyLayers = () => {
      const { layers } = useStore.getState();
      built?.mc.forEach(e => (e.show = layers.mc));
      built?.ellipses.forEach(e => (e.show = layers.ellipses));
      built?.sweep.forEach(e => (e.show = layers.sweep));
      if (!layers.follow) viewer.camera.lookAtTransform(Cesium.Matrix4.IDENTITY);
      onTime();
    };
    const onTime = () => {
      const s = useStore.getState();
      ghost.show = s.hoverT != null;
      if (s.layers.follow) {
        const p = rocketAt(s.t), sim = currentSim(s);
        if (p && sim) viewer.camera.lookAt(p, new Cesium.HeadingPitchRange(0.6, -0.35, Math.max(250, lerp(sim, sim.alt, s.t) * 1.4)));
      }
      scene.requestRender();
    };
    const frame = (duration?: number) => {
      const s = useStore.getState(), sim = currentSim(s);
      if (sim) frameFlight(viewer, sim, s.pad, duration);
    };

    rebuild();
    viewer.camera.setView({ destination: Cesium.Cartesian3.fromDegrees(useStore.getState().pad.lon, useStore.getState().pad.lat, 9e6) });
    const firstFrame = setTimeout(() => frame(), 300);

    const unsubs = [
      useStore.subscribe(s => [s.data, s.simIdx, s.pad] as const, rebuild, { equalityFn: (a, b) => a.every((x, i) => x === b[i]) }),
      useStore.subscribe(s => s.layers, applyLayers),
      useStore.subscribe(s => [s.t, s.hoverT] as const, onTime, { equalityFn: (a, b) => a[0] === b[0] && a[1] === b[1] }),
      useStore.subscribe(s => s.frameTick, () => { const s = useStore.getState(); if (s.layers.follow) s.setLayer("follow", false); frame(); }),
    ];

    const dbl = new Cesium.ScreenSpaceEventHandler(scene.canvas);
    dbl.setInputAction((ev: Cesium.ScreenSpaceEventHandler.PositionedEvent) => {
      const cart = viewer.camera.pickEllipsoid(ev.position);
      if (!cart) return;
      const c = Cesium.Cartographic.fromCartesian(cart);
      useStore.getState().setPad({ lat: Cesium.Math.toDegrees(c.latitude), lon: Cesium.Math.toDegrees(c.longitude) });
    }, Cesium.ScreenSpaceEventType.LEFT_DOUBLE_CLICK);

    // Docking resizes the panel without a window resize; tell Cesium.
    const ro = new ResizeObserver(() => { viewer.resize(); scene.requestRender(); });
    ro.observe(host.current!);

    return () => {
      clearTimeout(firstFrame);
      ro.disconnect();
      unsubs.forEach(u => u());
      dbl.destroy();
      viewer.destroy();
    };
  }, []);

  return <div ref={host} style={{ position: "absolute", inset: 0 }} />;
}
