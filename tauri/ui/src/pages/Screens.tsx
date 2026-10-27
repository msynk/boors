// /market/screens — the four single-factor equity screens, each with its description and the "a rank is not a
// signal" note.
import { A, useNavigate } from '@solidjs/router';
import { Show, createSignal } from 'solid-js';
import { market, type ScreenKind, type ScreenRowDto } from '../api';
import { Icon } from '../components/Icon';
import { Table } from '../components/Table';
import { ErrorState, Skeleton, Tabs } from '../components/ui';
import * as f from '../format/format';
import { downloadText, toCsv } from '../grid/logic';
import { className, t, type MessageKey } from '../i18n';
import { createQuery } from '../state/query';
import { usePageTitle } from '../state/title';

const SCREENS: { kind: ScreenKind; title: MessageKey; description: MessageKey }[] = [
  { kind: 'Momentum6To1', title: 'screenMomentum', description: 'screenMomentumDesc' },
  { kind: 'High52WeekProximity', title: 'screenHigh52', description: 'screenHigh52Desc' },
  { kind: 'IndividualFlowIntensity20', title: 'screenFlow', description: 'screenFlowDesc' },
  { kind: 'BuyerPower5', title: 'screenBuyerPower', description: 'screenBuyerPowerDesc' },
];

const [kind, setKind] = createSignal<ScreenKind>('Momentum6To1');

/** Momentum and flow intensity are fractions, proximity is a ratio (1 = at the high), buyer power a plain ratio. */
export function formatScore(screen: ScreenKind, score: number): string {
  switch (screen) {
    case 'Momentum6To1':
    case 'IndividualFlowIntensity20':
      return f.fractionChangeText(score);
    case 'High52WeekProximity':
      return f.percent(score * 100, 1);
    default:
      return f.ratio(score);
  }
}

export default function Screens() {
  const navigate = useNavigate();
  usePageTitle(t('screensTitle'));
  const rows = createQuery(kind, (k, signal, force) => market.getScreen(k, 50, { signal, force }));
  const current = () => SCREENS.find((s) => s.kind === kind())!;
  const scoreTone = (score: number) => (kind() === 'Momentum6To1' || kind() === 'IndividualFlowIntensity20' ? f.fractionTone(score) : undefined);

  const exportCsv = () => {
    const list = rows.data() ?? [];
    downloadText(
      `screen-${kind()}.csv`,
      toCsv(
        list,
        list.map((_, i) => i),
        [
          { header: 'rank', value: (r: ScreenRowDto) => list.indexOf(r) + 1 },
          { header: 'insCode', value: (r) => r.insCode },
          { header: t('symbol'), value: (r) => r.symbol },
          { header: t('name'), value: (r) => r.name },
          { header: t('class'), value: (r) => r.instrumentClass },
          { header: t('score'), value: (r) => r.score },
          { header: t('observations'), value: (r) => r.observations },
        ],
      ),
    );
  };

  return (
    <div class="page screens-page">
      <header class="page-head">
        <h1 class="page-title">{t('screensTitle')}</h1>
      </header>
      <section class="card">
        <Tabs
          label={t('screensTitle')}
          value={kind()}
          onChange={setKind}
          tabs={SCREENS.map((s) => ({ value: s.kind, label: t(s.title) }))}
        />
        <div class="screen-desc">
          <p>{t(current().description)}</p>
          <button type="button" class="btn sm" onClick={exportCsv} disabled={!rows.data()?.length}>
            <Icon name="download" size={15} />
            {t('exportCsv')}
          </button>
        </div>
        <div role="tabpanel" class="tabpanel">
          <Show when={!rows.error()} fallback={<ErrorState error={rows.error()!} onRetry={() => rows.refetch(true)} />}>
            <Show when={rows.data()} fallback={<Skeleton lines={10} />}>
              {(list) => (
                <Table
                  rows={list()}
                  label={t(current().title)}
                  empty={t('noRows')}
                  onRowClick={(r) => navigate(`/symbol/${r.insCode}`)}
                  columns={[
                    { key: 'rank', header: '#', numeric: true, width: '3.5rem', cell: (_, i) => f.integer(i + 1) },
                    { key: 'symbol', header: t('symbol'), cell: (r) => <A href={`/symbol/${r.insCode}`} class="sym">{f.persianText(r.symbol)}</A> },
                    { key: 'name', header: t('name'), class: 'ellipsis', cell: (r) => f.persianText(r.name) },
                    { key: 'class', header: t('class'), class: 'pro-only', cell: (r) => className(r.instrumentClass) },
                    {
                      key: 'score',
                      header: t('score'),
                      numeric: true,
                      cell: (r) => (
                        <bdi dir="ltr" class={`num ${scoreTone(r.score) ? `chg ${scoreTone(r.score)}` : ''}`}>
                          {formatScore(kind(), r.score)}
                        </bdi>
                      ),
                    },
                    { key: 'obs', header: t('observations'), numeric: true, cell: (r) => f.integer(r.observations) },
                  ]}
                />
              )}
            </Show>
          </Show>
        </div>
      </section>
      <p class="note" role="note">
        <Icon name="info" size={16} />
        {t('screensNote')}
      </p>
    </div>
  );
}
