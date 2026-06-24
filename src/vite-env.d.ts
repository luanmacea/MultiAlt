/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_ENABLE_NEXUS?: string;
  readonly VITE_ENABLE_WEBSERVER?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
