import { defineConfig } from "vite";
// @ts-expect-error sin el paquete @types/node esto da error de tipos
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// El puerto es 1421 y no el 1420 de siempre para poder tener DiskScope y
// PhotoForge corriendo a la vez sin que uno le quite el puerto al otro.
export default defineConfig(() => ({
  clearScreen: false,
  server: {
    port: 1421,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1422 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
}));
