import type { FlightData } from "./types";
import { useStore } from "./store";

function validate(d: unknown): FlightData {
  const f = d as FlightData;
  if (!f || f.version !== 1 || !Array.isArray(f.sims) || !f.sims.length) {
    throw new Error("Not an orviz flight file. Generate one with: uv run orviz.py MyRocket.ork --app");
  }
  return f;
}

export async function loadFromUrl(url: string) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  useStore.getState().setData(validate(await r.json()), url.split("/").pop() ?? url);
}

export async function loadFromFile(file: File) {
  useStore.getState().setData(validate(JSON.parse(await file.text())), file.name);
}

/** Opens a file picker for a .orviz.json. */
export function pickFile(onError: (e: Error) => void) {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = ".json,application/json";
  input.onchange = () => { const f = input.files?.[0]; if (f) loadFromFile(f).catch(onError); };
  input.click();
}
