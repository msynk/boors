// The document title of a page: "<page> | Boors" (the tab and the Tauri window caption).
import { onCleanup } from 'solid-js';
import { t } from '../i18n';

export function setPageTitle(title: string | null): void {
  if (typeof document === 'undefined') return;
  document.title = title ? `${title} | ${t('appName')}` : `${t('appName')} | ${t('appTagline')}`;
}

/** Sets the title for the lifetime of the calling component. */
export function usePageTitle(title: string): void {
  setPageTitle(title);
  onCleanup(() => setPageTitle(null));
}
