import fs from "node:fs";
import path from "node:path";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { viteStaticCopy } from "vite-plugin-static-copy";

const cesium = "node_modules/cesium/Build/Cesium";

// In dev, serve flight data at /data.json: $ORVIZ_DATA, else the newest *.orviz.json in the repo root.
// (In production, `orviz.py --serve` does the same.)
function devData(): Plugin {
  return {
    name: "orviz-dev-data",
    configureServer(server) {
      server.middlewares.use("/data.json", (_req, res, next) => {
        const root = path.resolve(__dirname, "..");
        const file = process.env.ORVIZ_DATA ?? fs.readdirSync(root)
          .filter(f => f.endsWith(".orviz.json"))
          .map(f => path.join(root, f))
          .sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs)[0];
        if (!file) return next();
        res.setHeader("Content-Type", "application/json");
        fs.createReadStream(file).pipe(res);
      });
    },
  };
}

export default defineConfig({
  base: "./",
  plugins: [
    react(),
    devData(),
    // Cesium loads its workers, assets and widgets at runtime from CESIUM_BASE_URL (set in index.html).
    viteStaticCopy({
      targets: [{ src: `${cesium}/{Workers,Assets,Widgets,ThirdParty}/**/*`, dest: "cesium", rename: { stripBase: 4 } }],
    }),
  ],
  build: { chunkSizeWarningLimit: 6000 },
});
