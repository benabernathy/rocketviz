// Updates (desktop app): signed releases on GitHub, verified against the public key in tauri.conf.json.
import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useStore } from "./store";

/**
 * Looks for a newer release. Quietly at startup (`manual` false: only an available update is mentioned), or from the
 * menu (`manual`: also says when it's up to date or the check failed).
 */
export async function checkForUpdates(manual: boolean) {
  const say = (intent: "info" | "success" | "error", text: string) => useStore.setState({ notice: { intent, text } });
  try {
    const update = await check();
    if (!update) {
      if (manual) say("success", `rocketviz ${await getVersion()} is the latest version.`);
      return;
    }
    useStore.setState({
      notice: {
        intent: "info",
        text: `rocketviz ${update.version} is available (you have ${update.currentVersion}).`,
        action: {
          label: "Install and restart",
          run: async () => {
            if (useStore.getState().run) { say("error", "Finish or cancel the simulation before updating."); return; }
            let total = 0, done = 0;
            useStore.setState({ notice: null, busy: `Downloading rocketviz ${update.version}…` });
            try {
              await update.downloadAndInstall(e => {
                if (e.event === "Started") total = e.data.contentLength ?? 0;
                else if (e.event === "Progress") {
                  done += e.data.chunkLength;
                  if (total) useStore.setState({ busy: `Downloading rocketviz ${update.version}… ${Math.round((100 * done) / total)} %` });
                } else useStore.setState({ busy: "Installing the update…" });
              });
              await relaunch();
            } catch (e) {
              useStore.setState({ busy: null });
              say("error", `The update didn't install: ${e instanceof Error ? e.message : String(e)}`);
            }
          },
        },
      },
    });
  } catch (e) {
    if (manual) say("error", `Couldn't check for updates: ${e instanceof Error ? e.message : String(e)}`);
  }
}
