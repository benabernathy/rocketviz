// Simulate: choose configurations, change launch conditions, set up the Monte Carlo analysis, and run it.
// The condition fields come from the engine's parameter list, so any engine gets a working form.
import { useEffect, useMemo, useState } from "react";
import {
  Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Input,
  Select, Switch, Text, Tooltip, makeStyles, tokens,
} from "@fluentui/react-components";
import { AddRegular, DeleteRegular, PlayRegular } from "@fluentui/react-icons";
import { useStore } from "../store";
import { desktop, type AnalysisSettings, type ConfigInfo, type DesignInfo, type Dispersion, type ParamDispersion, type ParameterDef } from "../desktop";
import { configLabel, simulate } from "../analysis";

const useStyles = makeStyles({
  surface: { width: "min(760px, calc(100vw - 32px))", maxWidth: "none" },
  content: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalL, maxHeight: "min(70vh, 720px)", overflowY: "auto", paddingRight: tokens.spacingHorizontalS },
  section: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  heading: { color: tokens.colorNeutralForeground2 },
  hint: { color: tokens.colorNeutralForeground3 },
  configs: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXXS },
  configMeta: { color: tokens.colorNeutralForeground3, marginLeft: "32px", marginTop: "-6px" },
  groups: { display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))", gap: tokens.spacingHorizontalL },
  group: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  mc: { display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))", gap: tokens.spacingHorizontalM },
  // Fluent inputs size to their content by default; in a grid cell they should shrink to the column.
  input: { minWidth: 0, width: "100%" },
  dispRow: {
    display: "grid", gridTemplateColumns: "minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 1.4fr) auto", gap: tokens.spacingHorizontalS, alignItems: "center",
    "& > *": { minWidth: 0 },
  },
  pair: { display: "grid", gridTemplateColumns: "1fr 1fr", gap: tokens.spacingHorizontalXS },
  error: { color: tokens.colorPaletteRedForeground1 },
});

/** How a parameter is shown: fractions as percent, degrees as °. */
function display(p: ParameterDef | undefined) {
  if (p?.unit === "fraction") return { unit: "%", scale: 100 };
  return { unit: p?.unit === "deg" ? "°" : p?.unit ?? "", scale: 1 };
}
const fmt = (v: number) => String(Number(v.toPrecision(7)));

/** Grouping of the standard parameters; anything else lands in "Other". */
const GROUPS: [string, string[]][] = [
  ["Wind", ["wind_speed", "wind_direction", "turbulence_intensity"]],
  ["Launch rod", ["rod_length", "rod_angle", "rod_direction"]],
  ["Launch site", ["launch_latitude", "launch_longitude", "launch_altitude"]],
];
/** Set in the Monte Carlo section, not per condition. */
const RUN_KEYS = new Set(["seed", "time_step"]);

interface McForm { runs: string; seed: string; sweep: string; step: string }
interface DispRow { key: string; kind: Dispersion["kind"]; a: string; b: string }

const MC_KEY = "rocketviz.simulate.v1";
const MC_DEFAULT: McForm = { runs: "150", seed: "42", sweep: "8", step: "0.05" };
function loadMc(): McForm {
  try { return { ...MC_DEFAULT, ...JSON.parse(localStorage.getItem(MC_KEY) ?? "{}") }; } catch { return MC_DEFAULT; }
}

// What was entered last for each design, so re-running after a tweak doesn't mean re-typing everything.
const remembered = new Map<string, { selected: string[]; overrides: Record<string, string>; custom: DispRow[] | null }>();

/** Parses a field: blank -> undefined, else a number within the parameter's range (in display units). */
function parse(text: string, p?: ParameterDef, scale = 1): { value?: number; error?: string } {
  if (!text.trim()) return {};
  const v = Number(text.trim());
  if (!Number.isFinite(v)) return { error: "Enter a number" };
  if (p?.min != null && v < p.min * scale) return { error: `At least ${fmt(p.min * scale)}` };
  if (p?.max != null && v > p.max * scale) return { error: `At most ${fmt(p.max * scale)}` };
  return { value: v / scale };
}

function designValue(configs: ConfigInfo[], key: string, scale: number) {
  const vals = configs.map(c => c.defaults[key]).filter((v): v is number => typeof v === "number");
  if (!vals.length) return "";
  return vals.every(v => v === vals[0]) ? fmt(vals[0] * scale) : "varies by configuration";
}

export function SimulateDialog() {
  const design = useStore(s => s.design);
  const open = useStore(s => s.dialog === "simulate");
  const close = () => useStore.setState({ dialog: null });
  const styles = useStyles();
  // The surface (and so the form) mounts each time the dialog opens: fresh fields, from `remembered`.
  return (
    <Dialog open={open && !!design} onOpenChange={(_, d) => { if (!d.open) close(); }}>
      <DialogSurface className={styles.surface}>
        {design && <SimulateForm key={design.path} design={design} onClose={close} />}
      </DialogSurface>
    </Dialog>
  );
}

function SimulateForm({ design, onClose }: { design: DesignInfo; onClose: () => void }) {
  const styles = useStyles();
  const memory = remembered.get(design.path);
  const [selected, setSelected] = useState<string[]>(memory?.selected ?? design.configs.map(c => c.id));
  const [overrides, setOverrides] = useState<Record<string, string>>(memory?.overrides ?? {});
  const [mc, setMc] = useState<McForm>(loadMc);
  const [custom, setCustom] = useState<DispRow[] | null>(memory?.custom ?? null);
  const [error, setError] = useState<string | null>(null);

  const params = design.parameters;
  const param = (key: string) => params.find(p => p.key === key);
  const chosen = design.configs.filter(c => selected.includes(c.id));
  const conditionKeys = params.filter(p => !RUN_KEYS.has(p.key)).map(p => p.key);
  const grouped = useMemo(() => {
    const known = new Set(GROUPS.flatMap(([, keys]) => keys));
    const groups = GROUPS.map(([name, keys]) => [name, keys.filter(k => conditionKeys.includes(k))] as [string, string[]]);
    groups.push(["Other", conditionKeys.filter(k => !known.has(k))]);
    return groups.filter(([, keys]) => keys.length);
  }, [conditionKeys.join()]);
  const dispersible = params.filter(p => !RUN_KEYS.has(p.key));

  useEffect(() => {
    remembered.set(design.path, { selected, overrides, custom });
  }, [design.path, selected, overrides, custom]);

  // Validation.
  const fieldErrors: Record<string, string | undefined> = {};
  const parsedOverrides: Record<string, number> = {};
  for (const key of conditionKeys) {
    const p = param(key), { scale } = display(p), r = parse(overrides[key] ?? "", p, scale);
    if (r.error) fieldErrors[key] = r.error;
    else if (r.value !== undefined) parsedOverrides[key] = r.value;
  }
  const runs = parse(mc.runs, { key: "", name: "", unit: "", kind: "standard", min: 0, max: 5000 });
  const seed = parse(mc.seed);
  const sweep = parse(mc.sweep, { key: "", name: "", unit: "", kind: "standard", min: 0, max: 40 });
  const step = parse(mc.step, param("time_step"));
  const mcErrors = {
    runs: runs.error ?? (mc.runs.trim() && !Number.isInteger(runs.value) ? "Whole number" : undefined),
    seed: seed.error ?? (mc.seed.trim() && (!Number.isInteger(seed.value) || seed.value! < 0) ? "Whole number, 0 or more" : undefined),
    sweep: sweep.error ?? (mc.sweep.trim() && !Number.isInteger(sweep.value) ? "Whole number" : undefined),
    step: step.error,
  };
  const dispersions = custom?.map(row => {
    const p = param(row.key), { scale } = display(p);
    const a = parse(row.a), b = parse(row.b);
    const bad = row.kind === "uniform"
      ? (a.value === undefined || b.value === undefined || b.value <= a.value ? "Enter min < max" : undefined)
      : (a.value === undefined || a.value < 0 ? "Enter σ ≥ 0" : undefined);
    const d: ParamDispersion = row.kind === "uniform"
      ? { key: row.key, kind: "uniform", min: (a.value ?? 0) / scale, max: (b.value ?? 0) / scale }
      : { key: row.key, kind: row.kind, sigma: (a.value ?? 0) / scale };
    return { d, bad };
  });
  const invalid = !chosen.length || Object.values(fieldErrors).some(Boolean) || Object.values(mcErrors).some(Boolean)
    || !!dispersions?.some(x => x.bad);

  const setOverride = (key: string, v: string) => setOverrides(o => ({ ...o, [key]: v }));
  const windForDefaults = () => parsedOverrides.wind_speed
    ?? (chosen[0]?.defaults.wind_speed as number | undefined) ?? 0;

  const toggleCustom = async (on: boolean) => {
    if (!on) { setCustom(null); return; }
    const defaults = await desktop.defaultDispersions(windForDefaults());
    setCustom(defaults.map(d => {
      const { scale } = display(param(d.key));
      return d.kind === "uniform"
        ? { key: d.key, kind: d.kind, a: fmt(d.min * scale), b: fmt(d.max * scale) }
        : { key: d.key, kind: d.kind, a: fmt(d.sigma * scale), b: "" };
    }));
  };
  const updateRow = (i: number, patch: Partial<DispRow>) => setCustom(rows => rows!.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  const unusedParams = dispersible.filter(p => !custom?.some(r => r.key === p.key));

  const run = () => {
    if (invalid) return;
    try { localStorage.setItem(MC_KEY, JSON.stringify(mc)); } catch { /* storage unavailable */ }
    const settings: AnalysisSettings = {
      overrides: parsedOverrides,
      seed: seed.value ?? 42,
      mc_runs: runs.value ?? 150,
      dispersions: dispersions ? dispersions.map(x => x.d) : null,
      mc_time_step: step.value ?? 0.05,
      sweep_max: sweep.value ?? 8,
    };
    onClose();
    simulate(design, chosen.map(c => c.id), settings).catch(e =>
      useStore.setState({ notice: { intent: "error", text: `Simulation failed: ${e instanceof Error ? e.message : String(e)}` } }));
  };

  const condition = (key: string) => {
    const p = param(key)!, { unit, scale } = display(p);
    const fromDesign = designValue(chosen, key, scale);
    const intoWind = key === "rod_direction" && chosen.some(c => c.defaults.launch_into_wind === true);
    return (
      <Field
        key={key}
        label={p.name}
        validationMessage={fieldErrors[key] ?? (intoWind && !overrides[key]?.trim() ? "Blank: into the wind, as the design says" : undefined)}
        validationState={fieldErrors[key] ? "error" : "none"}
        hint={p.description ?? undefined}
      >
        <Input
          className={styles.input}
          inputMode="decimal"
          value={overrides[key] ?? ""}
          placeholder={fromDesign || "from design"}
          contentAfter={unit ? <Text size={200} className={styles.hint}>{unit}</Text> : undefined}
          onChange={(_, d) => setOverride(key, d.value)}
        />
      </Field>
    );
  };

  return (
    <DialogBody>
      <DialogTitle>Simulate {design.name}</DialogTitle>
      <DialogContent className={styles.content}>
        <Text size={200} className={styles.hint}>
          {design.path} · {design.engine.name} {design.engine.version}
        </Text>

        <section className={styles.section}>
          <Text weight="semibold" className={styles.heading}>Configurations</Text>
          <div className={styles.configs}>
            {design.configs.map(c => {
              const d = c.defaults, num = (k: string) => (typeof d[k] === "number" ? (d[k] as number) : NaN);
              return (
                <div key={c.id}>
                  <Checkbox
                    label={configLabel(c)}
                    checked={selected.includes(c.id)}
                    onChange={(_, data) => setSelected(s => (data.checked ? [...s, c.id] : s.filter(x => x !== c.id)))}
                  />
                  <Text size={200} block className={styles.configMeta}>
                    Site {num("launch_latitude").toFixed(5)}, {num("launch_longitude").toFixed(5)} · wind {fmt(num("wind_speed"))} m/s
                    from {fmt(num("wind_direction"))}°
                  </Text>
                </div>
              );
            })}
          </div>
          {!chosen.length && <Text size={200} className={styles.error}>Choose at least one configuration.</Text>}
        </section>

        <section className={styles.section}>
          <Text weight="semibold" className={styles.heading}>Launch conditions</Text>
          <Text size={200} className={styles.hint}>Leave a field blank to use each configuration's own value from the design.</Text>
          <div className={styles.groups}>
            {grouped.map(([name, keys]) => (
              <div key={name} className={styles.group}>
                <Text size={200} weight="semibold">{name}</Text>
                {keys.map(condition)}
              </div>
            ))}
          </div>
        </section>

        <section className={styles.section}>
          <Text weight="semibold" className={styles.heading}>Monte Carlo landing analysis</Text>
          <div className={styles.mc}>
            <Field label="Runs" validationMessage={mcErrors.runs} hint="0 skips it">
              <Input className={styles.input} inputMode="numeric" value={mc.runs} onChange={(_, d) => setMc(m => ({ ...m, runs: d.value }))} />
            </Field>
            <Field label="Random seed" validationMessage={mcErrors.seed} hint="Same seed, same result">
              <Input className={styles.input} inputMode="numeric" value={mc.seed} onChange={(_, d) => setMc(m => ({ ...m, seed: d.value }))} />
            </Field>
            <Field label="Wind sweep up to" validationMessage={mcErrors.sweep} hint="0 skips it">
              <Input className={styles.input} inputMode="numeric" value={mc.sweep} contentAfter={<Text size={200} className={styles.hint}>m/s</Text>}
                onChange={(_, d) => setMc(m => ({ ...m, sweep: d.value }))} />
            </Field>
            <Field label="Time step" validationMessage={mcErrors.step} hint="For Monte Carlo runs">
              <Input className={styles.input} inputMode="decimal" value={mc.step} contentAfter={<Text size={200} className={styles.hint}>s</Text>}
                onChange={(_, d) => setMc(m => ({ ...m, step: d.value }))} />
            </Field>
          </div>

          <Switch label="Customize dispersions" checked={custom != null} onChange={(_, d) => { toggleCustom(d.checked).catch(e => setError(String(e))); }} />
          {custom == null ? (
            <Text size={200} className={styles.hint}>
              Standard dispersions: wind speed σ = 30 % of the wind + 0.3 m/s, wind direction σ = 20°, rod tilt error
              up to about 1.5° (half-normal) in any direction, and fresh turbulence every run.
            </Text>
          ) : (
            <div className={styles.section}>
              {custom.map((row, i) => {
                const { unit } = display(param(row.key)), bad = dispersions?.[i]?.bad;
                const after = unit ? <Text size={200} className={styles.hint}>{unit}</Text> : undefined;
                return (
                  <div key={i} className={styles.dispRow}>
                    <Select value={row.key} onChange={(_, d) => updateRow(i, { key: d.value })} aria-label="Parameter">
                      {dispersible.filter(p => p.key === row.key || !custom.some(r => r.key === p.key)).map(p => (
                        <option key={p.key} value={p.key}>{p.name}</option>
                      ))}
                    </Select>
                    <Select value={row.kind} onChange={(_, d) => updateRow(i, { kind: d.value as Dispersion["kind"] })} aria-label="Distribution">
                      <option value="normal">Normal ±σ</option>
                      <option value="half_normal">Half-normal +σ</option>
                      <option value="uniform">Uniform range</option>
                    </Select>
                    <Field validationMessage={bad} validationState={bad ? "error" : "none"}>
                      {row.kind === "uniform" ? (
                        <div className={styles.pair}>
                          <Input className={styles.input} aria-label="Minimum" placeholder="min" value={row.a} contentAfter={after} onChange={(_, d) => updateRow(i, { a: d.value })} />
                          <Input className={styles.input} aria-label="Maximum" placeholder="max" value={row.b} contentAfter={after} onChange={(_, d) => updateRow(i, { b: d.value })} />
                        </div>
                      ) : (
                        <Input className={styles.input} aria-label="Sigma" contentBefore={<Text size={200}>σ</Text>} value={row.a} contentAfter={after}
                          onChange={(_, d) => updateRow(i, { a: d.value })} />
                      )}
                    </Field>
                    <Tooltip content="Remove" relationship="label">
                      <Button appearance="subtle" icon={<DeleteRegular />} onClick={() => setCustom(rows => rows!.filter((_, j) => j !== i))} />
                    </Tooltip>
                  </div>
                );
              })}
              <div>
                <Button size="small" icon={<AddRegular />} disabled={!unusedParams.length}
                  onClick={() => setCustom(rows => [...rows!, { key: unusedParams[0].key, kind: "normal", a: "", b: "" }])}>
                  Add dispersion
                </Button>
              </div>
            </div>
          )}
        </section>
        {error && <Text className={styles.error}>{error}</Text>}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancel</Button>
        <Button appearance="primary" icon={<PlayRegular />} disabled={invalid} onClick={run}>Run simulation</Button>
      </DialogActions>
    </DialogBody>
  );
}
