import { useState } from "react";
import { Button, Text, makeStyles, shorthands, tokens, MessageBar, MessageBarBody } from "@fluentui/react-components";
import { FolderOpenRegular, RocketRegular, BeakerRegular } from "@fluentui/react-icons";
import { SAMPLE_URL, loadFromFile, loadFromUrl, pickFile } from "../data";

const useStyles = makeStyles({
  box: {
    display: "grid",
    justifyItems: "center",
    gap: tokens.spacingVerticalM,
    padding: tokens.spacingHorizontalXXXL,
    maxWidth: "520px",
    textAlign: "center",
    borderRadius: tokens.borderRadiusXLarge,
    ...shorthands.border(tokens.strokeWidthThick, "dashed", tokens.colorNeutralStroke1),
  },
  over: { ...shorthands.borderColor(tokens.colorBrandStroke1), backgroundColor: tokens.colorBrandBackground2 },
  icon: { fontSize: "48px", color: tokens.colorBrandForeground1 },
  code: { fontFamily: tokens.fontFamilyMonospace, backgroundColor: tokens.colorNeutralBackground1, padding: "2px 6px", borderRadius: tokens.borderRadiusMedium },
});

export function EmptyState() {
  const styles = useStyles();
  const [over, setOver] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fail = (e: Error) => setError(e.message);

  return (
    <div
      className={`${styles.box} ${over ? styles.over : ""}`}
      onDragOver={e => { e.preventDefault(); setOver(true); }}
      onDragLeave={() => setOver(false)}
      onDrop={e => { e.preventDefault(); setOver(false); const f = e.dataTransfer.files[0]; if (f) loadFromFile(f).catch(fail); }}
    >
      <RocketRegular className={styles.icon} />
      <Text size={500} weight="semibold">Open a flight</Text>
      <Text>Drop a <span className={styles.code}>.rocketviz.json</span> file here, or create one from an OpenRocket design:</Text>
      <Text className={styles.code}>uv run rocketviz.py MyRocket.ork --app --serve</Text>
      <div style={{ display: "flex", gap: 8 }}>
        <Button appearance="primary" icon={<FolderOpenRegular />} onClick={() => pickFile(fail)}>Open file…</Button>
        <Button icon={<BeakerRegular />} onClick={() => loadFromUrl(SAMPLE_URL).catch(fail)}>Open sample flight</Button>
      </div>
      {error && <MessageBar intent="error"><MessageBarBody>{error}</MessageBarBody></MessageBar>}
    </div>
  );
}
