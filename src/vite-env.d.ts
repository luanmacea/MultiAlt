/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_ENABLE_NEXUS?: string;
  readonly VITE_ENABLE_WEBSERVER?: string;
  readonly VITE_ENABLE_AVATAR_BATCH?: string;
  readonly VITE_ENABLE_ACCOUNT_GENERATOR?: string;
  readonly VITE_ENABLE_HELP_BUTTON?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
