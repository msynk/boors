// Ctrl+K command palette: keyboard-first search over symbols (Market/Search), pages and commands (settings).
import { useNavigate } from '@solidjs/router';
import { For, Show, createEffect, createMemo, createSignal, createUniqueId, on, onCleanup, onMount } from 'solid-js';
import { Portal } from 'solid-js/web';
import { isAbort, market, type InstrumentSearchResultDto } from '../api';
import { normalizeSearch, persianText } from '../format/format';
import { className, t } from '../i18n';
import { recent } from '../state/recent';
import { settings, toggleColors, toggleDensity, toggleDigits, toggleLanguage, toggleTheme, usesPersianDigits } from '../state/settings';
import { Icon, type IconName } from './Icon';
import { symbolHref } from './SymbolSearch';

interface PaletteItem {
  id: string;
  group: 'symbols' | 'pages' | 'commands';
  label: string;
  detail?: string;
  icon: IconName;
  keywords: string;
  run: () => void;
}

const [paletteOpen, setPaletteOpen] = createSignal(false);
export const openPalette = () => setPaletteOpen(true);

export function CommandPalette() {
  const navigate = useNavigate();
  const listId = createUniqueId();
  const [query, setQuery] = createSignal('');
  const [symbols, setSymbols] = createSignal<InstrumentSearchResultDto[]>([]);
  const [active, setActive] = createSignal(0);
  let input: HTMLInputElement | undefined;
  let list: HTMLUListElement | undefined;
  let restoreFocus: HTMLElement | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let controller: AbortController | undefined;

  const close = () => {
    setPaletteOpen(false);
    setQuery('');
    setSymbols([]);
    restoreFocus?.focus?.();
  };

  const pages = (): PaletteItem[] => [
    { id: 'p-overview', group: 'pages', label: t('navOverview'), icon: 'overview', keywords: 'overview market home نمای کلی بازار', run: () => navigate('/market') },
    { id: 'p-watch', group: 'pages', label: t('navWatch'), icon: 'watch', keywords: 'market watch filter دیده بان فیلتر', run: () => navigate('/market/watch') },
    { id: 'p-screens', group: 'pages', label: t('navScreens'), icon: 'screens', keywords: 'screens screener rank momentum غربالگری', run: () => navigate('/market/screens') },
    { id: 'p-health', group: 'pages', label: t('navHealth'), icon: 'health', keywords: 'data health jobs sync سلامت داده', run: () => navigate('/market/health') },
  ];

  const commands = (): PaletteItem[] => {
    const s = settings();
    return [
      { id: 'c-theme', group: 'commands', label: t('toggleTheme', s.theme === 'dark' ? t('themeLight') : t('themeDark')), icon: s.theme === 'dark' ? 'sun' : 'moon', keywords: 'theme dark light پوسته تیره روشن', run: toggleTheme },
      { id: 'c-lang', group: 'commands', label: t('switchLanguageCommand'), icon: 'language', keywords: 'language english persian farsi زبان', run: toggleLanguage },
      { id: 'c-digits', group: 'commands', label: usesPersianDigits(s) ? t('digitsLatin') : t('digitsPersian'), icon: 'digits', keywords: 'digits numbers ارقام', run: toggleDigits },
      { id: 'c-colors', group: 'commands', label: s.colors === 'iran' ? t('colorsCb') : t('colorsIran'), icon: 'palette', keywords: 'colour color blind رنگ کوررنگی', run: toggleColors },
      { id: 'c-density', group: 'commands', label: s.density === 'pro' ? t('densityLiteHint') : t('densityProHint'), icon: 'density', keywords: 'density lite pro ساده حرفه ای', run: toggleDensity },
    ];
  };

  const items = createMemo<PaletteItem[]>(() => {
    // eslint-disable-next-line solid/reactivity -- used synchronously by the filter below
    const q = normalizeSearch(query());
    const match = (item: PaletteItem) => !q || normalizeSearch(item.label + ' ' + item.keywords).includes(q);
    const symbolItems: PaletteItem[] = (q ? symbols() : recent().map((r) => ({ ...r, instrumentClass: '', parentInsCode: null }))).map((r) => ({
      id: `s-${r.insCode}`,
      group: 'symbols',
      label: persianText(r.symbol),
      detail: [persianText(r.name), r.instrumentClass ? className(r.instrumentClass) : ''].filter(Boolean).join(t('sep')),
      icon: 'symbol',
      keywords: '',
      run: () => navigate(symbolHref(r)),
    }));
    return [...symbolItems, ...pages().filter(match), ...commands().filter(match)];
  });

  createEffect(on(items, () => setActive(0)));

  createEffect(
    on(query, (value) => {
      clearTimeout(timer);
      controller?.abort();
      const q = value.trim();
      if (!q) {
        setSymbols([]);
        return;
      }
      timer = setTimeout(() => {
        const current = new AbortController();
        controller = current;
        market.search(normalizeSearch(q), 8, { signal: current.signal }).then(setSymbols, (e: unknown) => {
          if (!isAbort(e)) setSymbols([]);
        });
      }, 100);
    }),
  );

  const runItem = (item: PaletteItem | undefined) => {
    if (!item) return;
    close();
    item.run();
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const count = items().length;
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (count) setActive((active() + 1) % count);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (count) setActive((active() - 1 + count) % count);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      runItem(items()[active()]);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      close();
    } else if (event.key === 'Tab') {
      event.preventDefault(); // focus stays in the dialog
    }
  };

  createEffect(() => {
    const index = active();
    if (!paletteOpen()) return;
    list?.querySelector<HTMLElement>(`[data-index="${index}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  onMount(() => {
    const onGlobalKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && (event.key === 'k' || event.key === 'K')) {
        event.preventDefault();
        if (paletteOpen()) close();
        else {
          restoreFocus = document.activeElement as HTMLElement | null;
          setPaletteOpen(true);
        }
      }
    };
    window.addEventListener('keydown', onGlobalKey);
    onCleanup(() => window.removeEventListener('keydown', onGlobalKey));
  });

  createEffect(() => {
    if (paletteOpen()) queueMicrotask(() => input?.focus());
  });

  onCleanup(() => {
    clearTimeout(timer);
    controller?.abort();
  });

  const groupLabel = (group: PaletteItem['group']) => (group === 'symbols' ? t('paletteSymbols') : group === 'pages' ? t('palettePages') : t('paletteCommands'));

  return (
    <Show when={paletteOpen()}>
      <Portal>
        <div class="palette-backdrop" onMouseDown={close} />
        <div class="palette" role="dialog" aria-modal="true" aria-label={t('paletteOpen')} dir={document.documentElement.dir === "rtl" ? "rtl" : "ltr"}>
          <div class="palette-input">
            <Icon name="search" size={18} />
            <input
              ref={input}
              type="text"
              role="combobox"
              aria-expanded="true"
              aria-controls={listId}
              aria-activedescendant={items().length ? `${listId}-${active()}` : undefined}
              placeholder={t('palettePlaceholder')}
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
              onKeyDown={onKeyDown}
              spellcheck={false}
              autocomplete="off"
            />
            <kbd class="kbd">Esc</kbd>
          </div>
          <ul class="palette-list" id={listId} role="listbox" ref={list}>
            <For each={items()} fallback={<li class="palette-empty">{t('paletteEmpty')}</li>}>
              {(item, i) => (
                <>
                  <Show when={i() === 0 || items()[i() - 1].group !== item.group}>
                    <li class="palette-group" role="presentation">
                      {groupLabel(item.group)}
                    </li>
                  </Show>
                  <li
                    id={`${listId}-${i()}`}
                    data-index={i()}
                    role="option"
                    class="palette-item"
                    aria-selected={active() === i()}
                    onMouseMove={() => setActive(i())}
                    onMouseDown={(e) => {
                      e.preventDefault();
                      runItem(item);
                    }}
                  >
                    <Icon name={item.icon} size={16} />
                    <span class="palette-label">{item.label}</span>
                    <Show when={item.detail}>
                      <span class="palette-detail">{item.detail}</span>
                    </Show>
                  </li>
                </>
              )}
            </For>
          </ul>
          <div class="palette-hint">{t('paletteHint')}</div>
        </div>
      </Portal>
    </Show>
  );
}
