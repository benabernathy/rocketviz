import {
  Button, Dropdown, Option, Slider, Text, Tooltip, Menu, MenuTrigger, MenuPopover, MenuList, MenuItem,
  makeStyles, tokens,
} from "@fluentui/react-components";
import {
  PlayFilled, PauseFilled, PreviousRegular, ChevronLeftRegular, ChevronRightRegular, FlagRegular,
} from "@fluentui/react-icons";
import { useSim, useStore } from "../store";
import { endTime, keyEvents, phaseAt } from "../flight";

const useStyles = makeStyles({
  bar: {
    display: "flex",
    alignItems: "center",
    gap: tokens.spacingHorizontalS,
    padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}`,
    backgroundColor: tokens.colorNeutralBackground1,
    borderTop: `${tokens.strokeWidthThin} solid ${tokens.colorNeutralStroke2}`,
    minHeight: "48px",
  },
  slider: { flex: 1, minWidth: "120px" },
  clock: {
    fontFamily: tokens.fontFamilyMonospace,
    fontVariantNumeric: "tabular-nums",
    minWidth: "120px",
    textAlign: "right",
    whiteSpace: "nowrap",
  },
  phase: { display: "inline-flex", alignItems: "center", gap: tokens.spacingHorizontalXS, minWidth: "104px", whiteSpace: "nowrap" },
  dot: { width: "10px", height: "10px", borderRadius: tokens.borderRadiusCircular, flexShrink: 0 },
  speed: { minWidth: "80px", width: "80px" },
});

const SPEEDS = [0.25, 0.5, 1, 2, 4, 10, 30];

export function Transport() {
  const styles = useStyles();
  const sim = useSim();
  const t = useStore(s => s.t);
  const playing = useStore(s => s.playing);
  const speed = useStore(s => s.speed);
  const { setT, togglePlay, setSpeed, setPlaying } = useStore.getState();
  if (!sim) return null;

  const end = endTime(sim);
  const events = keyEvents(sim);
  const phase = phaseAt(sim, t);
  const prev = [...events].reverse().find(e => e.t < t - 0.05);
  const next = events.find(e => e.t > t + 0.05);
  const seek = (x: number) => { setPlaying(false); setT(x); };

  return (
    <footer className={styles.bar}>
      <Tooltip content={playing ? "Pause (Space)" : "Play (Space)"} relationship="label">
        <Button appearance="primary" icon={playing ? <PauseFilled /> : <PlayFilled />} onClick={togglePlay} />
      </Tooltip>
      <Tooltip content="Back to T+0 (Home)" relationship="label">
        <Button appearance="subtle" icon={<PreviousRegular />} onClick={() => seek(0)} />
      </Tooltip>
      <Tooltip content={prev ? `Previous event: ${prev.label}` : "No earlier event"} relationship="label">
        <Button appearance="subtle" icon={<ChevronLeftRegular />} disabled={!prev} onClick={() => prev && seek(prev.t)} />
      </Tooltip>
      <Tooltip content={next ? `Next event: ${next.label}` : "No later event"} relationship="label">
        <Button appearance="subtle" icon={<ChevronRightRegular />} disabled={!next} onClick={() => next && seek(next.t)} />
      </Tooltip>
      <Menu>
        <MenuTrigger disableButtonEnhancement>
          <Button appearance="subtle" icon={<FlagRegular />}>Events</Button>
        </MenuTrigger>
        <MenuPopover>
          <MenuList>
            {events.map(e => (
              <MenuItem key={e.type} onClick={() => seek(e.t)} secondaryContent={`T+${e.t.toFixed(1)} s`}>{e.label}</MenuItem>
            ))}
          </MenuList>
        </MenuPopover>
      </Menu>

      <span className={styles.phase}>
        <span className={styles.dot} style={{ background: phase.color }} />
        <Text weight="semibold">{phase.label}</Text>
      </span>

      <Slider
        className={styles.slider}
        min={0}
        max={Math.round(end * 10)}
        value={Math.round(t * 10)}
        onChange={(_, d) => seek(d.value / 10)}
        aria-label="Flight time"
      />
      <Text className={styles.clock}>T+{t.toFixed(1)} / {end.toFixed(1)} s</Text>

      <Dropdown
        className={styles.speed}
        aria-label="Playback speed"
        value={`${speed}×`}
        selectedOptions={[String(speed)]}
        onOptionSelect={(_, d) => setSpeed(Number(d.optionValue))}
      >
        {SPEEDS.map(x => <Option key={x} value={String(x)}>{`${x}×`}</Option>)}
      </Dropdown>
    </footer>
  );
}
