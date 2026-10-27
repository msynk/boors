import { defineConfig, loadEnv } from 'vite';
import solid from 'vite-plugin-solid';

// The UI is served three ways: by `vite` in development (with an /api proxy to a boors-server), by boors-server from
// `dist/` (same origin, so /api/v1 just works), and by the Tauri shell from `dist/` (IPC, no HTTP at all).
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const api = process.env.BOORS_API ?? env.BOORS_API ?? 'http://localhost:5080';
  const proxy = { '/api': { target: api, changeOrigin: true } };

  return {
    plugins: [solid()],
    clearScreen: false,
    envPrefix: ['VITE_', 'TAURI_ENV_'],
    // apps/desktop/src-tauri/tauri.conf.json points devUrl at :5173, so the port is fixed.
    server: { port: 5173, strictPort: true, proxy },
    preview: { port: 4173, proxy },
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      // WebView2 (Windows), WKWebView (macOS/iOS) and WebKitGTK are all evergreen enough for ES2022.
      target: 'es2022',
      sourcemap: false,
      reportCompressedSize: true,
      chunkSizeWarningLimit: 1200,
      modulePreload: { polyfill: false },
    },
  };
});
