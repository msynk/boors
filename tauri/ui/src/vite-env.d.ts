/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "1" serves fixtures from src/mocks instead of the API (`npm run dev:mock`). */
  readonly VITE_MOCK?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
