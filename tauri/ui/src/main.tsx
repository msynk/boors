// Entry: fonts and styles, the mock transport in mock mode (excluded from production builds), then the app.
import 'vazirmatn/Vazirmatn-Variable-font-face.css';
import './styles/tokens.css';
import './styles/base.css';
import './styles/components.css';
import { render } from 'solid-js/web';
import { App } from './App';
import { client } from './api/client';
import './state/settings';

async function start() {
  // `import.meta.env.VITE_MOCK` is replaced at build time, so in a production build this branch and the whole
  // src/mocks tree are dropped.
  if (import.meta.env.VITE_MOCK === '1') {
    const { createMockTransport } = await import('./mocks');
    client.setTransport(createMockTransport(), 'mock');
  }
  const root = document.getElementById('root');
  if (root) render(() => <App />, root);
}

void start();
