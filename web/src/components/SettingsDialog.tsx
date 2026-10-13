// Settings: map imagery (provider and API keys) and, in the desktop app, which simulation engines and Java it found.
import { useEffect, useState } from "react";
import {
  Badge, Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Input,
  Radio, RadioGroup, Spinner, Text, makeStyles, tokens,
} from "@fluentui/react-components";
import { CheckmarkCircleRegular, ErrorCircleRegular } from "@fluentui/react-icons";
import { isTauri } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { desktop, type EngineReport, type ImageryProviderInfo } from "../desktop";
import { BROWSER_PROVIDERS } from "../imagery";

const useStyles = makeStyles({
  surface: { width: "min(640px, calc(100vw - 32px))", maxWidth: "none" },
  content: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalL, maxHeight: "70vh", overflowY: "auto" },
  section: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  heading: { color: tokens.colorNeutralForeground2 },
  hint: { color: tokens.colorNeutralForeground3 },
  keyRow: { display: "flex", gap: tokens.spacingHorizontalS, alignItems: "end" },
  keyInput: { flex: 1 },
  ok: { color: tokens.colorPaletteGreenForeground1, display: "flex", gap: tokens.spacingHorizontalXS, alignItems: "center" },
  bad: { color: tokens.colorPaletteRedForeground1, display: "flex", gap: tokens.spacingHorizontalXS, alignItems: "flex-start" },
  engine: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXXS },
  mono: { fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, wordBreak: "break-all", color: tokens.colorNeutralForeground3 },
});

export function SettingsDialog() {
  const styles = useStyles();
  const open = useStore(s => s.dialog === "settings");
  // The surface (and so the form) mounts each time the dialog opens, so it re-reads providers and engines.
  return (
    <Dialog open={open} onOpenChange={(_, d) => { if (!d.open) useStore.setState({ dialog: null }); }}>
      <DialogSurface className={styles.surface}>
        <SettingsForm />
      </DialogSurface>
    </Dialog>
  );
}

function SettingsForm() {
  const styles = useStyles();
  const imagery = useStore(s => s.imagery);
  const [providers, setProviders] = useState<ImageryProviderInfo[] | null>(isTauri() ? null : BROWSER_PROVIDERS);
  const [engines, setEngines] = useState<EngineReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => desktop.imageryProviders().then(setProviders).catch(e => setError(String(e)));
  useEffect(() => {
    if (!isTauri()) return;
    refresh();
    desktop.engineReport().then(setEngines).catch(e => setError(String(e)));
  }, []);

  return (
    <DialogBody>
      <DialogTitle>Settings</DialogTitle>
      <DialogContent className={styles.content}>
        <section className={styles.section}>
          <Text weight="semibold" className={styles.heading}>Map imagery</Text>
          {!providers ? <Spinner size="tiny" /> : (
            <RadioGroup value={imagery} onChange={(_, d) => useStore.setState({ imagery: d.value })}>
              {providers.map(p => (
                <Radio key={p.id} value={p.id} label={
                  <>{p.name}{p.coverage ? <Text size={200} className={styles.hint}> · {p.coverage} only</Text> : null}</>
                } />
              ))}
            </RadioGroup>
          )}
          {providers?.filter(p => p.keyed && isTauri()).map(p => <KeyField key={p.id} provider={p} onSaved={refresh} />)}
        </section>

        {isTauri() && (
          <section className={styles.section}>
            <Text weight="semibold" className={styles.heading}>Simulation engines</Text>
            {!engines ? <Spinner size="tiny" /> : <>
              {engines.engines.length === 0 && (
                <Text className={styles.bad}><ErrorCircleRegular />No engines found, so designs can't be simulated (saved flights still open).</Text>
              )}
              {engines.engines.map(e => (
                <div key={e.id} className={styles.engine}>
                  <Text>{e.name} <Text size={200} className={styles.hint}>{e.version} · {e.license ?? "license unknown"} · opens {e.extensions.map(x => `.${x}`).join(", ")}</Text></Text>
                  <Text className={styles.mono}>{e.dir}</Text>
                </div>
              ))}
              {engines.problems.map(p => <Text key={p} className={styles.bad}><ErrorCircleRegular />{p}</Text>)}
              <div className={styles.engine}>
                <Text>Java runtime</Text>
                {engines.java
                  ? <Text className={styles.mono}>{engines.java}</Text>
                  : <Text className={styles.bad}><ErrorCircleRegular />{engines.java_error}</Text>}
              </div>
            </>}
          </section>
        )}
        {error && <Text className={styles.bad}>{error}</Text>}
      </DialogContent>
      <DialogActions>
        <Button appearance="primary" onClick={() => useStore.setState({ dialog: null })}>Done</Button>
      </DialogActions>
    </DialogBody>
  );
}

/** API key for a provider: checked against the provider before it is saved. */
function KeyField({ provider, onSaved }: { provider: ImageryProviderInfo; onSaved: () => void }) {
  const styles = useStyles();
  const [key, setKey] = useState("");
  const [state, setState] = useState<{ busy?: boolean; error?: string; saved?: string }>({});

  const apply = async (value: string) => {
    setState({ busy: true });
    try {
      await desktop.setImageryKey(provider.id, value);
      setKey("");
      setState({ saved: value ? "Key checked and saved." : "Key removed." });
      onSaved();
      // Reload the imagery so the maps switch endpoints now.
      useStore.setState(s => ({ imageryEpoch: s.imageryEpoch + 1 }));
    } catch (e) {
      setState({ error: String(e) });
    }
  };

  return (
    <div className={styles.section}>
      <div className={styles.keyRow}>
        <Field className={styles.keyInput} label={`${provider.name} API key`}>
          <Input type="password" value={key} placeholder={provider.has_key ? "•••••••• (saved)" : "Paste key"} onChange={(_, d) => setKey(d.value)} />
        </Field>
        <Button appearance="primary" disabled={!key.trim() || state.busy} onClick={() => apply(key)}>
          {state.busy ? "Checking…" : "Save key"}
        </Button>
        {provider.has_key && <Button disabled={state.busy} onClick={() => apply("")}>Remove</Button>}
      </div>
      {state.error && <Text size={200} className={styles.bad}><ErrorCircleRegular />{state.error}</Text>}
      {provider.id === "esri" && (
        <Text size={200} className={styles.hint}>
          Optional. Without a key, rocketviz uses Esri's public endpoint. A free ArcGIS Location Platform account
          (location.arcgis.com) gives you a key with basemap access.
        </Text>
      )}
      {provider.has_key && !state.error && (
        <Text size={200} className={styles.ok}>
          <CheckmarkCircleRegular />{state.saved ?? "Using your key."} <Badge appearance="tint" size="small">premium endpoint</Badge>
        </Text>
      )}
      {!provider.has_key && state.saved && <Text size={200} className={styles.hint}>{state.saved}</Text>}
    </div>
  );
}
