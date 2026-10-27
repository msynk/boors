// /symbol/:insCode — the symbol workspace: header, key metrics, the price/value/flow chart and the events,
// shareholders, Codal letters and supervisor messages tabs.
import { useParams } from '@solidjs/router';
import { For, Show, Suspense, createEffect, createSignal, lazy, on } from 'solid-js';
import { instrument, type CodalLetterDto, type CorporateEventDto, type InstrumentMetricsDto, type ShareholderDto, type SupervisorMessageDto } from '../api';
import type { ChartRange } from '../charts/SymbolChart';
import { Icon } from '../components/Icon';
import { KpiTile } from '../components/KpiTile';
import { Table } from '../components/Table';
import { Card, Chg, EmptyState, ErrorState, Flow, Num, Segmented, Sep, Skeleton, StaleBanner, Tabs, Tag } from '../components/ui';
import * as f from '../format/format';
import { className, dayStatus, haltReason, messageKind, t } from '../i18n';
import { createQuery } from '../state/query';
import { rememberSymbol } from '../state/recent';
import { settings } from '../state/settings';
import { setPageTitle, usePageTitle } from '../state/title';

const SymbolChart = lazy(() => import('../charts/SymbolChart'));

const [range, setRange] = createSignal<ChartRange>('1y');
const [adjusted, setAdjusted] = createSignal(true);
type TabKey = 'events' | 'holders' | 'letters' | 'messages';
const [tab, setTab] = createSignal<TabKey>('letters');

function RangeBar(props: { low: number | null; high: number | null; close: number | null }) {
  const position = () => {
    const { low, high, close } = props;
    if (low === null || high === null || close === null || high <= low) return null;
    return Math.max(0, Math.min(100, ((close - low) / (high - low)) * 100));
  };
  return (
    <Show when={position() !== null}>
      <div class="range-bar" dir="ltr" role="img" aria-label={`${f.price(props.low)} – ${f.price(props.high)}`}>
        <span class="range-track" />
        <span class="range-marker" style={{ left: `${position()}%` }} />
      </div>
    </Show>
  );
}

function Header(props: { m: InstrumentMetricsDto }) {
  return (
    <header class="symbol-head">
      <div class="symbol-id">
        <h1 class="symbol-name">{f.persianText(props.m.symbol)}</h1>
        <span class="symbol-company">{f.persianText(props.m.name)}</span>
        <div class="symbol-tags">
          <Tag>{className(props.m.instrumentClass)}</Tag>
          <Show when={props.m.sectorName}>
            <Tag class="outline">{f.persianText(props.m.sectorName)}</Tag>
          </Show>
        </div>
      </div>
      <div class="symbol-price">
        <span class="symbol-close num">
          <Num>{f.price(props.m.close)}</Num>
        </span>
        <Chg value={props.m.changePercent} class="symbol-change" />
        <span class="symbol-asof">
          <Num>{f.date(props.m.asOf)}</Num> <Sep /> {dayStatus(props.m.dayStatus)}
        </span>
      </div>
    </header>
  );
}

function Metrics(props: { m: InstrumentMetricsDto }) {
  const m = () => props.m;
  const windows: { key: string; label: () => string }[] = [
    { key: '1w', label: () => t('week1') },
    { key: '1m', label: () => t('month1') },
    { key: '3m', label: () => t('months3') },
    { key: '6m', label: () => t('months6') },
    { key: '1y', label: () => t('year1') },
  ];
  // Any key the server adds beyond the known windows is shown too, after them.
  const extra = () => Object.keys(m().returns).filter((k) => !windows.some((w) => w.key === k));
  return (
    <div class="kpi-grid compact">
      <For each={[...windows, ...extra().map((k) => ({ key: k, label: () => k }))]}>
        {(w) => <KpiTile caption={w.label()} value={<Chg value={m().returns[w.key] ?? null} fraction />} />}
      </For>
      <KpiTile caption={t('usdReturn1y')} value={<Chg value={m().usdReturn1Year} fraction />} />
      <KpiTile
        small
        caption={t('range52')}
        value={
          <Num>
            {f.price(m().low52Week)} – {f.price(m().high52Week)}
          </Num>
        }
        note={
          <>
            <RangeBar low={m().low52Week} high={m().high52Week} close={m().close !== null && m().fromHigh52WeekPercent !== null && m().high52Week !== null ? m().high52Week! * (1 + m().fromHigh52WeekPercent! / 100) : null} />
            {t('fromHigh')}: <Chg value={m().fromHigh52WeekPercent} />
          </>
        }
      />
      <KpiTile caption={t('marketCap')} value={<Num>{f.hemat(m().marketCap)}</Num>} />
      <KpiTile caption={t('avgValue20')} value={<Num>{f.billionToman(m().averageValue20)}</Num>} />
      <KpiTile
        caption={t('flow20')}
        value={<Flow value={m().individualNetFlow20} text={f.signed((m().individualNetFlow20 ?? 0) / f.RIALS_PER_BILLION_TOMAN, 1)} />}
        change={
          <Show when={m().individualNetFlow20}>
            <span class={`chg ${f.flowTone(m().individualNetFlow20)}`}>{(m().individualNetFlow20 ?? 0) > 0 ? t('inflowWord') : t('outflowWord')}</span>
          </Show>
        }
      />
      <KpiTile
        caption={t('buyerPower')}
        value={<Num>{f.ratio(m().buyerPower)}</Num>}
        change={
          <span class="muted">
            {t('perCapitaBuySell')}: <Num>{f.millionToman(m().perCapitaBuy)}</Num> / <Num>{f.millionToman(m().perCapitaSell)}</Num>
          </span>
        }
        note={m().buyerPowerPercentile !== null ? t('buyerPowerPercentile', f.num((m().buyerPowerPercentile ?? 0) * 100, 0)) : undefined}
      />
    </div>
  );
}

function EventsTab(props: { rows: CorporateEventDto[] }) {
  return (
    <Table
      rows={props.rows}
      label={t('capitalEvents')}
      empty={t('noEvents')}
      columns={[
        { key: 'date', header: t('date'), cell: (e) => <Num>{f.date(e.date)}</Num>, sort: { value: (e) => e.date, kind: 'text' } },
        { key: 'kind', header: t('eventKind'), cell: (e) => (e.kind === 'share_change' ? t('shareChange') : t('priceAdjustment')) },
        { key: 'factor', header: t('factor'), numeric: true, cell: (e) => f.num(e.factor, 4) },
        { key: 'before', header: t('sharesBefore'), numeric: true, cell: (e) => f.integer(e.sharesBefore) },
        { key: 'after', header: t('sharesAfter'), numeric: true, cell: (e) => f.integer(e.sharesAfter) },
        { key: 'source', header: t('source'), cell: (e) => e.source ?? f.DASH },
      ]}
    />
  );
}

function HoldersTab(props: { rows: ShareholderDto[] }) {
  return (
    <Table
      rows={props.rows}
      label={t('shareholders')}
      empty={t('noShareholders')}
      columns={[
        { key: 'holder', header: t('holder'), class: 'wrap', cell: (s) => f.persianText(s.holderName) },
        { key: 'percent', header: t('percentCol'), numeric: true, cell: (s) => f.percent(s.percent, 2), sort: { value: (s) => s.percent, kind: 'number' } },
        { key: 'shares', header: t('shares'), numeric: true, cell: (s) => f.integer(s.shares), sort: { value: (s) => s.shares, kind: 'number' } },
        {
          key: 'change',
          header: t('change'),
          numeric: true,
          cell: (s) => (s.changeAmount === 0 ? f.DASH : <Flow value={s.changeAmount} text={f.signed(s.changeAmount)} />),
          sort: { value: (s) => s.changeAmount, kind: 'number' },
        },
        { key: 'asof', header: t('asOf'), cell: (s) => <Num>{f.date(s.snapshotDate)}</Num> },
      ]}
    />
  );
}

const safeUrl = (url: string | null) => (url && /^https?:\/\//i.test(url) ? url : null);

function LettersTab(props: { rows: CodalLetterDto[] }) {
  return (
    <Show when={props.rows.length > 0} fallback={<EmptyState>{t('noLetters')}</EmptyState>}>
      <ul class="feed">
        <For each={props.rows}>
          {(letter) => (
            <li class="feed-item">
              <div class="feed-meta">
                <Num>{letter.publishedAt ? f.dateTime(letter.publishedAt) : f.DASH}</Num>
                <Show when={letter.letterCode}>
                  <Tag>{letter.letterCode}</Tag>
                </Show>
                <Show when={letter.disclosureGroup}>
                  <span class="muted">{letter.disclosureGroup}</span>
                </Show>
              </div>
              <Show when={safeUrl(letter.reportUrl)} fallback={<span class="feed-title">{f.persianText(letter.title)}</span>}>
                {(url) => (
                  <a class="feed-title" href={url()} target="_blank" rel="noopener noreferrer" title={t('externalLink')}>
                    {f.persianText(letter.title)}
                    <Icon name="external" size={13} label={t('externalLink')} />
                  </a>
                )}
              </Show>
              <div class="feed-links">
                <Show when={letter.hasExcel && safeUrl(letter.excelUrl)}>
                  {(url) => (
                    <a href={url()} target="_blank" rel="noopener noreferrer">
                      {t('excel')}
                    </a>
                  )}
                </Show>
                <Show when={safeUrl(letter.pdfUrl)}>
                  {(url) => (
                    <a href={url()} target="_blank" rel="noopener noreferrer">
                      {t('pdf')}
                    </a>
                  )}
                </Show>
              </div>
            </li>
          )}
        </For>
      </ul>
    </Show>
  );
}

function MessagesTab(props: { rows: SupervisorMessageDto[] }) {
  return (
    <Show when={props.rows.length > 0} fallback={<EmptyState>{t('noMessages')}</EmptyState>}>
      <ul class="feed">
        <For each={props.rows}>
          {(m) => (
            <li class="feed-item">
              <div class="feed-meta">
                <Num>{f.date(m.date)}</Num>
                <Tag tone={m.kind.toLowerCase() === 'halt' ? 'down' : m.kind.toLowerCase() === 'reopen' ? 'up' : 'neutral'}>{messageKind(m.kind)}</Tag>
                <span class="muted">{haltReason(m.reason)}</span>
                <Show when={m.reopensWithoutLimit}>
                  <Tag>{t('reopensWithoutLimit')}</Tag>
                </Show>
              </div>
              <strong class="feed-title">{f.persianText(m.title)}</strong>
              <Show when={m.body}>
                <p class="feed-body">{f.persianText(m.body)}</p>
              </Show>
            </li>
          )}
        </For>
      </ul>
    </Show>
  );
}

export default function SymbolPage() {
  const params = useParams<{ insCode: string }>();
  const code = () => (/^\d{1,20}$/.test(params.insCode ?? '') ? params.insCode : null);
  usePageTitle(t('symbolTitle'));

  const metrics = createQuery(code, (c, signal, force) => instrument.getMetrics(c, { signal, force }));
  const bars = createQuery(code, (c, signal, force) => instrument.getBars(c, null, { signal, force }));
  const flows = createQuery(code, (c, signal, force) => instrument.getFlows(c, null, { signal, force }));
  const events = createQuery(code, (c, signal, force) => instrument.getEvents(c, { signal, force }));
  const holders = createQuery(code, (c, signal, force) => instrument.getShareholders(c, { signal, force }));
  const letters = createQuery(code, (c, signal, force) => instrument.getLetters(c, 50, { signal, force }));
  const messages = createQuery(code, (c, signal, force) => instrument.getMessages(c, 50, { signal, force }));

  createEffect(
    on(metrics.data, (m) => {
      if (!m) return;
      setPageTitle(`${f.persianText(m.symbol)}${t('sep')}${f.persianText(m.name)}`);
      rememberSymbol({ insCode: m.insCode, symbol: m.symbol, name: m.name });
    }),
  );

  const chartHeight = () => (settings().density === 'lite' ? 420 : 560);
  const traded = () => (bars.data() ?? []).some((b) => b.trades > 0);

  const ranges: { value: ChartRange; label: string }[] = [
    { value: '1m', label: '1M' },
    { value: '3m', label: '3M' },
    { value: '6m', label: '6M' },
    { value: '1y', label: '1Y' },
    { value: '3y', label: '3Y' },
    { value: '5y', label: '5Y' },
    { value: 'all', label: t('rangeAll') },
  ];

  const tabs = () => [
    { value: 'letters' as const, label: t('letters'), count: letters.data()?.length },
    { value: 'messages' as const, label: t('messages'), count: messages.data()?.length },
    { value: 'events' as const, label: t('capitalEvents'), count: events.data()?.length },
    { value: 'holders' as const, label: t('shareholders'), count: holders.data()?.length },
  ];

  const tabQuery = () => (tab() === 'events' ? events : tab() === 'holders' ? holders : tab() === 'letters' ? letters : messages);

  return (
    <div class="page symbol-page">
      <Show when={code()} fallback={<EmptyState icon="warning">{t('errorNotFound')}</EmptyState>}>
        <Show
          when={!metrics.error()}
          fallback={
            <Show when={metrics.error()!.status !== 404} fallback={<EmptyState icon="info">{t('noSymbolData')}</EmptyState>}>
              <ErrorState error={metrics.error()!} onRetry={() => metrics.refetch(true)} />
            </Show>
          }
        >
          <Show
            when={metrics.data()}
            fallback={
              <>
                <Skeleton height="4.5rem" />
                <div class="kpi-grid compact">
                  <For each={Array.from({ length: 6 })}>{() => <Skeleton height="5rem" />}</For>
                </div>
                <Skeleton height={`${chartHeight()}px`} />
              </>
            }
          >
            {(m) => (
              <>
                <Header m={m()} />
                <StaleBanner session={m().asOf} />
                <Show when={m().asOf} fallback={<EmptyState icon="info">{t('noSymbolData')}</EmptyState>}>
                  <Metrics m={m()} />
                </Show>

                <Card
                  class="chart-card"
                  actions={
                    <div class="chart-controls">
                      <Segmented size="sm" label={t('pricePane')} value={range()} onChange={setRange} options={ranges} />
                      <Segmented
                        size="sm"
                        label={t('adjustedHint')}
                        value={adjusted() ? 'adj' : 'raw'}
                        onChange={(v) => setAdjusted(v === 'adj')}
                        options={[
                          { value: 'adj', label: t('adjusted'), title: t('adjustedHint') },
                          { value: 'raw', label: t('raw') },
                        ]}
                      />
                    </div>
                  }
                  title={t('pricePane')}
                >
                  <Show when={!bars.error()} fallback={<ErrorState error={bars.error()!} onRetry={() => bars.refetch(true)} compact />}>
                    <Show when={bars.data() && flows.settled()} fallback={<Skeleton height={`${chartHeight()}px`} />}>
                      <Show when={traded()} fallback={<EmptyState>{t('noTrades')}</EmptyState>}>
                        <Suspense fallback={<Skeleton height={`${chartHeight()}px`} />}>
                          <SymbolChart
                            bars={bars.data()!}
                            flows={flows.data() ?? []}
                            events={events.data() ?? []}
                            messages={messages.data() ?? []}
                            adjusted={adjusted()}
                            range={range()}
                            height={chartHeight()}
                          />
                        </Suspense>
                      </Show>
                    </Show>
                  </Show>
                </Card>

                <Card>
                  <Tabs label={t('symbolTitle')} value={tab()} onChange={setTab} tabs={tabs()} idPrefix="symbol-tab" />
                  <div role="tabpanel" class="tabpanel" aria-labelledby={`symbol-tab-${tab()}`}>
                    <Show when={!tabQuery().error()} fallback={<ErrorState error={tabQuery().error()!} onRetry={() => tabQuery().refetch(true)} compact />}>
                      <Show when={tabQuery().data()} fallback={<Skeleton lines={6} />}>
                        <Show when={tab() === 'events'}>
                          <EventsTab rows={events.data() ?? []} />
                        </Show>
                        <Show when={tab() === 'holders'}>
                          <HoldersTab rows={holders.data() ?? []} />
                        </Show>
                        <Show when={tab() === 'letters'}>
                          <LettersTab rows={letters.data() ?? []} />
                        </Show>
                        <Show when={tab() === 'messages'}>
                          <MessagesTab rows={messages.data() ?? []} />
                        </Show>
                      </Show>
                    </Show>
                  </div>
                </Card>
              </>
            )}
          </Show>
        </Show>
      </Show>
    </div>
  );
}
