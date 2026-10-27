// Tree-shaken ECharts (core + bar + treemap + grid/tooltip/markLine, canvas renderer) as a Solid component. The option is rebuilt when the
// reactive values it reads or the theme change; the chart resizes with its box and is disposed on unmount.
import { BarChart, TreemapChart } from 'echarts/charts';
import { GridComponent, MarkLineComponent, TooltipComponent } from 'echarts/components';
import * as echarts from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';
import { createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import { prefersReducedMotion, readTheme, themeKey, whenFontReady, type ChartTheme } from './theme';

// Legends are HTML (direct labels, proper RTL); accessibility comes from the container label and the table views.
echarts.use([BarChart, TreemapChart, GridComponent, TooltipComponent, MarkLineComponent, CanvasRenderer]);

export type EChartsOption = echarts.EChartsCoreOption;

export interface EChartClick {
  dataIndex: number;
  data: unknown;
  name: string;
}

export function EChart(props: {
  option: (theme: ChartTheme) => EChartsOption;
  height: number | string;
  label: string;
  onClick?: (event: EChartClick) => void;
  class?: string;
}) {
  let host!: HTMLDivElement;
  let chart: echarts.ECharts | undefined;
  const [ready, setReady] = createSignal(false);
  let disposed = false;
  let observer: ResizeObserver | undefined;

  onMount(() => {
    host.dir = 'ltr';
    void whenFontReady().then(() => {
      if (disposed) return;
      chart = echarts.init(host, null, { renderer: 'canvas' });
      // An event handler: reading props here at click time is intended.
      // eslint-disable-next-line solid/reactivity
      chart.on('click', (params) => {
        const p = params as unknown as EChartClick;
        props.onClick?.({ dataIndex: p.dataIndex, data: p.data, name: p.name });
      });
      observer = new ResizeObserver(() => chart?.resize());
      observer.observe(host);
      setReady(true);
    });
  });

  createEffect(() => {
    if (!ready() || !chart) return;
    themeKey();
    const theme = readTheme();
    chart.setOption(
      {
        animation: !prefersReducedMotion(),
        animationDuration: 250,
        animationDurationUpdate: 200,
        textStyle: { fontFamily: theme.font, color: theme.text2 },
        backgroundColor: 'transparent',
        ...props.option(theme),
      },
      { notMerge: true, lazyUpdate: true },
    );
  });

  onCleanup(() => {
    disposed = true;
    observer?.disconnect();
    chart?.dispose();
    chart = undefined;
  });

  return (
    <div
      ref={host}
      class={`echart ${props.class ?? ''}`}
      style={{ height: typeof props.height === 'number' ? `${props.height}px` : props.height }}
      role="img"
      aria-label={props.label}
    />
  );
}
