// /market/watch — the market watch (دیده‌بان): TSETMC filter box, class groups, traded-only switch and a virtualized,
// sortable grid of ~3,100 instruments with a frozen symbol column, quick search and CSV export.
import { useNavigate } from '@solidjs/router';
import { For, Show, createMemo, createSignal } from 'solid-js';
import { market, ProblemError, type MarketWatchEntryDto } from '../api';
import { Icon } from '../components/Icon';
import { ErrorState, Segmented, Sep, Skeleton, StaleBanner, Switch } from '../components/ui';
import * as f from '../format/format';
import { downloadText, filterIndices, identityOrder, sortByKeys, sortIndices, textRanks, toCsv } from '../grid/logic';
import { VirtualGrid, type GridColumn } from '../grid/VirtualGrid';
import { t, type MessageKey } from '../i18n';
import { createQuery } from '../state/query';
import { settings } from '../state/settings';
import { usePageTitle } from '../state/title';
import { csvColumns, watchColumns } from './watch/columns';
import {
  CLASS_GROUPS,
  PRESETS,
  appliedFilter,
  draftFilter,
  group,
  quick,
  setAppliedFilter,
  setDraftFilter,
  setGroup,
  setQuick,
  setSort,
  setTradedOnly,
  sort,
  tradedOnly,
  type ClassGroup,
} from './watch/state';

const collator = new Intl.Collator('fa');

/** Exposed for the perf check: duration of the last sort in ms. */
declare global {
  interface Window {
    __boorsGrid?: { lastSortMs: number; rows: number };
  }
}

export default function Watch() {
  const navigate = useNavigate();
  usePageTitle(t('watchTitle'));
  const columns = createMemo(() => {
    const all = watchColumns();
    return settings().density === 'lite' ? all.filter((c) => !c.pro) : all;
  });
  const rowHeight = () => (settings().density === 'lite' ? 36 : 28);

  const query = createQuery(
    () => ({ group: group(), tradedOnly: tradedOnly(), filter: appliedFilter() }),
    (key, signal, force) =>
      market.getMarketWatch(
        { date: null, filter: key.filter.trim() ? key.filter : null, classes: CLASS_GROUPS[key.group], tradedOnly: key.tradedOnly },
        { signal, force },
      ),
  );

  // A 400 for the filter is a user mistake, shown next to the box; anything else is a page error.
  const syntaxError = () => {
    const e = query.error();
    return e && e.status === 400 && appliedFilter().trim() ? e : null;
  };
  const pageError = () => {
    const e = query.error();
    return e && !syntaxError() ? e : null;
  };

  const rows = createMemo<MarketWatchEntryDto[]>(() => (query.error() ? [] : query.data()?.entries ?? []));
  const searchKeys = createMemo(() => rows().map((e) => f.normalizeSearch(`${e.symbol} ${e.name ?? ''}`)));
  // Text columns are ranked once per dataset (collation is the slow part); every later sort is numeric.
  const ranks = createMemo(() => {
    rows();
    return new Map<string, Float64Array>();
  });
  const sorted = createMemo(() => {
    const list = rows();
    const spec = sort();
    const column = spec ? columns().find((c) => c.key === spec.key) : undefined;
    if (!spec || !column?.sort) return identityOrder(list.length);
    const started = performance.now();
    let order: number[];
    if (column.sort.kind === 'text') {
      let keys = ranks().get(column.key);
      if (!keys) {
        keys = textRanks(list, column.sort.value, collator);
        ranks().set(column.key, keys);
      }
      order = sortByKeys(keys, spec.dir);
    } else {
      order = sortIndices(list, column.sort.value, spec.dir, column.sort.kind, collator);
    }
    window.__boorsGrid = { lastSortMs: performance.now() - started, rows: list.length };
    return order;
  });
  const visible = createMemo(() => filterIndices(sorted(), searchKeys(), f.normalizeSearch(quick())));

  const onSort = (column: GridColumn<MarketWatchEntryDto>) => {
    const current = sort();
    if (current?.key === column.key) setSort({ key: column.key, dir: current.dir === 'asc' ? 'desc' : 'asc' });
    else setSort({ key: column.key, dir: column.sort?.firstDir ?? 'desc' });
  };

  const apply = () => {
    const text = draftFilter();
    if (text === appliedFilter()) query.refetch(true);
    else setAppliedFilter(text);
  };
  const clear = () => {
    setDraftFilter('');
    setAppliedFilter('');
  };

  const exportCsv = () => {
    const session = query.data()?.session ?? 'latest';
    downloadText(`market-watch-${session}.csv`, toCsv(rows(), visible(), csvColumns(columns())));
  };

  const groups = (): { value: ClassGroup; label: string }[] => [
    { value: 'shares', label: t('groupShares') },
    { value: 'funds', label: t('groupFunds') },
    { value: 'rights', label: t('groupRights') },
    { value: 'all', label: t('groupAll') },
  ];

  const [showAllErrors, setShowAllErrors] = createSignal(false);
  const filterErrors = () => query.data()?.filterErrors ?? [];

  return (
    <div class="page watch-page">
      <header class="page-head">
        <h1 class="page-title">{t('watchTitle')}</h1>
        <Show when={query.data()}>
          {(d) => (
            <span class="page-meta">
              {t('session')} <span class="num">{f.date(d().session)}</span> <Sep /> <span class="num">{t('evaluated', f.integer(d().entries.length), f.integer(d().evaluated))}</span>
            </span>
          )}
        </Show>
      </header>
      <StaleBanner session={query.data()?.session} />

      <section class="card watch-controls" aria-label={t('filterLabel')}>
        <div class="watch-row">
          <Segmented options={groups()} value={group()} onChange={setGroup} label={t('class')} />
          <Switch checked={tradedOnly()} onChange={setTradedOnly} label={t('tradedOnly')} />
          <div class="grow" />
          <label class="select-wrap">
            <span class="sr-only">{t('exampleFilters')}</span>
            <select
              class="select"
              onChange={(e) => {
                const preset = PRESETS.find((p) => p.key === e.currentTarget.value);
                if (preset) {
                  setDraftFilter(preset.value);
                  setAppliedFilter(preset.value);
                }
                e.currentTarget.value = '';
              }}
            >
              <option value="">{t('exampleFilters')}</option>
              <For each={PRESETS}>{(p) => <option value={p.key}>{t(p.key as MessageKey)}</option>}</For>
            </select>
          </label>
        </div>
        <div class="filter-box">
          <label class="field-label" for="watch-filter">
            {t('filterLabel')}
          </label>
          <textarea
            id="watch-filter"
            class={`filter-input ${syntaxError() ? 'invalid' : ''}`}
            dir="ltr"
            rows={2}
            spellcheck={false}
            autocomplete="off"
            placeholder="(pc)>(py)*1.03 && (ct).Buy_I_Volume>(ct).Sell_I_Volume"
            value={draftFilter()}
            aria-invalid={syntaxError() ? 'true' : undefined}
            aria-describedby="watch-filter-help watch-filter-error"
            onInput={(e) => setDraftFilter(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                apply();
              }
            }}
          />
          <div class="filter-actions">
            <button type="button" class="btn primary" onClick={apply}>
              <Icon name="filter" size={15} />
              {t('apply')}
            </button>
            <button type="button" class="btn ghost" onClick={clear} disabled={!draftFilter() && !appliedFilter()}>
              <Icon name="close" size={15} />
              {t('clear')}
            </button>
            <Show when={query.loading()}>
              <span class="spinner" role="status" aria-label={t('loading')} />
            </Show>
            <p class="field-help" id="watch-filter-help">
              {t('filterHelp')} <code dir="ltr">true==function(){'{'}…{'}'}()</code>. {t('filterHelpApply')}
            </p>
          </div>
          <div id="watch-filter-error" aria-live="polite">
            <Show when={syntaxError()}>
              {(e) => (
                <div class="filter-error" role="alert">
                  <Icon name="error" size={16} />
                  <strong>{e().title || t('filterSyntax')}</strong>
                  <Show when={e().detail}>
                    <code dir="ltr">{e().detail}</code>
                  </Show>
                </div>
              )}
            </Show>
          </div>
          <Show when={filterErrors().length > 0}>
            <div class="filter-warnings" role="status">
              <div class="filter-warnings-title">
                <Icon name="warning" size={16} />
                {t('filterErrorsTitle')}
              </div>
              <ul dir="ltr">
                <For each={showAllErrors() ? filterErrors() : filterErrors().slice(0, 6)}>{(error) => <li>{error}</li>}</For>
              </ul>
              <Show when={!showAllErrors() && filterErrors().length > 6}>
                <button type="button" class="link-btn" onClick={() => setShowAllErrors(true)}>
                  {t('filterErrorsMore', f.integer(filterErrors().length - 6))}
                </button>
              </Show>
            </div>
          </Show>
        </div>
      </section>

      <div class="grid-toolbar">
        <label class="quick-search">
          <Icon name="search" size={15} />
          <span class="sr-only">{t('quickSearch')}</span>
          <input type="search" placeholder={t('quickSearchPlaceholder')} value={quick()} onInput={(e) => setQuick(e.currentTarget.value)} />
        </label>
        <span class="muted num" aria-live="polite">
          {t('rows', f.integer(visible().length))}
        </span>
        <div class="grow" />
        <button type="button" class="btn sm" onClick={exportCsv} disabled={visible().length === 0}>
          <Icon name="download" size={15} />
          {t('exportCsv')}
        </button>
      </div>

      <div class="grid-wrap">
        <Show when={!pageError()} fallback={<ErrorState error={pageError() as ProblemError} onRetry={() => query.refetch(true)} />}>
          <Show when={query.settled() || query.data()} fallback={<Skeleton lines={12} class="grid-skeleton" />}>
            <VirtualGrid
              rows={rows()}
              order={visible()}
              columns={columns()}
              rowHeight={rowHeight()}
              height="100%"
              sort={sort()}
              onSort={onSort}
              onOpen={(e) => navigate(`/symbol/${e.insCode}`)}
              label={t('watchTitle')}
              help={t('gridHelp')}
              empty={<div class="state empty">{syntaxError() ? t('filterSyntax') : t('noRows')}</div>}
            />
          </Show>
        </Show>
      </div>
    </div>
  );
}
