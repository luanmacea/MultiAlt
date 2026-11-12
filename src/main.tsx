import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import "./i18n";
import "./index.css";

createRoot(document.getElementById("root")!).render(<App />);

// Sinal de vida da interface para o watchdog do WebView2
// (`src-tauri/src/webview_recovery.rs`). Dois quadros depois do render o
// navegador já pintou de fato — avisar no fim do `render()` mentiria, porque o
// WebView2 pode montar a árvore e nunca desenhar nada (é justamente a falha de
// composição que deixa a janela em branco). Sem este aviso dentro do prazo o
// backend reabre o app em safe mode de vídeo.
requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    void invoke("frontend_painted").catch(() => {});
  });
});
