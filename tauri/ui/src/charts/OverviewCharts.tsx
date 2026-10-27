// ECharts views of the overview page: 100% breadth bars, sorted diverging sector bars and the market treemap.
// Loaded lazily (one chunk with ECharts) so the overview's first paint does not wait for the chart runtime.
import { format as echartsFormat } from 'echarts/core';
import { createMemo } from 'solid-js';
import type { BreadthDayDto, MarketWatchEntryDto, SectorPerformanceDto } from '../api';
import * as f from '../format/format';
import { dayStatus, t } from '../i18n';
import { divergingColor, inkOn, mix } from './color';
import { EChart, type EChartsOption } from './EChart';
import type { ChartTheme } from './theme';

const esc = (s: string) => echartsFormat.encodeHTML(s);
const tooltipDir = () => (f.formatConfig().lang === 'fa' ? 'rtl' : 'ltr');

function tooltipBase(theme: ChartTheme) {
  return {
    backgroundColor: theme.surface,
    borderColor: theme.axis,
    borderWidth: 1,
    padding: [6, 10],
    textStyle: { color: theme.text, fontFamily: theme.font, fontSize: 12 },
    extraCssText: `direction:${tooltipDir()};box-shadow:0 4px 18px rgba(0,0,0,.25);border-radius:6px;`,
  };
}

const row = (label: string, value: string, swatch?: string) =>
  `<div style="display:flex;gap:12px;justify-content:space-between;align-items:center">` +
  `<span>${swatch ? `<span style="display:inline-block;width:8px;height:8px;border-radius:2px;background:${swatch};margin-inline-end:6px"></span>` : ''}${esc(label)}</span>` +
  `<span style="direction:ltr;unicode-bidi:isolate;font-variant-numeric:tabular-nums">${esc(value)}</span></div>`;

// ---- breadth -------------------------------------------------------------------------------------------------------

export function BreadthChart(props: { days: BreadthDayDto[]; height?: number }) {
  const option = (theme: ChartTheme): EChartsOption => {
    const days = props.days;
    const categories = [
      { key: 'up', name: t('advancers'), color: theme.up, value: (d: BreadthDayDto) => d.advancers },
      { key: 'flat', name: t('unchanged'), color: theme.flat, value: (d: BreadthDayDto) => d.unchanged },
      { key: 'down', name: t('decliners'), color: theme.down, value: (d: BreadthDayDto) => d.decliners },
      { key: 'inactive', name: t('halted'), color: theme.inactive, value: (d: BreadthDayDto) => d.notTraded + d.halted },
    ];
    const totals = days.map((d) => categories.reduce((sum, c) => sum + c.value(d), 0) || 1);
    const last = days.length - 1;
    return {
      grid: { left: 8, right: 92, top: 8, bottom: 8 },
      tooltip: {
        ...tooltipBase(theme),
        trigger: 'axis',
        axisPointer: { type: 'shadow', shadowStyle: { color: theme.grid + '80' } },
        formatter: (items: { dataIndex: number }[]) => {
          const d = days[items[0]?.dataIndex ?? 0];
          if (!d) return '';
          const total = totals[items[0].dataIndex];
          return (
            `<div style="font-weight:600;margin-bottom:4px">${esc(f.date(d.date))}</div>` +
            categories.map((c) => row(c.name, `${f.integer(c.value(d))}${t('sep')}${f.percent((c.value(d) / total) * 100, 0)}`, c.color)).join('') +
            row(t('lockedUp'), f.integer(d.lockedUp)) +
            row(t('lockedDown'), f.integer(d.lockedDown)) +
            row(t('equityValue'), f.hemat(d.equityValue))
          );
        },
      },
      xAxis: {
        type: 'category',
        data: days.map((d) => f.shortDate(d.date)),
        axisLine: { lineStyle: { color: theme.axis } },
        axisTick: { show: false },
        axisLabel: { color: theme.text3, hideOverlap: true, fontSize: 10 },
      },
      yAxis: {
        type: 'value',
        max: 100,
        interval: 25,
        axisLabel: { color: theme.text3, formatter: (v: number) => f.percent(v, 0), fontSize: 10 },
        splitLine: { lineStyle: { color: theme.grid } },
      },
      series: categories.map((c) => ({
        name: c.name,
        type: 'bar',
        stack: 'breadth',
        barCategoryGap: '22%',
        itemStyle: { color: c.color, borderColor: theme.surface, borderWidth: 1 },
        emphasis: { focus: 'series' },
        // Direct labels on the latest session only.
        label: {
          show: true,
          position: 'right',
          color: theme.text2,
          fontSize: 10,
          // Number first: the canvas paragraph is LTR, so "17% name" reads correctly in both languages.
          formatter: (p: { dataIndex: number }) => (p.dataIndex === last ? `${f.percent((c.value(days[last]) / totals[last]) * 100, 0)} ${c.name}` : ''),
        },
        labelLayout: { hideOverlap: true },
        data: days.map((d, i) => (c.value(d) / totals[i]) * 100),
      })),
    };
  };
  const legend = () => [
    { name: t('advancers'), cls: 'up' },
    { name: t('unchanged'), cls: 'flat' },
    { name: t('decliners'), cls: 'down' },
    { name: `${t('notTraded')} / ${t('halted')}`, cls: 'inactive' },
  ];
  return (
    <>
      <ul class="chart-legend">
        {legend().map((item) => (
          <li>
            <span class={`legend-swatch ${item.cls}`} aria-hidden="true" />
            {item.name}
          </li>
        ))}
      </ul>
      <EChart option={option} height={(props.height ?? 280) - 24} label={t('breadthTitle', props.days.length)} />
    </>
  );
}

// ---- sectors -------------------------------------------------------------------------------------------------------

export type SectorMetric = 'return' | 'flow';

export function SectorBars(props: { sectors: SectorPerformanceDto[]; metric: SectorMetric }) {
  const rows = createMemo(() => {
    const value = (s: SectorPerformanceDto) => (props.metric === 'return' ? (s.weightedReturn === null ? null : s.weightedReturn * 100) : s.individualNetFlow / f.RIALS_PER_BILLION_TOMAN);
    return props.sectors
      .map((s) => ({ sector: s, value: value(s) }))
      .filter((r): r is { sector: SectorPerformanceDto; value: number } => r.value !== null && Number.isFinite(r.value))
      .sort((a, b) => b.value - a.value);
  });
  const label = (v: number) => (props.metric === 'return' ? f.changeText(v, 2) : f.signed(v, 1));
  const option = (theme: ChartTheme): EChartsOption => {
    // A snapshot per render: the tooltip formatter must describe the bars that were drawn with it.
    // eslint-disable-next-line solid/reactivity
    const data = rows();
    return {
      grid: { left: 8, right: 64, top: 8, bottom: 24 },
      tooltip: {
        ...tooltipBase(theme),
        trigger: 'item',
        formatter: (p: { dataIndex: number }) => {
          const r = data[p.dataIndex];
          if (!r) return '';
          const s = r.sector;
          return (
            `<div style="font-weight:600;margin-bottom:4px">${esc(f.persianText(s.sectorName) || s.sectorCode)}</div>` +
            row(t('sectorReturns'), f.fractionChangeText(s.weightedReturn)) +
            row(t('sectorFlow'), f.signed(s.individualNetFlow / f.RIALS_PER_BILLION_TOMAN, 1) + ' ' + t('billionToman')) +
            row(t('valueBillionToman'), f.billionToman(s.value)) +
            row(`${t('advancers')} / ${t('decliners')}`, `${f.integer(s.advancers)} / ${f.integer(s.decliners)}`)
          );
        },
      },
      xAxis: {
        type: 'value',
        axisLabel: { color: theme.text3, fontSize: 10, formatter: (v: number) => (props.metric === 'return' ? f.percent(v, 0) : f.num(v, 0)) },
        splitLine: { lineStyle: { color: theme.grid } },
        axisLine: { show: false },
      },
      yAxis: {
        type: 'category',
        inverse: true,
        data: data.map((r) => f.persianText(r.sector.sectorName) || r.sector.sectorCode),
        axisTick: { show: false },
        // Keep the names at the edge; the zero line is a mark line so it does not drag the labels to x = 0.
        axisLine: { show: false, onZero: false },
        axisLabel: { color: theme.text2, fontSize: 11, width: 150, overflow: 'truncate' },
      },
      series: [
        {
          type: 'bar',
          barWidth: '62%',
          data: data.map((r) => ({
            value: r.value,
            itemStyle: { color: r.value >= 0 ? theme.up : theme.down, borderRadius: r.value >= 0 ? [0, 3, 3, 0] : [3, 0, 0, 3] },
            label: { position: r.value >= 0 ? 'right' : 'left' },
          })),
          label: { show: true, color: theme.text2, fontSize: 10, formatter: (p: { value: number }) => label(p.value) },
          markLine: {
            silent: true,
            symbol: 'none',
            label: { show: false },
            lineStyle: { color: theme.text3, type: 'solid', width: 1 },
            data: [{ xAxis: 0 }],
          },
        },
      ],
    };
  };
  return <EChart option={option} height={Math.max(260, rows().length * 22 + 40)} label={props.metric === 'return' ? t('sectorReturnsNote') : t('sectorFlowNote')} />;
}

// ---- treemap -------------------------------------------------------------------------------------------------------

const HALTED = new Set(['SymbolHalted', 'NoTrade', 'MarketClosed']);

/** The instrument's daily limit in percent, from its allowed band and implied yesterday price (default 3%). */
export function dailyLimitPercent(e: MarketWatchEntryDto): number {
  if (e.allowedHigh && e.close > 0 && e.changePercent !== null) {
    const yesterday = e.close / (1 + e.changePercent / 100);
    if (yesterday > 0) {
      const limit = (e.allowedHigh / yesterday - 1) * 100;
      if (limit > 0.5 && limit < 30) return limit;
    }
  }
  return 3;
}

export interface TreemapLeaf {
  name: string;
  value: number;
  insCode: string;
  change: number | null;
  halted: boolean;
}

export function MarketTreemap(props: { entries: MarketWatchEntryDto[]; sectorNames: Map<string, string>; onOpen: (insCode: string) => void; height: number }) {
  const option = (theme: ChartTheme): EChartsOption => {
    const poles = { up: theme.up, down: theme.down, mid: theme.mid, upOverflow: theme.upOverflow, downOverflow: theme.downOverflow };
    const groups = new Map<string, { name: string; value: number; children: unknown[]; itemStyle: { color: string } }>();
    const leaves = new Map<string, MarketWatchEntryDto>();
    for (const e of props.entries) {
      if (!e.marketCap || e.marketCap <= 0) continue;
      const code = e.sectorCode ?? '—';
      let group = groups.get(code);
      if (!group) {
        group = { name: f.persianText(props.sectorNames.get(code)) || code, value: 0, children: [], itemStyle: { color: theme.mid } };
        groups.set(code, group);
      }
      const halted = e.trades === 0 || (e.dayStatus !== null && HALTED.has(e.dayStatus));
      const color = halted ? theme.inactive : divergingColor(e.changePercent ?? 0, dailyLimitPercent(e), poles);
      leaves.set(e.insCode, e);
      group.value += e.marketCap;
      group.children.push({
        name: f.persianText(e.symbol),
        value: e.marketCap,
        insCode: e.insCode,
        itemStyle: halted
          ? { color, decal: { symbol: 'rect', symbolSize: 1, dashArrayX: [1, 0], dashArrayY: [2, 5], rotation: Math.PI / 4, color: theme.hatch } }
          : { color },
        label: { color: inkOn(color) },
        change: e.changePercent,
        halted,
      });
    }
    const data = [...groups.values()].sort((a, b) => b.value - a.value);
    return {
      tooltip: {
        ...tooltipBase(theme),
        formatter: (p: { data?: { insCode?: string }; name: string; value: number }) => {
          const e = p.data?.insCode ? leaves.get(p.data.insCode) : undefined;
          if (!e) return `<div style="font-weight:600">${esc(p.name)}</div>` + row(t('marketCapHemat'), f.hemat(p.value));
          return (
            `<div style="font-weight:600;margin-bottom:4px">${esc(f.persianText(e.symbol))}${esc(t('sep'))}${esc(f.persianText(e.name))}</div>` +
            row(t('change'), f.changeText(e.changePercent)) +
            row(t('closingPrice'), f.price(e.close)) +
            row(t('marketCapHemat'), f.hemat(e.marketCap)) +
            row(t('valueBillionToman'), f.billionToman(e.value)) +
            row(t('status'), dayStatus(e.dayStatus))
          );
        },
      },
      series: [
        {
          type: 'treemap',
          data,
          roam: false,
          nodeClick: false,
          breadcrumb: { show: false },
          width: '100%',
          height: '100%',
          top: 0,
          left: 0,
          squareRatio: 0.5 * (1 + Math.sqrt(5)),
          visibleMin: 80,
          label: {
            show: true,
            fontFamily: theme.font,
            fontSize: 11,
            lineHeight: 14,
            overflow: 'truncate',
            formatter: (p: { name: string; data?: { change?: number | null; halted?: boolean } }) =>
              p.data?.halted ? `${p.name}\n${dayStatus('SymbolHalted')}` : `${p.name}\n${f.changeText(p.data?.change ?? null, 1)}`,
          },
          upperLabel: { show: true, height: 18, color: theme.text2, fontSize: 11, fontFamily: theme.font, overflow: 'truncate' },
          itemStyle: { borderColor: theme.surface, borderWidth: 1, gapWidth: 1 },
          levels: [
            { itemStyle: { borderColor: theme.surface, borderWidth: 0, gapWidth: 3 }, upperLabel: { show: false } },
            { itemStyle: { borderColor: mix(theme.surface, theme.axis, 0.6), borderWidth: 2, gapWidth: 1 }, upperLabel: { show: true } },
            { itemStyle: { borderColor: theme.surface, borderWidth: 0.5, gapWidth: 0 } },
          ],
        },
      ],
    };
  };
  return (
    <EChart
      option={option}
      height={props.height}
      label={t('treemapTitle')}
      onClick={(event) => {
        const insCode = (event.data as { insCode?: string } | undefined)?.insCode;
        if (insCode) props.onOpen(insCode);
      }}
    />
  );
}
