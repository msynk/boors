// App shell: top bar (brand, symbol search, palette, health chip, settings), side navigation (bottom tabs on phones)
// and the routed page.
import { A } from '@solidjs/router';
import { Show, Suspense, type ParentProps } from 'solid-js';
import { client } from '../api';
import { t } from '../i18n';
import { useHealth } from '../state/health';
import { settings, toggleColors, toggleDensity, toggleDigits, toggleLanguage, toggleTheme, usesPersianDigits } from '../state/settings';
import { CommandPalette, openPalette } from './CommandPalette';
import { Icon, type IconName } from './Icon';
import { SymbolSearch } from './SymbolSearch';
import { Skeleton } from './ui';

const nav: { href: string; label: () => string; icon: IconName; end?: boolean }[] = [
  { href: '/market', label: () => t('navOverview'), icon: 'overview', end: true },
  { href: '/market/watch', label: () => t('navWatch'), icon: 'watch' },
  { href: '/market/screens', label: () => t('navScreens'), icon: 'screens' },
  { href: '/market/health', label: () => t('navHealth'), icon: 'health' },
];

function SettingsToolbar() {
  const s = settings;
  return (
    <div class="toolbar" role="group" aria-label={t('settings')}>
      <button
        type="button"
        class="icon-btn"
        onClick={toggleTheme}
        title={t('toggleTheme', s().theme === 'dark' ? t('themeLight') : t('themeDark'))}
        aria-label={t('toggleTheme', s().theme === 'dark' ? t('themeLight') : t('themeDark'))}
      >
        <Icon name={s().theme === 'dark' ? 'sun' : 'moon'} />
      </button>
      <button type="button" class="text-btn" onClick={toggleLanguage} title={t('language')} lang={s().lang === 'fa' ? 'en' : 'fa'}>
        {t('switchToLanguage')}
      </button>
      <button
        type="button"
        class="text-btn num"
        onClick={toggleDigits}
        title={usesPersianDigits(s()) ? t('digitsLatin') : t('digitsPersian')}
        aria-label={usesPersianDigits(s()) ? t('digitsLatin') : t('digitsPersian')}
      >
        {usesPersianDigits(s()) ? '123' : '۱۲۳'}
      </button>
      <button
        type="button"
        class="icon-btn colors-btn"
        onClick={toggleColors}
        aria-pressed={s().colors === 'cb'}
        title={`${t('colors')}: ${s().colors === 'iran' ? t('colorsIran') : t('colorsCb')}`}
        aria-label={`${t('colors')}: ${s().colors === 'iran' ? t('colorsIran') : t('colorsCb')}`}
      >
        <span class="swatch up" aria-hidden="true" />
        <span class="swatch down" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="text-btn"
        onClick={toggleDensity}
        title={s().density === 'pro' ? t('densityProHint') : t('densityLiteHint')}
        aria-label={`${t('density')}: ${s().density === 'pro' ? t('densityPro') : t('densityLite')}`}
      >
        <Icon name={s().density === 'pro' ? 'density' : 'densityLite'} size={16} />
        <span>{s().density === 'pro' ? t('densityPro') : t('densityLite')}</span>
      </button>
    </div>
  );
}

function HealthChip() {
  const health = useHealth();
  const transport = () => (client.transportKind === 'ipc' ? t('transportIpc') : client.transportKind === 'mock' ? t('transportMock') : t('transportHttp'));
  return (
    <Show when={health()}>
      {(h) => (
        <A href="/market/health" class={`health-chip ${h().healthy ? 'ok' : 'warn'}`} title={transport()}>
          <Icon name={h().healthy ? 'okCircle' : 'warning'} size={15} />
          <span class="health-chip-label">{h().healthy ? t('healthChipOk') : t('healthChip')}</span>
        </A>
      )}
    </Show>
  );
}

export function Shell(props: ParentProps) {
  return (
    <div class="shell">
      <a class="skip-link" href="#main">
        {t('skipToContent')}
      </a>
      <header class="topbar">
        <A href="/market" class="brand" aria-label={t('appName')}>
          <svg class="brand-mark" viewBox="0 0 32 32" aria-hidden="true">
            <rect width="32" height="32" rx="7" />
            <path d="M7 21l5-6 4 3 7-9" />
          </svg>
          <span class="brand-name">{t('appName')}</span>
        </A>
        <SymbolSearch class="topbar-search" autofocusKey />
        <button type="button" class="palette-btn" onClick={openPalette} aria-label={t('paletteOpen')} title={t('paletteOpen')}>
          <Icon name="command" size={16} />
          <span class="palette-btn-label">{t('paletteOpen')}</span>
          <kbd class="kbd">Ctrl K</kbd>
        </button>
        <div class="topbar-spacer" />
        <HealthChip />
        <SettingsToolbar />
      </header>
      <nav class="sidenav" aria-label={t('navMarket')}>
        <div class="sidenav-group">{t('navMarket')}</div>
        {nav.map((item) => (
          <A href={item.href} end={item.end} class="nav-link" activeClass="active">
            <Icon name={item.icon} />
            <span>{item.label()}</span>
          </A>
        ))}
      </nav>
      <main id="main" class="main" tabIndex={-1}>
        {/* While a route's chunk loads, show the page skeleton instead of an empty pane. */}
        <Suspense
          fallback={
            <div class="page">
              <Skeleton height="2rem" class="w-40" />
              <Skeleton height="7rem" />
              <Skeleton height="22rem" />
            </div>
          }
        >
          {props.children}
        </Suspense>
      </main>
      <CommandPalette />
    </div>
  );
}
