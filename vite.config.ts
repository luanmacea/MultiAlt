import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { execSync } from "node:child_process";

const host = process.env.TAURI_DEV_HOST;

/**
 * Versão do build local de teste para a barra de título (src/buildInfo.ts): a
 * última versão publicada + o commit. Vazio na release (GitHub Actions), que
 * grava o número certo no tauri.conf.json e o app lê com `getVersion()`.
 */
function testBuildLabel(): string {
  if (process.env.GITHUB_ACTIONS) return "";
  const git = (args: string) =>
    execSync(`git ${args}`, { windowsHide: true, stdio: ["ignore", "pipe", "ignore"] }).toString().trim();
  try {
    return `${git("describe --tags --abbrev=0")} + test ${git("rev-parse --short HEAD")}`;
  } catch {
    return "test build";
  }
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: {
    __TEST_BUILD_LABEL__: JSON.stringify(testBuildLabel()),
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
