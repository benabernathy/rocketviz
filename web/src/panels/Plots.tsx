// Time-series plots (uPlot), one unit per plot so every chart has a single y-axis.
// All plots share a cursor; hovering previews that time on both maps, and click/drag scrubs playback.
// Each series tag doubles as the readout: it shows the value under the pointer, or at the playhead.
import { useEffect, useMemo, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import {
  Button, Menu, MenuItem, MenuList, MenuPopover, MenuTrigger, Tag, TagGroup, Text, Tooltip, makeStyles, tokens,
} from "@fluentui/react-components";
import { AddRegular, DeleteRegular, ChartMultipleRegular } from "@fluentui/react-icons";
import { MAX_SERIES_PER_PLOT, useSim, useStore, type PlotRow } from "../store";
import { idxAt, keyEvents } from "../flight";
import { SERIES_COLORS, fluentTheme, useIsDark } from "../theme";
import type { Series, Sim } from "../types";

const useStyles = makeStyles({
  root: { position: "absolute", inset: 0, display: "flex", flexDirection: "column", backgroundColor: tokens.colorNeutralBackground1 },
  bar: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS,
    padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS}`,
    borderBottom: `${tokens.strokeWidthThin} solid ${tokens.colorNeutralStroke2}`,
  },
  hint: { color: tokens.colorNeutralForeground3, marginLeft: "auto", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" },
  rows: { flex: 1, minHeight: 0, overflowY: "auto", display: "flex", flexDirection: "column" },
  row: {
    flex: "1 0 160px", display: "flex", flexDirection: "column", minHeight: "160px",
    borderBottom: `${tokens.strokeWidthThin} solid ${tokens.colorNeutralStroke3}`,
  },
  rowHead: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalXS, padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS} 0` },
  swatch: { display: "inline-block", width: "14px", height: "2px", borderRadius: "1px", marginRight: "6px", verticalAlign: "middle" },
  value: {
    marginLeft: "8px", fontFamily: tokens.fontFamilyMonospace, fontVariantNumeric: "tabular-nums",
    fontWeight: tokens.fontWeightSemibold, color: tokens.colorNeutralForeground1,
  },
  unit: { marginLeft: "3px", color: tokens.colorNeutralForeground3 },
  time: { fontFamily: tokens.fontFamilyMonospace, fontVariantNumeric: "tabular-nums", color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
  chart: { flex: 1, minHeight: 0, position: "relative", cursor: "crosshair", overflow: "hidden" },
  empty: { display: "grid", placeItems: "center", flex: 1, color: tokens.colorNeutralForeground3, gap: tokens.spacingVerticalS },
});

const SYNC_KEY = "orviz";
const fmt = (v: number | null | undefined) =>
  v == null || Number.isNaN(v) ? "—"
    : Math.abs(v) >= 1e5 || (Math.abs(v) < 1e-3 && v !== 0) ? v.toExponential(3)
      : Number(v.toPrecision(4)).toString();

function groupSeries(sim: Sim) {
  const groups = new Map<string, Series[]>();
  for (const s of sim.series) {
    if (!groups.has(s.group)) groups.set(s.group, []);
    groups.get(s.group)!.push(s);
  }
  return [...groups];
}

/** Grouped series picker. `filter` limits choices (e.g. to the plot's unit). */
function SeriesMenu({ sim, onPick, filter, trigger }: {
  sim: Sim; onPick: (key: string) => void; filter?: (s: Series) => boolean; trigger: React.ReactElement;
}) {
  const groups = groupSeries(sim)
    .map(([g, list]) => [g, filter ? list.filter(filter) : list] as const)
    .filter(([, list]) => list.length);
  return (
    <Menu>
      <MenuTrigger disableButtonEnhancement>{trigger}</MenuTrigger>
      <MenuPopover>
        <MenuList>
          {groups.length === 0 && <MenuItem disabled>No compatible series</MenuItem>}
          {groups.map(([g, list]) => (
            <Menu key={g}>
              <MenuTrigger disableButtonEnhancement><MenuItem>{g}</MenuItem></MenuTrigger>
              <MenuPopover>
                <MenuList>
                  {list.map(s => (
                    <MenuItem key={s.key} onClick={() => onPick(s.key)} secondaryContent={s.unit}>{s.name}</MenuItem>
                  ))}
                </MenuList>
              </MenuPopover>
            </Menu>
          ))}
        </MenuList>
      </MenuPopover>
    </Menu>
  );
}

/** Event marker labels, merging events too close together to label separately. */
function eventMarks(sim: Sim) {
  const marks: { t: number; label: string }[] = [];
  for (const e of keyEvents(sim)) {
    if (e.type === "LIFTOFF") continue; // T+0 is the axis origin
    const prev = marks[marks.length - 1];
    if (prev && e.t - prev.t < 0.5) prev.label += ` + ${e.label.toLowerCase()}`;
    else marks.push({ t: e.t, label: e.label });
  }
  return marks;
}

interface ChartProps {
  sim: Sim;
  series: { key: string; slot: number; s: Series }[];
  dark: boolean;
  /** Readout spans in the row header, keyed by series key, plus "time". */
  readout: React.RefObject<Map<string, HTMLElement>>;
}

function Chart({ sim, series, dark, readout }: ChartProps) {
  const host = useRef<HTMLDivElement>(null);
  const styles = useStyles();

  useEffect(() => {
    const el = host.current!;
    if (!series.length) return;
    const th = fluentTheme(dark);
    const colors = dark ? SERIES_COLORS.dark : SERIES_COLORS.light;
    const marks = eventMarks(sim);
    const axis: uPlot.Axis = {
      stroke: th.colorNeutralForeground3,
      grid: { stroke: th.colorNeutralStroke3, width: 1 },
      ticks: { stroke: th.colorNeutralStroke2, width: 1, size: 4 },
      font: `11px ${th.fontFamilyBase}`,
    };
    let scrubbing = false;

    const show = (idx: number | null | undefined, t: number) => {
      const r = readout.current;
      if (!r) return;
      const i = idx ?? Math.min(idxAt(sim, t), sim.t.length - 1);
      r.get("time")?.replaceChildren(`T+${(idx == null ? t : sim.t[i]).toFixed(1)} s`);
      for (const { key, s } of series) r.get(key)?.replaceChildren(fmt(s.data[i]));
    };

    const opts: uPlot.Options = {
      width: el.clientWidth,
      height: Math.max(60, el.clientHeight),
      pxAlign: false,
      scales: { x: { time: false } },
      legend: { show: false },
      cursor: {
        sync: { key: SYNC_KEY, setSeries: false },
        drag: { x: false, y: false },
        points: { size: 7, width: 2 },
      },
      axes: [
        { ...axis, size: 28, values: (_u, v) => v.map(x => `${x}s`) },
        { ...axis, size: 48, values: (_u, v) => v.map(fmt) },
      ],
      series: [
        {},
        ...series.map(({ s, slot }) => ({ label: s.name, stroke: colors[slot], width: 2, points: { show: false } })),
      ],
      hooks: {
        setCursor: [u => {
          const idx = u.cursor.idx, left = u.cursor.left ?? -1;
          if (!scrubbing) useStore.getState().setHoverT(idx == null || left < 0 ? null : u.posToVal(left, "x"));
          show(idx, useStore.getState().t);
        }],
        draw: [u => {
          const ctx = u.ctx, { top, height } = u.bbox, dpr = devicePixelRatio;
          ctx.save();
          ctx.font = `${10 * dpr}px ${th.fontFamilyBase}`;
          ctx.textBaseline = "top";
          ctx.textAlign = "left";
          let lastLabelEnd = -Infinity;
          for (const m of marks) {
            const x = Math.round(u.valToPos(m.t, "x", true));
            ctx.strokeStyle = th.colorNeutralStroke1;
            ctx.setLineDash([3 * dpr, 3 * dpr]);
            ctx.lineWidth = dpr;
            ctx.beginPath(); ctx.moveTo(x, top); ctx.lineTo(x, top + height); ctx.stroke();
            // Label only when it won't collide with the previous one; the line alone still marks the event.
            const w = ctx.measureText(m.label).width;
            if (x + 3 * dpr > lastLabelEnd + 6 * dpr && x + w < u.bbox.left + u.bbox.width) {
              ctx.fillStyle = th.colorNeutralForeground3;
              ctx.fillText(m.label, x + 3 * dpr, top + 2 * dpr);
              lastLabelEnd = x + 3 * dpr + w;
            }
          }
          const x = Math.round(u.valToPos(useStore.getState().t, "x", true));
          ctx.setLineDash([]);
          ctx.strokeStyle = "#ffb547";
          ctx.lineWidth = 2 * dpr;
          ctx.beginPath(); ctx.moveTo(x, top); ctx.lineTo(x, top + height); ctx.stroke();
          ctx.restore();
        }],
      },
    };

    const u = new uPlot(opts, [sim.t, ...series.map(({ s }) => s.data)], el);
    show(null, useStore.getState().t);

    // Click or drag on the plot scrubs playback.
    const seek = (e: PointerEvent) => {
      const r = u.over.getBoundingClientRect(), s = useStore.getState();
      s.setPlaying(false);
      s.setT(u.posToVal(Math.max(0, Math.min(e.clientX - r.left, r.width)), "x"));
    };
    const down = (e: PointerEvent) => { scrubbing = true; u.over.setPointerCapture(e.pointerId); seek(e); };
    const move = (e: PointerEvent) => { if (scrubbing) seek(e); };
    const up = (e: PointerEvent) => { scrubbing = false; u.over.releasePointerCapture(e.pointerId); };
    const leave = () => { if (!scrubbing) useStore.getState().setHoverT(null); };
    u.over.addEventListener("pointerdown", down);
    u.over.addEventListener("pointermove", move);
    u.over.addEventListener("pointerup", up);
    u.over.addEventListener("pointerleave", leave);

    const unsub = useStore.subscribe(s => s.t, t => {
      u.redraw(false, false);
      if (u.cursor.idx == null) show(null, t);
    });

    const ro = new ResizeObserver(() => u.setSize({ width: el.clientWidth, height: Math.max(60, el.clientHeight) }));
    ro.observe(el);

    return () => { ro.disconnect(); unsub(); u.destroy(); };
  }, [sim, series, dark, readout]);

  return <div ref={host} className={styles.chart} />;
}

function PlotRowView({ sim, row, dark }: { sim: Sim; row: PlotRow; dark: boolean }) {
  const styles = useStyles();
  const { addSeries, removeSeries, removePlot } = useStore.getState();
  const colors = dark ? SERIES_COLORS.dark : SERIES_COLORS.light;
  const readout = useRef(new Map<string, HTMLElement>());
  const series = useMemo(
    () => row.series.flatMap(r => { const s = sim.series.find(x => x.key === r.key); return s ? [{ ...r, s }] : []; }),
    [sim, row],
  );
  const missing = row.series.filter(r => !series.some(x => x.key === r.key));
  const unit = series[0]?.s.unit;
  const full = row.series.length >= MAX_SERIES_PER_PLOT;
  const bind = (key: string) => (el: HTMLElement | null) => { if (el) readout.current.set(key, el); else readout.current.delete(key); };

  return (
    <div className={styles.row}>
      <div className={styles.rowHead}>
        <TagGroup size="extra-small" onDismiss={(_, d) => removeSeries(row.id, d.value)} aria-label="Series in this plot">
          {series.map(({ key, slot, s }) => (
            <Tag key={key} value={key} dismissible dismissIcon={{ "aria-label": `Remove ${s.name}` }} appearance="outline">
              <span className={styles.swatch} style={{ background: colors[slot] }} />
              {s.name}
              <span ref={bind(key)} className={styles.value} />
              {s.unit && <span className={styles.unit}>{s.unit}</span>}
            </Tag>
          ))}
          {missing.map(r => (
            <Tag key={r.key} value={r.key} dismissible appearance="outline">{r.key} (not in this flight)</Tag>
          ))}
        </TagGroup>
        <SeriesMenu
          sim={sim}
          onPick={k => addSeries(row.id, k)}
          filter={s => s.unit === unit && !row.series.some(r => r.key === s.key)}
          trigger={
            <Tooltip content={full ? `Up to ${MAX_SERIES_PER_PLOT} series per plot` : `Add a series in ${unit || "the same unit"}`} relationship="label">
              <Button size="small" appearance="subtle" icon={<AddRegular />} disabled={full} />
            </Tooltip>
          }
        />
        <div style={{ flex: 1 }} />
        <span ref={bind("time")} className={styles.time} />
        <Tooltip content="Remove plot" relationship="label">
          <Button size="small" appearance="subtle" icon={<DeleteRegular />} onClick={() => removePlot(row.id)} />
        </Tooltip>
      </div>
      {series.length > 0 && <Chart sim={sim} series={series} dark={dark} readout={readout} />}
    </div>
  );
}

export function Plots() {
  const styles = useStyles();
  const sim = useSim();
  const plots = useStore(s => s.plots);
  const addPlot = useStore(s => s.addPlot);
  const dark = useIsDark();
  if (!sim) return null;

  return (
    <div className={styles.root}>
      <div className={styles.bar}>
        <SeriesMenu sim={sim} onPick={addPlot} trigger={<Button size="small" icon={<AddRegular />}>Add plot</Button>} />
        <Text size={200} className={styles.hint}>Click or drag a plot to scrub · hover to preview on the maps · maximize this panel for more room</Text>
      </div>
      <div className={styles.rows}>
        {plots.length === 0 && (
          <div className={styles.empty}>
            <ChartMultipleRegular fontSize={32} />
            <Text>No plots. Use “Add plot” to chart any recorded flight variable.</Text>
          </div>
        )}
        {plots.map(p => <PlotRowView key={p.id} sim={sim} row={p} dark={dark} />)}
      </div>
    </div>
  );
}
