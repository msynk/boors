// The symbol workspace chart: candlesticks (raw or point-in-time adjusted), value traded and individual net flow, as
// three panes of one Lightweight Chart, so the time scale and crosshair are shared. Separate panes, never dual axes.
import {
  CandlestickSeries,
  HistogramSeries,
  LineSeries,
  LineStyle,
  createSeriesMarkers,
  createTextWatermark,
  type IChartApiBase,
  type IPriceLine,
  type ITextWatermarkPluginApi,
  type ISeriesApi,
  type ISeriesMarkersPluginApi,
  type MouseEventParams,
  type SeriesMarker,
  type Time,
} from 'lightweight-charts';
import { Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from 'solid-js';
import type { BarDto, CorporateEventDto, FlowDayDto, SupervisorMessageDto } from '../api';
import * as f from '../format/format';
import { t } from '../i18n';
import { createTimeChart, priceFormatter, themeOptions } from './lw';
import { readTheme, themeKey, type ChartTheme } from './theme';

export type ChartRange = '1m' | '3m' | '6m' | '1y' | '3y' | '5y' | 'all';

export const RANGE_MONTHS: Record<ChartRange, number | null> = { '1m': 1, '3m': 3, '6m': 6, '1y': 12, '3y': 36, '5y': 60, all: null };

interface Candle {
  time: string;
  open: number;
  high: number;
  low: number;
  close: number;
  /** The day's permitted band on the same (raw or adjusted) scale, when known. */
  bandLow: number | null;
  bandHigh: number | null;
  bar: BarDto;
}

/** Bars that traded, scaled by adjustedClose/close when adjusted (point-in-time adjusted OHLC). */
export function toCandles(bars: BarDto[], adjusted: boolean): Candle[] {
  const out: Candle[] = [];
  for (const b of bars) {
    if (b.trades <= 0 || b.close <= 0) continue;
    const k = adjusted && b.close ? b.adjustedClose / b.close : 1;
    const open = b.open > 0 ? b.open : b.close;
    const high = Math.max(b.high > 0 ? b.high : b.close, open, b.close);
    const low = Math.min(b.low > 0 ? b.low : b.close, open, b.close);
    const bandLow = b.allowedLow && b.allowedLow > 0 ? b.allowedLow * k : null;
    const bandHigh = b.allowedHigh && b.allowedHigh > 0 ? b.allowedHigh * k : null;
    out.push({ time: b.date, open: open * k, high: high * k, low: low * k, close: b.close * k, bandLow, bandHigh, bar: b });
  }
  return out;
}

/** One band edge as line points; days without a known band become whitespace, so the line breaks there. */
export function bandLine(candles: Candle[], edge: 'bandLow' | 'bandHigh'): ({ time: Time; value: number } | { time: Time })[] {
  return candles.map((c) => (c[edge] === null ? { time: c.time as Time } : { time: c.time as Time, value: c[edge] }));
}

/** The first candle on or after `date` (corporate actions mostly take effect on days without trades). */
function snap(times: string[], date: string): string | null {
  let lo = 0;
  let hi = times.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (times[mid] >= date) {
      found = mid;
      hi = mid - 1;
    } else lo = mid + 1;
  }
  return found >= 0 ? times[found] : null;
}

export function buildMarkers(candles: Candle[], events: CorporateEventDto[], messages: SupervisorMessageDto[], theme: ChartTheme): SeriesMarker<Time>[] {
  const times = candles.map((c) => c.time);
  const markers: SeriesMarker<Time>[] = [];
  for (const c of candles) {
    const s = c.bar.status;
    if (s === 'QueueLockedUp') markers.push({ time: c.time as Time, position: 'aboveBar', shape: 'arrowDown', color: theme.up, size: 0.6 });
    else if (s === 'QueueLockedDown') markers.push({ time: c.time as Time, position: 'belowBar', shape: 'arrowUp', color: theme.down, size: 0.6 });
    else if (s === 'ReopenAuction') markers.push({ time: c.time as Time, position: 'aboveBar', shape: 'square', color: theme.warning, size: 0.7 });
  }
  const marked = new Set<string>();
  const ordered = [...events].sort((a, b) => (a.kind === b.kind ? 0 : a.kind === 'adjustment' ? -1 : 1));
  for (const e of ordered) {
    const at = snap(times, e.date);
    if (!at || marked.has(at)) continue;
    marked.add(at);
    const text = e.kind === 'adjustment' && e.factor ? '×' + f.num(e.factor, 2) : e.sharesBefore && e.sharesAfter ? '+' + f.percent((e.sharesAfter / e.sharesBefore - 1) * 100, 0) : '';
    markers.push({ time: at as Time, position: 'belowBar', shape: 'circle', color: theme.event, text, size: 0.9 });
  }
  for (const m of messages) {
    if (m.kind.toLowerCase() !== 'halt') continue;
    const at = snap(times, m.date);
    if (at) markers.push({ time: at as Time, position: 'aboveBar', shape: 'square', color: theme.flat, size: 0.6 });
  }
  markers.sort((a, b) => (String(a.time) < String(b.time) ? -1 : String(a.time) > String(b.time) ? 1 : 0));
  return markers;
}

export default function SymbolChart(props: {
  bars: BarDto[];
  flows: FlowDayDto[];
  events: CorporateEventDto[];
  messages: SupervisorMessageDto[];
  adjusted: boolean;
  range: ChartRange;
  height: number;
}) {
  let host!: HTMLDivElement;
  let chart: IChartApiBase<Time> | undefined;
  let candles: ISeriesApi<'Candlestick', Time> | undefined;
  let bandHigh: ISeriesApi<'Line', Time> | undefined;
  let bandLow: ISeriesApi<'Line', Time> | undefined;
  // The latest session's band as horizontal price lines: visible even when only that one day has a band.
  let limitLines: { line: IPriceLine; tone: 'up' | 'down' }[] = [];
  let value: ISeriesApi<'Histogram', Time> | undefined;
  let flow: ISeriesApi<'Histogram', Time> | undefined;
  let markers: ISeriesMarkersPluginApi<Time> | undefined;
  const watermarks: ITextWatermarkPluginApi<Time>[] = [];
  const [hover, setHover] = createSignal<string | null>(null);

  const candleData = createMemo(() => toCandles(props.bars, props.adjusted));
  const flowByDate = createMemo(() => new Map(props.flows.map((d) => [d.date, d])));
  const candleByDate = createMemo(() => new Map(candleData().map((c) => [c.time, c])));
  const shownDate = () => hover() ?? candleData()[candleData().length - 1]?.time ?? null;
  const shown = () => {
    const d = shownDate();
    return d ? candleByDate().get(d) ?? null : null;
  };

  const decimals = () => {
    const last = candleData()[candleData().length - 1];
    return last && last.close < 100 ? 2 : 0;
  };

  const applyRange = () => {
    if (!chart) return;
    const data = candleData();
    if (data.length === 0) return;
    const months = RANGE_MONTHS[props.range];
    const lastIndex = data.length - 1;
    if (months === null) {
      chart.timeScale().setVisibleLogicalRange({ from: -1, to: lastIndex + 3 });
      return;
    }
    const from = f.addCalendar(data[lastIndex].time, -months);
    let index = data.findIndex((c) => c.time >= from);
    if (index < 0) index = 0;
    chart.timeScale().setVisibleLogicalRange({ from: index - 0.5, to: lastIndex + 3 });
  };

  const paint = (theme: ChartTheme) => {
    candles!.applyOptions({ upColor: theme.up, downColor: theme.down, wickUpColor: theme.up, wickDownColor: theme.down, borderVisible: false });
    bandHigh!.applyOptions({ color: theme.up });
    bandLow!.applyOptions({ color: theme.down });
    for (const l of limitLines) l.line.applyOptions({ color: l.tone === 'up' ? theme.up : theme.down });
    value!.applyOptions({ color: theme.volume });
    flow!.setData(
      props.flows.map((d) => ({ time: d.date as Time, value: d.individualNetValue / f.RIALS_PER_BILLION_TOMAN, color: d.individualNetValue >= 0 ? theme.up : theme.down })),
    );
    markers!.setMarkers(buildMarkers(candleData(), props.events, props.messages, theme));
    const labels = [t('valuePane'), t('flowPane')];
    watermarks.forEach((w, i) =>
      w.applyOptions({ horzAlign: 'left', vertAlign: 'top', lines: [{ text: labels[i], color: theme.text3, fontSize: 11, fontFamily: theme.font, fontStyle: '' }] }),
    );
  };

  onMount(() => {
    const theme = readTheme();
    chart = createTimeChart(host, theme);
    candles = chart.addSeries(CandlestickSeries, {
      priceLineVisible: false,
      lastValueVisible: true,
      priceFormat: { type: 'custom', formatter: priceFormatter(decimals()), minMove: 1 },
    });
    // Daily-limit band edges: thin dashed lines, dimmed so they read as context rather than as a series.
    const bandOptions = {
      lineWidth: 1 as const,
      lineStyle: LineStyle.Dashed,
      priceLineVisible: false,
      lastValueVisible: false,
      crosshairMarkerVisible: false,
      pointMarkersVisible: false,
      priceFormat: { type: 'custom' as const, formatter: priceFormatter(decimals()), minMove: 1 },
    };
    bandHigh = chart.addSeries(LineSeries, { ...bandOptions, color: theme.up });
    bandLow = chart.addSeries(LineSeries, { ...bandOptions, color: theme.down });
    value = chart.addSeries(
      HistogramSeries,
      { priceLineVisible: false, lastValueVisible: false, priceFormat: { type: 'custom', formatter: priceFormatter(1), minMove: 0.1 } },
      1,
    );
    flow = chart.addSeries(
      HistogramSeries,
      { priceLineVisible: false, lastValueVisible: false, base: 0, priceFormat: { type: 'custom', formatter: priceFormatter(1), minMove: 0.1 } },
      2,
    );
    flow.createPriceLine({ price: 0, color: theme.text3, lineWidth: 1, lineStyle: LineStyle.Solid, axisLabelVisible: false, title: '' });
    markers = createSeriesMarkers(candles, []);
    const panes = chart.panes();
    panes[0]?.setStretchFactor(3.2);
    panes[1]?.setStretchFactor(1);
    panes[2]?.setStretchFactor(1.2);
    for (const pane of [panes[1], panes[2]]) if (pane) watermarks.push(createTextWatermark(pane, { horzAlign: 'left', vertAlign: 'top', lines: [] }));

    const onMove = (param: MouseEventParams<Time>) => setHover(param.time ? String(param.time) : null);
    chart.subscribeCrosshairMove(onMove);

    // Candles (and markers) follow the raw/adjusted toggle without moving the visible window.
    createEffect(
      on(candleData, (data, previous) => {
        const keepRange = previous !== undefined && previous.length > 0 ? chart!.timeScale().getVisibleLogicalRange() : null;
        candles!.applyOptions({ priceFormat: { type: 'custom', formatter: priceFormatter(decimals()), minMove: 1 } });
        candles!.setData(data.map((c) => ({ time: c.time as Time, open: c.open, high: c.high, low: c.low, close: c.close })));
        bandHigh!.applyOptions({ priceFormat: { type: 'custom', formatter: priceFormatter(decimals()), minMove: 1 } });
        bandLow!.applyOptions({ priceFormat: { type: 'custom', formatter: priceFormatter(decimals()), minMove: 1 } });
        bandHigh!.setData(bandLine(data, 'bandHigh'));
        bandLow!.setData(bandLine(data, 'bandLow'));
        for (const l of limitLines) candles!.removePriceLine(l.line);
        limitLines = [];
        const latest = data[data.length - 1];
        if (latest && latest.bandHigh !== null && latest.bandLow !== null) {
          const theme = readTheme();
          const limit = (price: number, tone: 'up' | 'down') => ({
            line: candles!.createPriceLine({
              price,
              color: tone === 'up' ? theme.up : theme.down,
              lineWidth: 1,
              lineStyle: LineStyle.Dashed,
              axisLabelVisible: true,
              title: '',
            }),
            tone,
          });
          limitLines = [limit(latest.bandHigh, 'up'), limit(latest.bandLow, 'down')];
        }
        value!.setData(data.map((c) => ({ time: c.time as Time, value: c.bar.value / f.RIALS_PER_BILLION_TOMAN })));
        if (keepRange) chart!.timeScale().setVisibleLogicalRange(keepRange);
        else applyRange();
      }),
    );
    // Flows, markers and pane labels repaint with the data and with the theme.
    createEffect(on([candleData, () => props.flows, () => props.events, () => props.messages], () => paint(readTheme())));
    createEffect(on(() => props.range, applyRange, { defer: true }));
    createEffect(
      on(themeKey, () => {
        const next = readTheme();
        chart!.applyOptions(themeOptions(next));
        paint(next);
      }, { defer: true }),
    );

    onCleanup(() => {
      chart?.unsubscribeCrosshairMove(onMove);
      chart?.remove();
      chart = undefined;
    });
  });

  const flowShown = () => {
    const d = shownDate();
    return d ? flowByDate().get(d) ?? null : null;
  };
  const dayChange = () => shown()?.bar.return ?? null;

  return (
    <div class="symbol-chart">
      <div class="lw-wrap" style={{ height: `${props.height}px` }}>
        <div class="lw-legend wide" aria-live="off">
          <span class="lw-legend-date num">{shownDate() ? f.date(shownDate()) : ''}</span>
          <Show when={shown()}>
            {(c) => (
              <>
                <span class="opt">
                  {t('open_')} <bdi dir="ltr" class="num">{f.num(c().open, decimals())}</bdi>
                </span>
                <span class="opt">
                  {t('high')} <bdi dir="ltr" class="num">{f.num(c().high, decimals())}</bdi>
                </span>
                <span class="opt">
                  {t('low')} <bdi dir="ltr" class="num">{f.num(c().low, decimals())}</bdi>
                </span>
                <span>
                  {t('close')} <bdi dir="ltr" class="num">{f.num(c().close, decimals())}</bdi>
                </span>
                <bdi dir="ltr" class={`num chg ${f.fractionTone(dayChange())}`}>{f.fractionChangeText(dayChange())}</bdi>
                <span class="opt">
                  {t('value')} <bdi dir="ltr" class="num">{f.billionToman(c().bar.value)}</bdi>
                </span>
              </>
            )}
          </Show>
          <Show when={flowShown()}>
            {(d) => (
              <span>
                {t('sectorFlow')}{' '}
                <bdi dir="ltr" class={`num chg ${f.flowTone(d().individualNetValue)}`}>
                  {f.signed(d().individualNetValue / f.RIALS_PER_BILLION_TOMAN, 1)}
                </bdi>
              </span>
            )}
          </Show>
        </div>
        <div ref={host} class="lw-host" role="img" aria-label={[t('pricePane'), t('valuePane'), t('flowPane')].join(t('sep'))} />
      </div>
      <ul class="marker-legend">
        <li>
          <span class="mk circle event" aria-hidden="true" /> {t('markerAdjustment')}
        </li>
        <li>
          <span class="mk arrow-down up" aria-hidden="true" /> {t('markerLockedUp')}
        </li>
        <li>
          <span class="mk arrow-up down" aria-hidden="true" /> {t('markerLockedDown')}
        </li>
        <li>
          <span class="mk square warning" aria-hidden="true" /> {t('markerReopen')}
        </li>
        <li>
          <span class="mk square flat" aria-hidden="true" /> {t('markerHalt')}
        </li>
        <Show when={candleData().some((c) => c.bandHigh !== null)}>
          <li>
            <span class="mk band" aria-hidden="true" /> {t('markerLimitBand')}
          </li>
        </Show>
      </ul>
    </div>
  );
}
