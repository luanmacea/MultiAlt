import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { LiveWindow } from "./live/LiveWindow";
import "./i18n";
import "./index.css";

// A janela "MultiAlt Live" (commands/live_view.rs) abre o mesmo frontend com
// `?view=live` e só mostra a grade das contas (docs/features/live.md).
const isLiveWindow = new URLSearchParams(window.location.search).get("view") === "live";

createRoot(document.getElementById("root")!).render(isLiveWindow ? <LiveWindow /> : <App />);

// Sinal de vida da interface para o watchdog do WebView2
// (`src-tauri/src/webview_recovery.rs`). Dois quadros depois do render o
// navegador já pintou de fato — avisar no fim do `render()` mentiria, porque o
// WebView2 pode montar a árvore e nunca desenhar nada (é justamente a falha de
// composição que deixa a janela em branco). Sem este aviso dentro do prazo o
// backend reabre o app em safe mode de vídeo.
// Só a janela principal: o watchdog é dela.
if (!isLiveWindow) {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void invoke("frontend_painted").catch(() => {});
    });
  });
}
