// Docking workspace (dockview). Every panel can be dragged, re-tabbed, resized, and maximized from its header.
import { useEffect, useState } from "react";
import {
  DockviewReact, themeDark, themeLight,
  type DockviewApi, type DockviewReadyEvent, type IDockviewHeaderActionsProps, type IDockviewPanelProps,
} from "dockview-react";
import "dockview-react/dist/styles/dockview.css";
import { Button, Tooltip } from "@fluentui/react-components";
import { ArrowMaximizeRegular, ArrowMinimizeRegular } from "@fluentui/react-icons";
import { Globe3D } from "../panels/Globe3D";
import { Map2D } from "../panels/Map2D";
import { Plots } from "../panels/Plots";
import { FlightInfo } from "../panels/FlightInfo";
import { useStore } from "../store";

const LAYOUT_KEY = "rocketviz.layout.v1";
export const PANELS: Record<string, { title: string; component: React.FunctionComponent<IDockviewPanelProps> }> = {
  globe: { title: "3D view", component: Globe3D },
  map: { title: "Map", component: Map2D },
  plots: { title: "Plots", component: Plots },
  info: { title: "Flight", component: FlightInfo },
};
const components = Object.fromEntries(Object.entries(PANELS).map(([k, v]) => [k, v.component]));

let api: DockviewApi | null = null;

function defaultLayout(a: DockviewApi) {
  a.clear();
  // WebGL views stay mounted when hidden behind another tab, so they keep their camera and state.
  const globe = a.addPanel({ id: "globe", component: "globe", title: PANELS.globe.title, renderer: "always" });
  a.addPanel({ id: "map", component: "map", title: PANELS.map.title, renderer: "always",
    position: { referencePanel: globe, direction: "right" }, initialWidth: Math.round(a.width * 0.32) });
  a.addPanel({ id: "plots", component: "plots", title: PANELS.plots.title, renderer: "always",
    position: { direction: "below" }, initialHeight: Math.round(a.height * 0.34) });
  a.addPanel({ id: "info", component: "info", title: PANELS.info.title,
    position: { direction: "left" }, initialWidth: 280 });
  globe.api.setActive();
}

export function resetLayout() {
  if (api) defaultLayout(api);
}

/** Show or hide a panel; a reopened panel goes back to roughly its default spot. */
export function togglePanel(id: string) {
  if (!api) return;
  const open = api.getPanel(id);
  if (open) { open.api.close(); return; }
  const anchor = (other: string) => api!.getPanel(other);
  const base = { id, component: id, title: PANELS[id].title };
  if (id === "globe") api.addPanel({ ...base, renderer: "always", position: anchor("map") ? { referencePanel: anchor("map")!, direction: "left" } : undefined });
  else if (id === "map") api.addPanel({ ...base, renderer: "always", position: anchor("globe") ? { referencePanel: anchor("globe")!, direction: "right" } : undefined });
  else if (id === "plots") api.addPanel({ ...base, renderer: "always", position: { direction: "below" }, initialHeight: Math.round(api.height * 0.34) });
  else api.addPanel({ ...base, position: { direction: "left" }, initialWidth: 280 });
}

const syncOpen = (a: DockviewApi) => useStore.setState({ openPanels: a.panels.map(p => p.id) });

function restore(a: DockviewApi) {
  try {
    const saved = localStorage.getItem(LAYOUT_KEY);
    if (saved) {
      a.fromJSON(JSON.parse(saved));
      // Panels the user closed stay closed (reopen them from View > Panels); an empty layout falls back.
      if (a.panels.length && a.panels.every(p => p.id in PANELS)) return;
    }
  } catch { /* corrupt or unavailable storage: use the default */ }
  defaultLayout(a);
}

function MaximizeAction({ api: group, containerApi }: IDockviewHeaderActionsProps) {
  const [max, setMax] = useState(group.isMaximized());
  useEffect(() => {
    const d = containerApi.onDidMaximizedGroupChange(() => setMax(group.isMaximized()));
    return () => d.dispose();
  }, [group, containerApi]);
  return (
    <Tooltip content={max ? "Restore" : "Maximize"} relationship="label">
      <Button
        appearance="transparent"
        size="small"
        icon={max ? <ArrowMinimizeRegular /> : <ArrowMaximizeRegular />}
        onClick={() => (max ? group.exitMaximized() : group.maximize())}
      />
    </Tooltip>
  );
}

export function DockLayout({ dark }: { dark: boolean }) {
  const onReady = (e: DockviewReadyEvent) => {
    api = e.api;
    restore(e.api);
    syncOpen(e.api);
    e.api.onDidAddPanel(() => syncOpen(e.api));
    e.api.onDidRemovePanel(() => syncOpen(e.api));
    e.api.onDidLayoutChange(() => {
      try { localStorage.setItem(LAYOUT_KEY, JSON.stringify(e.api.toJSON())); } catch { /* storage unavailable */ }
    });
  };
  useEffect(() => () => { api = null; }, []);

  return (
    <DockviewReact
      className="rocketviz-dock"
      components={components}
      onReady={onReady}
      theme={dark ? themeDark : themeLight}
      rightHeaderActionsComponent={MaximizeAction}
    />
  );
}
