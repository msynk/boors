// /market — market overview: KPI tiles, TEDPIX 1-year (rial / dollar-indexed), 60-session breadth, sector returns and
// flows, the market map, and the movers tables. Charts are lazy chunks; every card has its own loading/error state.
import { A, useNavigate } from '@solidjs/router';
import { For, Show, Suspense, createMemo, createSignal, lazy, type JSX } from 'solid-js';
import { EQUAL_WEIGHT_INDEX, TEDPIX, market, type MoverDto, type MoverKind, type SectorPerformanceDto } from '../api';
import { KpiTile } from '../components/KpiTile';
import { Table, type TableColumn } from '../components/Table';
import { Card, Chg, EmptyState, ErrorState, Flow, Num, Segmented, Sep, Skeleton, StaleBanner, Tabs } from '../components/ui';
import * as f from '../format/format';
import { dayStatus, t } from '../i18n';
import { createQuery, type Query } from '../state/query';
import { settings } from '../state/settings';
import { usePageTitle } from '../state/title';
import { indexInDollars, tail } from './overview/series';

const IndexChart = lazy(() => import('../charts/IndexChart'));
const BreadthChart = lazy(() => import('../charts/OverviewCharts').then((m) => ({ default: m.BreadthChart })));
const SectorBars = lazy(() => import('../charts/OverviewCharts').then((m) => ({ default: m.SectorBars })));
const MarketTreemap = lazy(() => import('../charts/OverviewCharts').then((m) => ({ default: m.MarketTreemap })));

const [moverKind, setMoverKind] = createSignal<MoverKind>('value');
const [denomination, setDenomination] = createSignal<'rial' | 'usd'>('rial');
const [sectorMetric, setSectorMetric] = createSignal<'return' | 'flow'>('return');
const [sectorView, setSectorView] = createSignal<'chart' | 'table'>('chart');
const [breadthView, setBreadthView] = createSignal<'chart' | 'table'>('chart');

const IFB_INDEX = '43685683301327984';

/** Known indices get a translated caption; anything else shows the name the server sent. */
function indexName(insCode: string, name: string): string {
  if (insCode === TEDPIX) return t('tedpix');
  if (insCode === EQUAL_WEIGHT_INDEX) return t('equalWeightIndex');
  if (insCode === IFB_INDEX) return t('ifbIndex');
  return f.persianText(name);
}

/** A card body for a query: skeleton while loading, error with retry, empty state, or the content. */
function QueryBody<T>(props: { query: Query<T>; height?: string; empty?: (data: T) => boolean; emptyText?: string; children: (data: T) => JSX.Element }) {
  return (
    <Show when={!props.query.error()} fallback={<ErrorState error={props.query.error()!} onRetry={() => props.query.refetch(true)} compact />}>
      <Show when={props.query.data() !== undefined} fallback={<Skeleton height={props.height ?? '12rem'} />}>
        <Show when={!props.empty?.(props.query.data() as T)} fallback={<EmptyState>{props.emptyText ?? t('noRows')}</EmptyState>}>
          {props.children(props.query.data() as T)}
        </Show>
      </Show>
    </Show>
  );
}

export default function Overview() {
  const navigate = useNavigate();
  usePageTitle(t('overviewTitle'));
  const pro = () => settings().density === 'pro';

  const overview = createQuery(() => true, (_, signal, force) => market.getOverview(null, { signal, force }));
  const session = () => overview.data()?.session ?? null;
  const yearAgo = () => (session() ? f.addCalendar(session()!, -12) : null);
  const quarterAgo = () => (session() ? f.addCalendar(session()!, -3) : null);

  const breadth = createQuery(session, (_, signal, force) => market.getBreadth(60, { signal, force }));
  const sectors = createQuery(session, (s, signal, force) => market.getSectors(s, { signal, force }));
  const tedpix = createQuery(yearAgo, (from, signal, force) => market.getIndexHistory(TEDPIX, from, { signal, force }));
  const dollar = createQuery(yearAgo, (from, signal, force) => market.getMacroHistory('price_dollar_rl', from, { signal, force }));
  const coin = createQuery(quarterAgo, (from, signal, force) => market.getMacroHistory('sekee', from, { signal, force }));
  const equalWeight = createQuery(quarterAgo, (from, signal, force) => market.getIndexHistory(EQUAL_WEIGHT_INDEX, from, { signal, force }));
  const movers = createQuery(
    () => (session() ? { kind: moverKind(), session: session()! } : null),
    (key, signal, force) => market.getMovers(key.kind, 20, key.session, { signal, force }),
  );
  const watch = createQuery(
    () => (session() && pro() ? session() : null),
    (s, signal, force) =>
      market.getMarketWatch({ date: s, filter: null, classes: ['TseStock', 'IfbStock', 'IfbBaseYellow', 'IfbBaseOrange', 'IfbBaseRed'], tradedOnly: false }, { signal, force }),
  );

  const indexPoints = createMemo(() => {
    const index = tedpix.data() ?? [];
    if (denomination() === 'usd') return indexInDollars(index, dollar.data() ?? []);
    return index.map((p) => ({ date: p.date, value: p.value }));
  });
  const sectorNames = createMemo(() => new Map((sectors.data() ?? []).map((s) => [s.sectorCode, s.sectorName ?? s.sectorCode])));

  const sparkFor = (insCode: string) => (insCode === TEDPIX ? tail(tedpix.data(), 60) : insCode === EQUAL_WEIGHT_INDEX ? tail(equalWeight.data(), 60) : []);

  const moverColumns = (): TableColumn<MoverDto>[] => [
    { key: 'symbol', header: t('symbol'), cell: (m) => <A href={`/symbol/${m.insCode}`} class="sym">{f.persianText(m.symbol)}</A>, sort: { value: (m) => m.symbol, kind: 'text' } },
    { key: 'name', header: t('name'), class: 'ellipsis pro-only', cell: (m) => f.persianText(m.name) },
    { key: 'close', header: t('closingPrice'), numeric: true, cell: (m) => f.price(m.close), sort: { value: (m) => m.close, kind: 'number' } },
    { key: 'change', header: t('change'), numeric: true, cell: (m) => <Chg value={m.changePercent} />, sort: { value: (m) => m.changePercent, kind: 'number' } },
    { key: 'value', header: t('valueBillionToman'), numeric: true, cell: (m) => f.billionToman(m.value), sort: { value: (m) => m.value, kind: 'number' } },
    {
      key: 'flow',
      header: t('individualNetBillionToman'),
      numeric: true,
      cell: (m) => <Flow value={m.individualNetFlow} text={f.billionToman(m.individualNetFlow)} />,
      sort: { value: (m) => m.individualNetFlow, kind: 'number' },
    },
    { key: 'status', header: t('status'), class: 'pro-only', cell: (m) => dayStatus(m.dayStatus) },
  ];

  const sectorColumns = (): TableColumn<SectorPerformanceDto>[] => [
    { key: 'name', header: t('sector'), cell: (s) => f.persianText(s.sectorName) || s.sectorCode, sort: { value: (s) => s.sectorName, kind: 'text' } },
    { key: 'ret', header: t('sectorReturns'), numeric: true, cell: (s) => <Chg value={s.weightedReturn} fraction />, sort: { value: (s) => s.weightedReturn, kind: 'number' } },
    {
      key: 'flow',
      header: t('sectorFlow'),
      numeric: true,
      cell: (s) => <Flow value={s.individualNetFlow} text={f.billionToman(s.individualNetFlow)} />,
      sort: { value: (s) => s.individualNetFlow, kind: 'number' },
    },
    { key: 'value', header: t('valueBillionToman'), numeric: true, cell: (s) => f.billionToman(s.value), sort: { value: (s) => s.value, kind: 'number' } },
    { key: 'ad', header: `${t('advancers')} / ${t('decliners')}`, numeric: true, cell: (s) => `${f.integer(s.advancers)} / ${f.integer(s.decliners)}` },
  ];

  const viewToggle = (value: () => 'chart' | 'table', set: (v: 'chart' | 'table') => void) => (
    <Segmented
      size="sm"
      label={t('viewTable')}
      value={value()}
      onChange={set}
      options={[
        { value: 'chart', label: t('viewChart') },
        { value: 'table', label: t('viewTable') },
      ]}
    />
  );

  return (
    <div class="page overview-page">
      <header class="page-head">
        <h1 class="page-title">{t('overviewTitle')}</h1>
        <Show when={session()}>
          <span class="page-meta">
            {t('session')} <span class="num">{f.date(session())}</span>
          </span>
        </Show>
      </header>

      <Show when={!overview.error()} fallback={<ErrorState error={overview.error()!} onRetry={() => overview.refetch(true)} />}>
        <Show
          when={overview.settled()}
          fallback={
            <div class="kpi-grid">
              <For each={Array.from({ length: 8 })}>{() => <Skeleton height="6.5rem" />}</For>
            </div>
          }
        >
          <Show when={overview.data()} fallback={<EmptyState icon="info">{t('emptyMarket')}</EmptyState>}>
            {(o) => (
              <>
                <StaleBanner session={o().session} />
                <div class="kpi-grid">
                  <For each={o().indices}>
                    {(index) => (
                      <KpiTile
                        caption={indexName(index.insCode, index.name)}
                        value={<Num>{f.price(index.value)}</Num>}
                        change={<Chg value={index.changePercent} />}
                        spark={sparkFor(index.insCode)}
                        sparkTone={f.tone(index.changePercent)}
                      />
                    )}
                  </For>
                  <KpiTile
                    caption={t('tedpixUsd')}
                    value={<Chg value={o().tedpixUsdChangePercent} />}
                    note={t('tedpixUsdNote')}
                  />
                  <KpiTile
                    caption={t('equityValue')}
                    value={<Num>{f.hemat(o().equityValue)}</Num>}
                    note={
                      <>
                        {t('funds')}: <Num>{f.hemat(o().fundValue)}</Num>
                      </>
                    }
                    spark={(breadth.data() ?? []).slice(-30).map((d) => d.equityValue)}
                    sparkLabel={t('equityValue')}
                  />
                  <KpiTile
                    caption={t('breadth')}
                    value={
                      <span class="breadth-value">
                        <bdi dir="ltr" class="num chg up">▲ {f.integer(o().advancers)}</bdi>
                        <bdi dir="ltr" class="num chg down">▼ {f.integer(o().decliners)}</bdi>
                      </span>
                    }
                    note={
                      <>
                        {t('unchanged')} <Num>{f.integer(o().unchanged)}</Num> <Sep /> {t('notTraded')} <Num>{f.integer(o().notTraded)}</Num> <Sep /> {t('halted')}{' '}
                        <Num>{f.integer(o().halted)}</Num>
                      </>
                    }
                    spark={(breadth.data() ?? []).slice(-30).map((d) => d.advancers - d.decliners)}
                    sparkTone={o().advancers >= o().decliners ? 'up' : 'down'}
                    sparkLabel={`${t('advancers')} − ${t('decliners')}`}
                  />
                  <KpiTile
                    caption={t('buyQueues')}
                    value={<Num>{f.integer(o().buyQueues)}</Num>}
                    change={
                      <Show when={o().buyQueueValue !== null}>
                        <span class="chg up">
                          <Num>{f.hemat(o().buyQueueValue)}</Num> {t('hemat')}
                        </span>
                      </Show>
                    }
                  />
                  <KpiTile
                    caption={t('sellQueues')}
                    value={<Num>{f.integer(o().sellQueues)}</Num>}
                    change={
                      <Show when={o().sellQueueValue !== null}>
                        <span class="chg down">
                          <Num>{f.hemat(o().sellQueueValue)}</Num> {t('hemat')}
                        </span>
                      </Show>
                    }
                  />
                  <KpiTile
                    caption={t('individualNetFlow')}
                    value={<Flow value={o().individualNetFlow} text={f.signed(o().individualNetFlow / f.RIALS_PER_HEMAT, 2)} />}
                    change={
                      <span class="muted">
                        {t('perCapitaBuySell')}: <Num>{f.millionToman(o().individualPerCapitaBuy)}</Num> / <Num>{f.millionToman(o().individualPerCapitaSell)}</Num>
                      </span>
                    }
                    note={o().flowValuesEstimated ? t('estimatedNote') : undefined}
                  />
                  <Show when={o().usd}>
                    {(usd) => (
                      <KpiTile
                        caption={t('usdFree')}
                        value={<Num>{f.price(usd().value)}</Num>}
                        change={<Chg value={usd().changePercent} />}
                        note={<Num>{f.date(usd().date)}</Num>}
                        spark={tail(dollar.data(), 60)}
                        sparkTone={f.tone(usd().changePercent)}
                      />
                    )}
                  </Show>
                  <Show when={o().coin}>
                    {(c) => (
                      <KpiTile
                        caption={t('coinEmami')}
                        value={<Num>{f.price(c().value)}</Num>}
                        change={<Chg value={c().changePercent} />}
                        note={<Num>{f.date(c().date)}</Num>}
                        spark={tail(coin.data(), 60)}
                        sparkTone={f.tone(c().changePercent)}
                      />
                    )}
                  </Show>
                </div>

                <div class="overview-grid">
                  <Card
                    class="span-7"
                    title={t('indexYear')}
                    subtitle={denomination() === 'usd' ? t('indexUsdNote') : undefined}
                    actions={
                      <Segmented
                        size="sm"
                        label={t('indexYear')}
                        value={denomination()}
                        onChange={setDenomination}
                        options={[
                          { value: 'rial', label: t('inRial') },
                          { value: 'usd', label: t('inUsd') },
                        ]}
                      />
                    }
                  >
                    <QueryBody query={tedpix} height="300px" empty={(d) => d.length === 0}>
                      {() => (
                        <Show when={denomination() === 'rial' || !dollar.error()} fallback={<ErrorState error={dollar.error()!} onRetry={() => dollar.refetch(true)} compact />}>
                          <Suspense fallback={<Skeleton height="300px" />}>
                            <IndexChart points={indexPoints()} label={denomination() === 'usd' ? t('indexUsdLabel') : t('tedpix')} decimals={denomination() === 'usd' ? 1 : 0} height={300} />
                          </Suspense>
                        </Show>
                      )}
                    </QueryBody>
                  </Card>

                  <Card class="span-5" title={t('breadthTitle', 60)} subtitle={t('breadthNote')} actions={viewToggle(breadthView, setBreadthView)}>
                    <QueryBody query={breadth} height="280px" empty={(d) => d.length === 0}>
                      {(days) => (
                        <Show
                          when={breadthView() === 'chart'}
                          fallback={
                            <Table
                              rows={[...days].reverse()}
                              label={t('breadthTitle', days.length)}
                              maxHeight="280px"
                              columns={[
                                { key: 'd', header: t('date'), cell: (d) => <Num>{f.date(d.date)}</Num> },
                                { key: 'a', header: t('advancers'), numeric: true, cell: (d) => f.integer(d.advancers) },
                                { key: 'u', header: t('unchanged'), numeric: true, cell: (d) => f.integer(d.unchanged) },
                                { key: 'dd', header: t('decliners'), numeric: true, cell: (d) => f.integer(d.decliners) },
                                { key: 'n', header: t('notTraded'), numeric: true, cell: (d) => f.integer(d.notTraded + d.halted) },
                              ]}
                            />
                          }
                        >
                          <Suspense fallback={<Skeleton height="280px" />}>
                            <BreadthChart days={days} height={280} />
                          </Suspense>
                        </Show>
                      )}
                    </QueryBody>
                  </Card>

                  <Show when={pro()}>
                    <Card class="span-12" title={t('treemapTitle')} subtitle={t('treemapNote')}>
                      <QueryBody query={watch} height="480px" empty={(d) => d.entries.length === 0}>
                        {(result) => (
                          <>
                            <Suspense fallback={<Skeleton height="480px" />}>
                              <MarketTreemap entries={result.entries} sectorNames={sectorNames()} onOpen={(code) => navigate(`/symbol/${code}`)} height={480} />
                            </Suspense>
                            {/* LTR like the chart's own scale: −limit on the left, +limit on the right. */}
                            <div class="scale-legend" dir="ltr" aria-hidden="true">
                              <span>
                                − <bdi>{t('treemapLimit')}</bdi>
                              </span>
                              <span class="scale-bar" />
                              <span>
                                + <bdi>{t('treemapLimit')}</bdi>
                              </span>
                              <span class="scale-halted" />
                              <bdi>{t('treemapHaltedLegend')}</bdi>
                            </div>
                          </>
                        )}
                      </QueryBody>
                    </Card>
                  </Show>

                  <Card
                    class="span-5"
                    title={t('sectorsTitle')}
                    subtitle={sectorMetric() === 'return' ? t('sectorReturnsNote') : t('sectorFlowNote')}
                    actions={
                      <>
                        <Segmented
                          size="sm"
                          label={t('sectorsTitle')}
                          value={sectorMetric()}
                          onChange={setSectorMetric}
                          options={[
                            { value: 'return', label: t('sectorReturns') },
                            { value: 'flow', label: t('sectorFlow') },
                          ]}
                        />
                        {viewToggle(sectorView, setSectorView)}
                      </>
                    }
                  >
                    <QueryBody query={sectors} height="420px" empty={(d) => d.length === 0}>
                      {(list) => (
                        <Show when={sectorView() === 'chart'} fallback={<Table rows={list} columns={sectorColumns()} label={t('sectorsTitle')} maxHeight="520px" />}>
                          <div class="sector-scroll">
                            <Suspense fallback={<Skeleton height="420px" />}>
                              <SectorBars sectors={list} metric={sectorMetric()} />
                            </Suspense>
                          </div>
                        </Show>
                      )}
                    </QueryBody>
                  </Card>

                  <Card class="span-7" title={t('movers')}>
                    <Tabs
                      label={t('movers')}
                      value={moverKind()}
                      onChange={setMoverKind}
                      tabs={[
                        { value: 'gainers', label: t('gainers') },
                        { value: 'losers', label: t('losers') },
                        { value: 'value', label: t('topValue') },
                        { value: 'inflow', label: t('inflow') },
                        { value: 'outflow', label: t('outflow') },
                      ]}
                    />
                    <div role="tabpanel" class="tabpanel">
                      <QueryBody query={movers} height="420px" empty={(d) => d.length === 0}>
                        {(rows) => <Table rows={rows} columns={moverColumns()} label={t('movers')} onRowClick={(m) => navigate(`/symbol/${m.insCode}`)} />}
                      </QueryBody>
                    </div>
                  </Card>
                </div>
              </>
            )}
          </Show>
        </Show>
      </Show>
    </div>
  );
}
