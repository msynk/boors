// Lightweight Charts setup shared by the index and symbol charts: LTR time axis in every culture, Jalali tick labels
// (and Jalali month/year boundaries) under fa, the digit setting on every label, theme colours from the CSS tokens,
// and the TradingView attribution logo kept visible (licence requirement).
import {
  ColorType,
  CrosshairMode,
  LineStyle,
  TickMarkType,
  createChartEx,
  defaultHorzScaleBehavior,
  type DeepPartial,
  type IChartApiBase,
  type IHorzScaleBehavior,
  type Mutable,
  type Time,
  type TimeChartOptions,
  type TimeScalePoint,
} from 'lightweight-charts';
import * as f from '../format/format';
import type { ChartTheme } from './theme';

/** yyyy-MM-dd of any LWC time value. */
export function timeToIso(time: Time): string {
  if (typeof time === 'string') return time;
  if (typeof time === 'number') return f.utcToIso(time * 1000);
  return `${time.year}-${String(time.month).padStart(2, '0')}-${String(time.day).padStart(2, '0')}`;
}

const YEAR = 70;
const MONTH = 60;
const DAY = 50;

const BaseBehavior = defaultHorzScaleBehavior();

/**
 * The default behaviour weights tick marks by Gregorian month/year starts. Under the Persian calendar the labels
 * would then sit mid-month, so this weights by Jalali boundaries instead (labels: year, month name, day).
 */
class JalaliHorzScaleBehavior extends BaseBehavior {
  override fillWeightsForPoints(points: readonly Mutable<TimeScalePoint>[], startIndex: number): void {
    if (points.length === 0) return;
    let prev = startIndex > 0 ? f.jalali(timeToIso(points[startIndex - 1].originalTime as Time)) : null;
    for (let i = startIndex; i < points.length; i++) {
      const point = points[i];
      const current = f.jalali(timeToIso(point.originalTime as Time));
      if (prev) {
        const weight = current.year !== prev.year ? YEAR : current.month !== prev.month ? MONTH : DAY;
        (point as { timeWeight: number }).timeWeight = weight;
      } else {
        (point as { timeWeight: number }).timeWeight = current.day <= 3 ? MONTH : DAY;
      }
      prev = current;
    }
  }
}

export function tickLabel(time: Time, type: TickMarkType): string {
  const iso = timeToIso(time);
  if (type === TickMarkType.Year) return f.yearLabel(iso);
  if (type === TickMarkType.Month) return f.monthName(iso);
  return f.formatConfig().lang === 'fa' ? f.localDigits(String(f.jalali(iso).day)) : f.localDigits(String(Number(iso.slice(8, 10))));
}

export function priceFormatter(decimals: number) {
  return (value: number) => f.num(value, decimals);
}

export function baseOptions(theme: ChartTheme): DeepPartial<TimeChartOptions> {
  return {
    autoSize: true,
    layout: {
      background: { type: ColorType.Solid, color: theme.surface },
      textColor: theme.text3,
      fontFamily: theme.font,
      fontSize: 11,
      attributionLogo: true,
      panes: { separatorColor: theme.axis, separatorHoverColor: theme.grid, enableResize: true },
    },
    grid: {
      vertLines: { color: theme.grid, style: LineStyle.Solid },
      horzLines: { color: theme.grid, style: LineStyle.Solid },
    },
    crosshair: {
      mode: CrosshairMode.Normal,
      vertLine: { color: theme.text3, labelBackgroundColor: theme.axis, style: LineStyle.Dashed, width: 1 },
      horzLine: { color: theme.text3, labelBackgroundColor: theme.axis, style: LineStyle.Dashed, width: 1 },
    },
    rightPriceScale: { borderColor: theme.axis, scaleMargins: { top: 0.08, bottom: 0.06 } },
    timeScale: {
      borderColor: theme.axis,
      rightOffset: 3,
      minBarSpacing: 0.5,
      fixLeftEdge: false,
      lockVisibleTimeRangeOnResize: true,
      tickMarkFormatter: (time: Time, type: TickMarkType) => tickLabel(time, type),
    },
    localization: {
      locale: f.formatConfig().lang === 'fa' ? 'fa-IR' : 'en-US',
      timeFormatter: (time: Time) => f.date(timeToIso(time)),
      priceFormatter: priceFormatter(0),
    },
    handleScale: { axisPressedMouseMove: { time: true, price: true } },
    kineticScroll: { mouse: false, touch: true },
  };
}

export function themeOptions(theme: ChartTheme): DeepPartial<TimeChartOptions> {
  const o = baseOptions(theme);
  return { layout: o.layout, grid: o.grid, crosshair: o.crosshair, rightPriceScale: { borderColor: theme.axis }, timeScale: { borderColor: theme.axis } };
}

/** A time chart whose container is forced LTR (the time axis reads left to right in every culture). */
export function createTimeChart(container: HTMLElement, theme: ChartTheme): IChartApiBase<Time> {
  container.dir = 'ltr';
  const behavior: IHorzScaleBehavior<Time> = f.formatConfig().lang === 'fa' ? new JalaliHorzScaleBehavior() : new BaseBehavior();
  return createChartEx<Time, IHorzScaleBehavior<Time>>(container, behavior, baseOptions(theme));
}
