// A single-line time series (TEDPIX in rial, or in dollars indexed to 100). One axis: never a dual-axis chart.
import { AreaSeries, type IChartApiBase, type ISeriesApi, type MouseEventParams, type Time } from 'lightweight-charts';
import { createEffect, createSignal, on, onCleanup, onMount } from 'solid-js';
import * as f from '../format/format';
import { createTimeChart, priceFormatter, themeOptions } from './lw';
import { readTheme, themeKey } from './theme';

export interface LinePoint {
  date: string;
  value: number;
}

export default function IndexChart(props: { points: LinePoint[]; label: string; decimals: number; height?: number }) {
  let host!: HTMLDivElement;
  let chart: IChartApiBase<Time> | undefined;
  let series: ISeriesApi<'Area', Time> | undefined;
  const [hover, setHover] = createSignal<LinePoint | null>(null);

  const last = () => props.points[props.points.length - 1] ?? null;
  const shown = () => hover() ?? last();
  const first = () => props.points[0] ?? null;
  const changeFromStart = () => {
    const s = shown();
    const a = first();
    return s && a && a.value ? (s.value / a.value - 1) * 100 : null;
  };

  const colors = () => {
    const theme = readTheme();
    return { lineColor: theme.accent, topColor: theme.accent + '33', bottomColor: theme.accent + '00' };
  };

  onMount(() => {
    const theme = readTheme();
    chart = createTimeChart(host, theme);
    series = chart.addSeries(AreaSeries, {
      ...colors(),
      lineWidth: 2,
      priceLineVisible: false,
      lastValueVisible: true,
      crosshairMarkerRadius: 4,
      priceFormat: { type: 'custom', formatter: priceFormatter(props.decimals), minMove: props.decimals ? 0.1 : 1 },
    });
    const byTime = new Map<string, LinePoint>();
    const onMove = (param: MouseEventParams<Time>) => {
      if (!param.time) return setHover(null);
      setHover(byTime.get(String(param.time)) ?? null);
    };
    chart.subscribeCrosshairMove(onMove);

    createEffect(
      on(
        () => props.points,
        (points) => {
          byTime.clear();
          for (const p of points) byTime.set(p.date, p);
          series!.applyOptions({ priceFormat: { type: 'custom', formatter: priceFormatter(props.decimals), minMove: props.decimals ? 0.1 : 1 } });
          series!.setData(points.map((p) => ({ time: p.date as Time, value: p.value })));
          chart!.timeScale().fitContent();
        },
      ),
    );

    createEffect(
      on(themeKey, () => {
        chart!.applyOptions(themeOptions(readTheme()));
        series!.applyOptions(colors());
      }, { defer: true }),
    );

    onCleanup(() => {
      chart?.unsubscribeCrosshairMove(onMove);
      chart?.remove();
      chart = undefined;
    });
  });

  return (
    <div class="lw-wrap" style={{ height: `${props.height ?? 300}px` }}>
      <div class="lw-legend" aria-live="off">
        <span class="lw-legend-label">{props.label}</span>
        <bdi dir="ltr" class="num">{shown() ? f.num(shown()!.value, props.decimals) : f.DASH}</bdi>
        <bdi dir="ltr" class={`num chg ${f.tone(changeFromStart())}`}>{f.changeText(changeFromStart())}</bdi>
        <span class="lw-legend-date num">{shown() ? f.date(shown()!.date) : ''}</span>
      </div>
      <div ref={host} class="lw-host" role="img" aria-label={props.label} />
    </div>
  );
}
