// Routes. The whole router is keyed on language + digits: switching either remounts the pages so every formatted
// string is rebuilt (formatters are plain functions, not signals). Page state that must survive lives in modules.
import { Navigate, Route, Router } from '@solidjs/router';
import { Show, lazy } from 'solid-js';
import { Shell } from './components/Shell';
import Overview from './pages/Overview';
import { localeKey } from './state/settings';

const Watch = lazy(() => import('./pages/Watch'));
const SymbolPage = lazy(() => import('./pages/Symbol'));
const Screens = lazy(() => import('./pages/Screens'));
const Health = lazy(() => import('./pages/Health'));
const NotFound = lazy(() => import('./pages/NotFound'));

export function App() {
  return (
    <Show when={localeKey()} keyed>
      {(_locale: string) => (
        <Router root={Shell}>
          <Route path="/" component={() => <Navigate href="/market" />} />
          <Route path="/index.html" component={() => <Navigate href="/market" />} />
          <Route path="/market" component={Overview} />
          <Route path="/market/watch" component={Watch} />
          <Route path="/market/screens" component={Screens} />
          <Route path="/market/health" component={Health} />
          <Route path="/symbol/:insCode" component={SymbolPage} />
          <Route path="*404" component={NotFound} />
        </Router>
      )}
    </Show>
  );
}
