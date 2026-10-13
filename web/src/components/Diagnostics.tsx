// Environment report for performance testing: which webview, and whether WebGL is on the GPU or in software.
import { useMemo, useState } from "react";
import {
  Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, makeStyles, tokens,
} from "@fluentui/react-components";
import { CopyRegular } from "@fluentui/react-icons";
import { isTauri } from "@tauri-apps/api/core";

const useStyles = makeStyles({
  kv: { display: "grid", gridTemplateColumns: "max-content 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalL}`, margin: 0 },
  dt: { color: tokens.colorNeutralForeground3 },
  dd: { margin: 0, fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, wordBreak: "break-word" },
  warn: { color: tokens.colorPaletteRedForeground1, fontWeight: tokens.fontWeightSemibold },
});

export function collectDiagnostics(): [string, string][] {
  const canvas = document.createElement("canvas");
  const gl = canvas.getContext("webgl2") ?? canvas.getContext("webgl");
  const dbg = gl?.getExtension("WEBGL_debug_renderer_info");
  const renderer = gl ? String(dbg ? gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER)) : "none";
  const vendor = gl ? String(dbg ? gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR)) : "none";
  return [
    ["Runtime", isTauri() ? "Desktop app (Tauri)" : "Browser"],
    ["User agent", navigator.userAgent],
    ["WebGL", gl ? (typeof WebGL2RenderingContext !== "undefined" && gl instanceof WebGL2RenderingContext ? "WebGL 2" : "WebGL 1") : "unavailable"],
    ["GPU vendor", vendor],
    ["GPU renderer", renderer],
    ["Max texture size", gl ? String(gl.getParameter(gl.MAX_TEXTURE_SIZE)) : "—"],
    ["Pixel ratio", String(devicePixelRatio)],
    ["Screen", `${screen.width}×${screen.height}`],
    ["CPU threads", String(navigator.hardwareConcurrency ?? "?")],
  ];
}

/** Software rasterizers mean the GPU isn't being used and 3D will be slow. */
const isSoftware = (r: string) => /llvmpipe|swiftshader|softpipe|software|basic render/i.test(r);

export function Diagnostics({ open, onClose }: { open: boolean; onClose: () => void }) {
  const styles = useStyles();
  const rows = useMemo(() => (open ? collectDiagnostics() : []), [open]);
  const [copied, setCopied] = useState(false);
  const report = rows.map(([k, v]) => `${k}: ${v}`).join("\n");

  return (
    <Dialog open={open} onOpenChange={(_, d) => { if (!d.open) { setCopied(false); onClose(); } }}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>Diagnostics</DialogTitle>
          <DialogContent>
            <dl className={styles.kv}>
              {rows.map(([k, v]) => [
                <dt key={`${k}-k`} className={styles.dt}>{k}</dt>,
                <dd key={`${k}-v`} className={`${styles.dd} ${k === "GPU renderer" && isSoftware(v) ? styles.warn : ""}`}>
                  {v}{k === "GPU renderer" && isSoftware(v) ? "  (software rendering: GPU not in use)" : ""}
                </dd>,
              ])}
            </dl>
          </DialogContent>
          <DialogActions>
            <Button icon={<CopyRegular />} onClick={() => navigator.clipboard.writeText(report).then(() => setCopied(true))}>
              {copied ? "Copied" : "Copy report"}
            </Button>
            <Button appearance="primary" onClick={onClose}>Close</Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
