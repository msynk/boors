// Market-watch columns: display text, tone, sort key and CSV export value for each field of MarketWatchEntryDto.
import type { MarketWatchEntryDto } from '../../api';
import * as f from '../../format/format';
import { className, dayStatus, t } from '../../i18n';
import type { GridColumn } from '../../grid/VirtualGrid';
import type { CsvColumn } from '../../grid/logic';

type E = MarketWatchEntryDto;

/** Where the close sits in the allowed band, 0 (floor) … 100 (ceiling). */
export function rangePosition(e: E): number | null {
  if (e.allowedLow === null || e.allowedHigh === null || e.allowedHigh <= e.allowedLow || e.close <= 0) return null;
  return ((e.close - e.allowedLow) / (e.allowedHigh - e.allowedLow)) * 100;
}

export interface WatchColumn extends GridColumn<E> {
  csv: (e: E) => string | number | null;
  pro?: boolean;
}

export function watchColumns(): WatchColumn[] {
  return [
    {
      key: 'symbol',
      header: t('symbol'),
      width: 96,
      frozen: true,
      text: (e) => f.persianText(e.symbol),
      href: (e) => `/symbol/${e.insCode}`,
      sort: { value: (e) => f.persianText(e.symbol), kind: 'text', firstDir: 'asc' },
      csv: (e) => e.symbol,
    },
    {
      key: 'name',
      header: t('name'),
      width: 190,
      text: (e) => f.persianText(e.name),
      sort: { value: (e) => f.persianText(e.name), kind: 'text', firstDir: 'asc' },
      csv: (e) => e.name,
    },
    {
      key: 'last',
      header: t('last'),
      width: 92,
      numeric: true,
      text: (e) => f.price(e.last),
      sort: { value: (e) => e.last, kind: 'number' },
      csv: (e) => e.last,
    },
    {
      key: 'lastChange',
      header: t('lastPercent'),
      width: 96,
      numeric: true,
      text: (e) => f.changeText(e.lastChangePercent),
      tone: (e) => f.tone(e.lastChangePercent),
      sort: { value: (e) => e.lastChangePercent, kind: 'number' },
      csv: (e) => e.lastChangePercent,
    },
    {
      key: 'close',
      header: t('closingPrice'),
      width: 96,
      numeric: true,
      text: (e) => f.price(e.close),
      sort: { value: (e) => e.close, kind: 'number' },
      csv: (e) => e.close,
    },
    {
      key: 'change',
      header: t('closePercent'),
      width: 96,
      numeric: true,
      text: (e) => f.changeText(e.changePercent),
      tone: (e) => f.tone(e.changePercent),
      sort: { value: (e) => e.changePercent, kind: 'number' },
      csv: (e) => e.changePercent,
    },
    {
      key: 'value',
      header: t('valueBillionToman'),
      width: 132,
      numeric: true,
      text: (e) => f.billionToman(e.value),
      sort: { value: (e) => e.value, kind: 'number' },
      csv: (e) => e.value,
    },
    {
      key: 'volume',
      header: t('volume'),
      width: 116,
      numeric: true,
      text: (e) => f.integer(e.volume),
      sort: { value: (e) => e.volume, kind: 'number' },
      csv: (e) => e.volume,
    },
    {
      key: 'trades',
      header: t('trades'),
      width: 76,
      numeric: true,
      text: (e) => f.integer(e.trades),
      sort: { value: (e) => e.trades, kind: 'number' },
      csv: (e) => e.trades,
    },
    {
      key: 'queue',
      header: t('queue'),
      width: 116,
      text: (e) => (e.queue === 'buy' ? `${t('buy')} ${f.billionToman(e.queueValue)}` : e.queue === 'sell' ? `${t('sell')} ${f.billionToman(e.queueValue)}` : ''),
      // A text tag plus colour: the queue side never rests on colour alone.
      render: (e) =>
        e.queue === 'buy' || e.queue === 'sell' ? (
          <span class={`tag ${e.queue === 'buy' ? 'up' : 'down'}`}>
            {e.queue === 'buy' ? t('buy') : t('sell')} <bdi dir="ltr" class="num">{f.billionToman(e.queueValue)}</bdi>
          </span>
        ) : null,
      sort: { value: (e) => (e.queue === 'buy' ? (e.queueValue ?? 0) : e.queue === 'sell' ? -(e.queueValue ?? 0) : null), kind: 'number' },
      csv: (e) => (e.queue ? `${e.queue} ${e.queueValue ?? ''}`.trim() : null),
    },
    {
      key: 'individualNet',
      header: t('individualNetBillionToman'),
      width: 150,
      numeric: true,
      text: (e) => f.billionToman(e.individualNetValue),
      tone: (e) => f.flowTone(e.individualNetValue),
      sort: { value: (e) => e.individualNetValue, kind: 'number' },
      csv: (e) => e.individualNetValue,
    },
    {
      key: 'buyerPower',
      header: t('buyerPower'),
      width: 96,
      numeric: true,
      text: (e) => f.ratio(e.buyerPower),
      sort: { value: (e) => e.buyerPower, kind: 'number' },
      csv: (e) => e.buyerPower,
    },
    {
      key: 'perCapitaBuy',
      header: t('perCapitaBuy'),
      width: 150,
      numeric: true,
      pro: true,
      text: (e) => f.millionToman(e.perCapitaBuy),
      sort: { value: (e) => e.perCapitaBuy, kind: 'number' },
      csv: (e) => e.perCapitaBuy,
    },
    {
      key: 'perCapitaSell',
      header: t('perCapitaSell'),
      width: 150,
      numeric: true,
      pro: true,
      text: (e) => f.millionToman(e.perCapitaSell),
      sort: { value: (e) => e.perCapitaSell, kind: 'number' },
      csv: (e) => e.perCapitaSell,
    },
    {
      key: 'individualBuyShare',
      header: t('individualBuyShare'),
      width: 110,
      numeric: true,
      pro: true,
      text: (e) => (e.individualBuyShare === null ? f.DASH : f.percent(e.individualBuyShare * 100, 0)),
      sort: { value: (e) => e.individualBuyShare, kind: 'number' },
      csv: (e) => e.individualBuyShare,
    },
    {
      key: 'pe',
      header: t('pe'),
      width: 70,
      numeric: true,
      text: (e) => f.num(e.priceEarnings, 1),
      sort: { value: (e) => e.priceEarnings, kind: 'number' },
      csv: (e) => e.priceEarnings,
    },
    {
      key: 'marketCap',
      header: t('marketCapHemat'),
      width: 122,
      numeric: true,
      text: (e) => f.hemat(e.marketCap),
      sort: { value: (e) => e.marketCap, kind: 'number' },
      csv: (e) => e.marketCap,
    },
    {
      key: 'rangePosition',
      header: t('rangePosition'),
      width: 104,
      numeric: true,
      pro: true,
      text: (e) => f.percent(rangePosition(e), 0),
      sort: { value: rangePosition, kind: 'number' },
      csv: (e) => rangePosition(e),
    },
    {
      key: 'allowedRange',
      header: t('allowedRange'),
      width: 140,
      numeric: true,
      pro: true,
      text: (e) => (e.allowedLow === null || e.allowedHigh === null ? f.DASH : `${f.price(e.allowedLow)} – ${f.price(e.allowedHigh)}`),
      sort: { value: (e) => e.allowedHigh, kind: 'number' },
      csv: (e) => (e.allowedLow === null ? null : `${e.allowedLow}-${e.allowedHigh}`),
    },
    {
      key: 'class',
      header: t('class'),
      width: 116,
      text: (e) => className(e.instrumentClass),
      sort: { value: (e) => className(e.instrumentClass), kind: 'text', firstDir: 'asc' },
      csv: (e) => e.instrumentClass,
    },
    {
      key: 'status',
      header: t('status'),
      width: 128,
      text: (e) => dayStatus(e.dayStatus),
      sort: { value: (e) => e.dayStatus, kind: 'text', firstDir: 'asc' },
      csv: (e) => e.dayStatus,
    },
  ];
}

export function csvColumns(columns: WatchColumn[]): CsvColumn<E>[] {
  return [{ header: 'insCode', value: (e) => e.insCode }, ...columns.map((c) => ({ header: c.header, value: c.csv }))];
}
