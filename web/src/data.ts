import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { FlightData } from "./types";
import { useStore } from "./store";

/** Bundled demo flight, so the app has something to show without OpenRocket. */
export const SAMPLE_URL = "samples/fizz.rocketviz.json";

function validate(d: unknown): FlightData {
  const f = d as FlightData;
  if (!f || f.version !== 1 || !Array.isArray(f.sims) || !f.sims.length) {
    throw new Error("Not a rocketviz flight file. Generate one with: uv run rocketviz.py MyRocket.ork --app");
  }
  return f;
}

const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

export async function loadFromUrl(url: string) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  useStore.getState().setData(validate(await r.json()), baseName(url));
}

export async function loadFromFile(file: File) {
  useStore.getState().setData(validate(JSON.parse(await file.text())), file.name);
}

/** Opens a .rocketviz.json: the native dialog in the desktop app, a file input in a browser. */
export function pickFile(onError: (e: Error) => void) {
  if (isTauri()) {
    open({ multiple: false, directory: false, filters: [{ name: "rocketviz flight", extensions: ["json"] }] })
      .then(async path => {
        if (typeof path !== "string") return;
        const text = await invoke<string>("read_flight", { path });
        useStore.getState().setData(validate(JSON.parse(text)), baseName(path));
      })
      .catch(e => onError(e instanceof Error ? e : new Error(String(e))));
    return;
  }
  const input = document.createElement("input");
  input.type = "file";
  input.accept = ".json,application/json";
  input.onchange = () => { const f = input.files?.[0]; if (f) loadFromFile(f).catch(onError); };
  input.click();
}
