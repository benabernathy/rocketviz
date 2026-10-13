// Progress strip under the toolbar: a running analysis (with Cancel), something slow starting, or a message.
import { useState } from "react";
import {
  Button, MessageBar, MessageBarActions, MessageBarBody, ProgressBar, Text, makeStyles, tokens,
} from "@fluentui/react-components";
import { DismissRegular } from "@fluentui/react-icons";
import { useStore } from "../store";
import { cancelSimulation, runFraction } from "../analysis";

const useStyles = makeStyles({
  bar: {
    display: "grid",
    gridTemplateColumns: "1fr auto",
    alignItems: "center",
    columnGap: tokens.spacingHorizontalM,
    rowGap: tokens.spacingVerticalXXS,
    padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}`,
    backgroundColor: tokens.colorNeutralBackground2,
    borderBottom: `${tokens.strokeWidthThin} solid ${tokens.colorNeutralStroke2}`,
  },
  text: { fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" },
  progress: { gridColumn: "1 / -1" },
  notice: { borderRadius: 0 },
});

const STAGE = { nominal: "Nominal flight", monte_carlo: "Monte Carlo", sweep: "Wind sweep" } as const;

export function RunBar() {
  const styles = useStyles();
  const run = useStore(s => s.run);
  const busy = useStore(s => s.busy);
  const notice = useStore(s => s.notice);
  const [cancelling, setCancelling] = useState(false);

  if (run) {
    const counts = run.total ? ` ${run.done} / ${run.total}` : "";
    const which = run.configs.length > 1 ? ` (${run.index + 1} of ${run.configs.length})` : "";
    return (
      <div className={styles.bar} role="status">
        <Text size={200} className={styles.text}>
          Simulating {run.configs[run.index]}{which} · {STAGE[run.stage]}{counts}
        </Text>
        <Button size="small" icon={<DismissRegular />} disabled={cancelling}
          onClick={() => { setCancelling(true); cancelSimulation().finally(() => setCancelling(false)); }}>
          {cancelling ? "Cancelling…" : "Cancel"}
        </Button>
        <ProgressBar className={styles.progress} value={runFraction(run)} thickness="large" />
      </div>
    );
  }
  if (busy) {
    return (
      <div className={styles.bar} role="status">
        <Text size={200} className={styles.text}>{busy}</Text>
        <span />
        <ProgressBar className={styles.progress} />
      </div>
    );
  }
  if (notice) {
    return (
      <MessageBar intent={notice.intent} className={styles.notice}>
        <MessageBarBody style={{ whiteSpace: "pre-wrap" }}>{notice.text}</MessageBarBody>
        <MessageBarActions containerAction={
          <Button appearance="transparent" aria-label="Dismiss" icon={<DismissRegular />} onClick={() => useStore.setState({ notice: null })} />
        } />
      </MessageBar>
    );
  }
  return null;
}
