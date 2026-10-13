import { useEffect, useState } from "react";
import { FluentProvider, Spinner, makeStyles, tokens } from "@fluentui/react-components";
import { Toolbar } from "./components/Toolbar";
import { Transport } from "./components/Transport";
import { DockLayout } from "./components/DockLayout";
import { EmptyState } from "./components/EmptyState";
import { useStore } from "./store";
import { fluentTheme, useIsDark } from "./theme";
import { loadFromUrl } from "./data";
import { runSmokeIfRequested } from "./smoke";

const useStyles = makeStyles({
  root: {
    position: "fixed",
    inset: 0,
    display: "flex",
    flexDirection: "column",
    backgroundColor: tokens.colorNeutralBackground3,
    color: tokens.colorNeutralForeground1,
  },
  main: { flex: 1, minHeight: 0, position: "relative" },
  center: { position: "absolute", inset: 0, display: "grid", placeItems: "center" },
});

export default function App() {
  const dark = useIsDark();
  const styles = useStyles();
  const hasData = useStore(s => s.data != null);
  const [loading, setLoading] = useState(true);
  const togglePlay = useStore(s => s.togglePlay);

  useEffect(() => {
    // `?data=<url>` picks a file; otherwise data.json next to the app (rocketviz.py --serve / vite dev provide it).
    const url = new URLSearchParams(location.search).get("data") ?? "data.json";
    loadFromUrl(url)
      .catch(() => { /* no bundled data: show the open-file screen */ })
      .finally(() => { setLoading(false); runSmokeIfRequested(); });
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (el.closest("input, textarea, select, [role=combobox], [role=menu], [role=listbox]")) return;
      const s = useStore.getState();
      if (e.code === "Space") { e.preventDefault(); togglePlay(); }
      else if (e.key === "ArrowRight") s.setT(s.t + (e.shiftKey ? 5 : 1));
      else if (e.key === "ArrowLeft") s.setT(s.t - (e.shiftKey ? 5 : 1));
      else if (e.key === "Home") s.setT(0);
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, [togglePlay]);

  return (
    // Layout styles go on an inner div: Fluent copies the provider's className onto its portal (menu/tooltip) hosts.
    <FluentProvider theme={fluentTheme(dark)} data-theme={dark ? "dark" : "light"}>
      <div className={styles.root}>
        <Toolbar />
        <main className={styles.main}>
          {hasData ? <DockLayout dark={dark} />
            : <div className={styles.center}>{loading ? <Spinner label="Loading flight data…" /> : <EmptyState />}</div>}
        </main>
        {hasData && <Transport />}
      </div>
    </FluentProvider>
  );
}
