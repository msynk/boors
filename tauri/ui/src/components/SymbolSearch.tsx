// Find an instrument by symbol or name and open its workspace. Suggestions come from Market/Search; Enter opens the
// highlighted suggestion, or the best match for the typed text. "/" focuses the box from anywhere.
import { useNavigate } from '@solidjs/router';
import { For, Show, createSignal, createUniqueId, onCleanup, onMount } from 'solid-js';
import { isAbort, market, type InstrumentSearchResultDto } from '../api';
import { normalizeSearch, persianText } from '../format/format';
import { className, t } from '../i18n';
import { Icon } from './Icon';

export const symbolHref = (r: { insCode: string; parentInsCode?: string | null }) => `/symbol/${r.parentInsCode ?? r.insCode}`;

export function SymbolSearch(props: { class?: string; autofocusKey?: boolean }) {
  const navigate = useNavigate();
  const listId = createUniqueId();
  const [text, setText] = createSignal('');
  const [items, setItems] = createSignal<InstrumentSearchResultDto[]>([]);
  const [open, setOpen] = createSignal(false);
  const [active, setActive] = createSignal(-1);
  const [searched, setSearched] = createSignal(false);
  let input!: HTMLInputElement;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let controller: AbortController | undefined;

  const load = (value: string) => {
    clearTimeout(timer);
    controller?.abort();
    const query = value.trim();
    if (!query) {
      setItems([]);
      setOpen(false);
      setSearched(false);
      return;
    }
    timer = setTimeout(() => {
      const current = new AbortController();
      controller = current;
      market.search(normalizeSearch(query), 10, { signal: current.signal }).then(
        (results) => {
          setItems(results);
          setActive(results.length ? 0 : -1);
          setSearched(true);
          setOpen(true);
        },
        (error: unknown) => {
          if (!isAbort(error)) {
            setItems([]);
            setSearched(true);
          }
        },
      );
    }, 140);
  };

  const go = (result: InstrumentSearchResultDto) => {
    setOpen(false);
    setText('');
    setItems([]);
    input.blur();
    navigate(symbolHref(result));
  };

  const submit = async () => {
    const picked = items()[active()];
    if (picked) return go(picked);
    const query = text().trim();
    if (!query) return;
    try {
      const [best] = await market.search(normalizeSearch(query), 1);
      if (best) go(best);
    } catch {
      // The suggestions box already shows the failure state.
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const count = items().length;
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setOpen(true);
      if (count) setActive((active() + 1) % count);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (count) setActive((active() - 1 + count) % count);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      void submit();
    } else if (event.key === 'Escape') {
      if (open()) {
        event.stopPropagation();
        setOpen(false);
      } else {
        input.blur();
      }
    }
  };

  onMount(() => {
    if (!props.autofocusKey) return;
    const onGlobalKey = (event: KeyboardEvent) => {
      if (event.key !== '/' || event.ctrlKey || event.metaKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return;
      event.preventDefault();
      input.focus();
    };
    window.addEventListener('keydown', onGlobalKey);
    onCleanup(() => window.removeEventListener('keydown', onGlobalKey));
  });

  onCleanup(() => {
    clearTimeout(timer);
    controller?.abort();
  });

  return (
    <div class={`symbol-search ${props.class ?? ''}`}>
      <Icon name="search" size={16} class="symbol-search-icon" />
      <input
        ref={input}
        type="search"
        role="combobox"
        aria-label={t('searchLabel')}
        aria-expanded={open()}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={open() && active() >= 0 ? `${listId}-${active()}` : undefined}
        placeholder={t('searchPlaceholder')}
        autocomplete="off"
        spellcheck={false}
        value={text()}
        onInput={(e) => {
          setText(e.currentTarget.value);
          load(e.currentTarget.value);
        }}
        onKeyDown={onKeyDown}
        onFocus={() => items().length && setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 120)}
      />
      <kbd class="kbd symbol-search-kbd" aria-hidden="true">
        /
      </kbd>
      <Show when={open()}>
        <ul class="suggestions" id={listId} role="listbox" aria-label={t('searchLabel')}>
          <For each={items()} fallback={<Show when={searched()}><li class="suggestion empty">{t('searchNoResults')}</li></Show>}>
            {(item, i) => (
              <li
                id={`${listId}-${i()}`}
                role="option"
                class="suggestion"
                aria-selected={active() === i()}
                onMouseDown={(e) => {
                  e.preventDefault();
                  go(item);
                }}
                onMouseEnter={() => setActive(i())}
              >
                <span class="suggestion-symbol">{persianText(item.symbol)}</span>
                <span class="suggestion-name">{persianText(item.name)}</span>
                <span class="suggestion-class">{className(item.instrumentClass)}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
}
