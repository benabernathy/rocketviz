import { useState } from "react";
import {
  Menu, MenuItemCheckbox, MenuItemRadio, MenuList, MenuPopover, MenuTrigger, MenuDivider, MenuGroup, MenuGroupHeader,
  MenuItem, Tab, TabList, Text, Toolbar as FToolbar, ToolbarButton, ToolbarDivider, Tooltip, makeStyles, tokens,
  type MenuCheckedValueChangeData,
} from "@fluentui/react-components";
import {
  FolderOpenRegular, LayerRegular, FullScreenMaximizeRegular, LocationRegular, DarkThemeRegular, GridRegular,
  RocketRegular, InfoRegular, DocumentRegular, SaveRegular, GlobeArrowUpRegular, SettingsRegular, PlayRegular,
  ArrowSyncRegular,
} from "@fluentui/react-icons";
import { isTauri } from "@tauri-apps/api/core";
import { useStore, type Layers, type ThemeMode } from "../store";
import { motorLabel } from "../flight";
import { exportKml, pickFile, saveFlightAs } from "../data";
import { checkForUpdates } from "../updates";
import { PANELS, resetLayout, togglePanel } from "./DockLayout";
import { Diagnostics } from "./Diagnostics";

const useStyles = makeStyles({
  bar: {
    display: "flex",
    alignItems: "center",
    gap: tokens.spacingHorizontalM,
    padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}`,
    backgroundColor: tokens.colorNeutralBackground1,
    borderBottom: `${tokens.strokeWidthThin} solid ${tokens.colorNeutralStroke2}`,
    minHeight: "44px",
    overflowX: "auto",
  },
  brand: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, whiteSpace: "nowrap" },
  logo: { color: tokens.colorBrandForeground1, fontSize: "20px" },
  sub: { color: tokens.colorNeutralForeground3 },
  spacer: { flex: 1 },
});

const MOD = /Mac/.test(navigator.platform) ? "⌘" : "Ctrl+";

/** Reports a failed file action under the toolbar. */
export const fail = (what: string) => (e: unknown) =>
  useStore.setState({ notice: { intent: "error", text: `${what}: ${e instanceof Error ? e.message : String(e)}` } });

const LAYER_ITEMS: [keyof Layers, string][] = [
  ["mc", "Monte Carlo landings"],
  ["ellipses", "1σ / 2σ / 3σ landing zones"],
  ["sweep", "Wind sweep"],
  ["follow", "Camera follows rocket"],
];

export function Toolbar() {
  const styles = useStyles();
  const data = useStore(s => s.data);
  const dataName = useStore(s => s.dataName);
  const simIdx = useStore(s => s.simIdx);
  const setSim = useStore(s => s.setSim);
  const layers = useStore(s => s.layers);
  const setLayer = useStore(s => s.setLayer);
  const theme = useStore(s => s.theme);
  const setTheme = useStore(s => s.setTheme);
  const frame = useStore(s => s.frame);
  const resetPad = useStore(s => s.resetPad);
  const openPanels = useStore(s => s.openPanels);
  const showFps = useStore(s => s.showFps);
  const [diagOpen, setDiagOpen] = useState(false);
  const design = useStore(s => s.design);
  const running = useStore(s => s.run != null || s.busy != null);
  const dirty = useStore(s => s.dirty);
  const desktopApp = isTauri();

  const checked = {
    layers: LAYER_ITEMS.filter(([k]) => layers[k]).map(([k]) => k), theme: [theme], panels: openPanels,
    debug: showFps ? ["fps"] : [],
  };
  const onCheck = (_: unknown, d: MenuCheckedValueChangeData) => {
    if (d.name === "theme") setTheme(d.checkedItems[0] as ThemeMode);
    else if (d.name === "debug") useStore.setState({ showFps: d.checkedItems.includes("fps") });
    else if (d.name === "panels") Object.keys(PANELS).forEach(id => { if (d.checkedItems.includes(id) !== openPanels.includes(id)) togglePanel(id); });
    else LAYER_ITEMS.forEach(([k]) => { if (d.checkedItems.includes(k) !== layers[k]) setLayer(k, !layers[k]); });
  };

  return (
    <header className={styles.bar}>
      <div className={styles.brand}>
        <RocketRegular className={styles.logo} />
        <Text weight="semibold" size={400}>{data?.rocket ?? "rocketviz"}</Text>
        {data && <Text size={200} className={styles.sub}>{dataName}{dirty ? " · not saved" : ""}</Text>}
      </div>

      {data && data.sims.length > 1 && (
        <TabList size="small" selectedValue={simIdx} onTabSelect={(_, d) => setSim(d.value as number)} aria-label="Motor configuration">
          {data.sims.map((s, i) => <Tab key={i} value={i}>{motorLabel(s)}</Tab>)}
        </TabList>
      )}

      <div className={styles.spacer} />

      <FToolbar size="small" aria-label="View">
        <Menu>
          <MenuTrigger disableButtonEnhancement>
            <ToolbarButton icon={<DocumentRegular />}>File</ToolbarButton>
          </MenuTrigger>
          <MenuPopover>
            <MenuList>
              <MenuItem icon={<FolderOpenRegular />} secondaryContent={`${MOD}O`} disabled={running} onClick={() => pickFile(fail("Couldn't open"))}>
                {desktopApp ? "Open design or flight…" : "Open flight…"}
              </MenuItem>
              {desktopApp && <>
                <MenuItem icon={<SaveRegular />} secondaryContent={`${MOD}S`} disabled={!data || running} onClick={() => saveFlightAs().catch(fail("Couldn't save"))}>
                  Save flight as…
                </MenuItem>
                <MenuItem icon={<GlobeArrowUpRegular />} disabled={!data || running} onClick={() => exportKml().catch(fail("Couldn't export"))}>
                  Export KML for Google Earth…
                </MenuItem>
                <MenuDivider />
                <MenuItem icon={<SettingsRegular />} onClick={() => useStore.setState({ dialog: "settings" })}>Settings…</MenuItem>
                <MenuItem icon={<ArrowSyncRegular />} disabled={running} onClick={() => checkForUpdates(true)}>Check for updates…</MenuItem>
              </>}
            </MenuList>
          </MenuPopover>
        </Menu>
        {design && (
          <Tooltip content={`Simulate ${design.name} with ${design.engine.name}: conditions, Monte Carlo, wind sweep`} relationship="description">
            <ToolbarButton appearance="primary" icon={<PlayRegular />} disabled={running} onClick={() => useStore.setState({ dialog: "simulate" })}>
              Simulate
            </ToolbarButton>
          </Tooltip>
        )}
        {data && <>
          <ToolbarDivider />
          <Menu checkedValues={checked} onCheckedValueChange={onCheck}>
            <MenuTrigger disableButtonEnhancement>
              <ToolbarButton icon={<LayerRegular />}>Layers</ToolbarButton>
            </MenuTrigger>
            <MenuPopover>
              <MenuList>
                {LAYER_ITEMS.map(([k, label]) => <MenuItemCheckbox key={k} name="layers" value={k}>{label}</MenuItemCheckbox>)}
              </MenuList>
            </MenuPopover>
          </Menu>
          <Tooltip content="Fit the whole flight in both maps" relationship="description">
            <ToolbarButton icon={<FullScreenMaximizeRegular />} onClick={frame}>Frame flight</ToolbarButton>
          </Tooltip>
          <Tooltip content="Move the pad back to the .ork launch site (double-click either map to move it)" relationship="description">
            <ToolbarButton icon={<LocationRegular />} onClick={resetPad}>Reset pad</ToolbarButton>
          </Tooltip>
        </>}
        <ToolbarDivider />
        <Menu checkedValues={checked} onCheckedValueChange={onCheck}>
          <MenuTrigger disableButtonEnhancement>
            <ToolbarButton icon={<DarkThemeRegular />}>View</ToolbarButton>
          </MenuTrigger>
          <MenuPopover>
            <MenuList>
              <MenuGroup>
                <MenuGroupHeader>Theme</MenuGroupHeader>
                <MenuItemRadio name="theme" value="system">System</MenuItemRadio>
                <MenuItemRadio name="theme" value="light">Light</MenuItemRadio>
                <MenuItemRadio name="theme" value="dark">Dark</MenuItemRadio>
              </MenuGroup>
              {data && <>
                <MenuDivider />
                <MenuGroup>
                  <MenuGroupHeader>Panels</MenuGroupHeader>
                  {Object.entries(PANELS).map(([id, p]) => <MenuItemCheckbox key={id} name="panels" value={id}>{p.title}</MenuItemCheckbox>)}
                </MenuGroup>
                <MenuItem icon={<GridRegular />} onClick={resetLayout}>Reset panel layout</MenuItem>
                <MenuDivider />
                <MenuItemCheckbox name="debug" value="fps">Show 3D frame rate</MenuItemCheckbox>
              </>}
              <MenuItem icon={<InfoRegular />} onClick={() => setDiagOpen(true)}>Diagnostics…</MenuItem>
            </MenuList>
          </MenuPopover>
        </Menu>
      </FToolbar>

      <Diagnostics open={diagOpen} onClose={() => setDiagOpen(false)} />
    </header>
  );
}
