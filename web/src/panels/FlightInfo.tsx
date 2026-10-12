// Flight panel: live readout at the playhead, recovery numbers, and map legend.
import { Divider, Text, makeStyles, tokens } from "@fluentui/react-components";
import { useSim, useStore } from "../store";
import { bearing, compass, feet, phaseAt, sigmaEllipse, stateAt, PHASE_COLOR, SIGMA_COLOR } from "../flight";

const useStyles = makeStyles({
  root: {
    position: "absolute", inset: 0, overflowY: "auto", padding: tokens.spacingHorizontalM,
    display: "flex", flexDirection: "column", gap: tokens.spacingVerticalM,
    backgroundColor: tokens.colorNeutralBackground1,
  },
  divider: { flexGrow: 0 },
  heading: { color: tokens.colorNeutralForeground3, textTransform: "uppercase", letterSpacing: "0.06em" },
  readout: { display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: tokens.spacingHorizontalS },
  big: { fontFamily: tokens.fontFamilyMonospace, fontVariantNumeric: "tabular-nums", fontSize: tokens.fontSizeBase500, fontWeight: tokens.fontWeightSemibold, lineHeight: 1.2 },
  cap: { color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase100 },
  phase: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  dot: { width: "10px", height: "10px", borderRadius: tokens.borderRadiusCircular },
  kv: { display: "grid", gridTemplateColumns: "auto 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}`, margin: 0 },
  dt: { color: tokens.colorNeutralForeground2, fontSize: tokens.fontSizeBase200 },
  dd: { margin: 0, textAlign: "right", fontFamily: tokens.fontFamilyMonospace, fontVariantNumeric: "tabular-nums", fontSize: tokens.fontSizeBase200 },
  accent: { color: tokens.colorBrandForeground1, fontWeight: tokens.fontWeightSemibold, fontSize: tokens.fontSizeBase300 },
  legend: { display: "grid", gap: tokens.spacingVerticalXXS, fontSize: tokens.fontSizeBase200, color: tokens.colorNeutralForeground2 },
  key: { display: "inline-block", width: "16px", height: "3px", borderRadius: "2px", marginRight: "8px", verticalAlign: "middle" },
  ring: { display: "inline-block", width: "12px", height: "12px", borderRadius: "50%", border: "2px solid", marginRight: "8px", verticalAlign: "middle", boxSizing: "border-box" },
});

export function FlightInfo() {
  const styles = useStyles();
  const sim = useSim();
  const t = useStore(s => s.t);
  const pad = useStore(s => s.pad);
  if (!sim) return null;

  const now = stateAt(sim, t), phase = phaseAt(sim, t);
  const N = sim.t.length - 1, c = sim.conditions, m = sim.summary;
  const dist = Math.hypot(sim.e[N], sim.n[N]), brg = bearing(sim.e[N], sim.n[N]);
  const d = sim.mc.map(x => Math.hypot(x.e, x.n)).sort((a, b) => a - b);
  const pct = (p: number) => (d.length ? `${Math.round(d[Math.min(d.length - 1, Math.floor(p * d.length))])} m` : "—");
  const ell = sim.mc.length > 2 ? sigmaEllipse(sim.mc.map(x => [x.e, x.n]), 2).axes : null;

  const rows: [string, string, boolean?][] = [
    ["Walk to rocket", `${Math.round(dist)} m · ${feet(dist)} ft`, true],
    ["Direction", `${Math.round(brg)}° ${compass(brg)}`],
    ["Apogee", `${Math.round(m.max_alt)} m · ${feet(m.max_alt)} ft`],
    ["Max speed", `${m.max_v.toFixed(1)} m/s`],
    ["Flight time", `${m.flight_t.toFixed(1)} s`],
    ["Landing speed", `${m.ground_v.toFixed(1)} m/s`],
    ["Off-rod speed", `${m.rod_v.toFixed(1)} m/s`],
  ];
  const mcRows: [string, string][] = [
    ["Runs", String(d.length)],
    ["Median drift", pct(0.5)],
    ["90th pct drift", pct(0.9)],
    ["2σ zone", ell ? `${Math.round(ell[0])} × ${Math.round(ell[1])} m` : "—"],
  ];
  const siteRows: [string, string][] = [
    ["Pad", `${pad.lat.toFixed(5)}, ${pad.lon.toFixed(5)}`],
    ["Wind", `${c.wind_speed.toFixed(1)} m/s from ${Math.round(c.wind_dir_deg)}° ${compass(c.wind_dir_deg)}`],
    ["Turbulence", `${(c.turbulence * 100).toFixed(0)} %`],
    ["Rod", `${c.rod_length.toFixed(2)} m at ${c.rod_angle_deg.toFixed(1)}°`],
  ];
  const kv = (list: [string, string, boolean?][]) => (
    <dl className={styles.kv}>
      {list.map(([k, v, accent]) => [
        <dt key={`${k}-k`} className={styles.dt}>{k}</dt>,
        <dd key={`${k}-v`} className={`${styles.dd} ${accent ? styles.accent : ""}`}>{v}</dd>,
      ])}
    </dl>
  );

  return (
    <div className={styles.root}>
      <div className={styles.phase}>
        <span className={styles.dot} style={{ background: phase.color }} />
        <Text weight="semibold">{phase.label}</Text>
        <Text className={styles.cap} style={{ marginLeft: "auto" }}>T+{t.toFixed(1)} s</Text>
      </div>
      <div className={styles.readout}>
        <div><div className={styles.big}>{Math.round(now.alt)}</div><div className={styles.cap}>ALT m AGL</div></div>
        <div><div className={styles.big}>{now.v.toFixed(1)}</div><div className={styles.cap}>SPEED m/s</div></div>
        <div><div className={styles.big}>{Math.round(Math.hypot(now.e, now.n))}</div><div className={styles.cap}>DRIFT m</div></div>
      </div>
      <Divider className={styles.divider} />
      <Text size={200} weight="semibold" className={styles.heading}>Recovery</Text>
      {kv(rows)}
      <Text size={200} weight="semibold" className={styles.heading}>Monte Carlo</Text>
      {kv(mcRows)}
      <Text size={200} weight="semibold" className={styles.heading}>Launch</Text>
      {kv(siteRows)}
      <Divider className={styles.divider} />
      <div className={styles.legend}>
        {(["boost", "coast", "descent"] as const).map(k => (
          <span key={k}><i className={styles.key} style={{ background: PHASE_COLOR[k] }} />{k[0].toUpperCase() + k.slice(1)}</span>
        ))}
        {([1, 2, 3] as const).map(k => (
          <span key={k}><i className={styles.ring} style={{ borderColor: SIGMA_COLOR[k] }} />{k}σ landing zone</span>
        ))}
      </div>
    </div>
  );
}
