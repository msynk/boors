import { afterEach, describe, expect, it } from 'vitest';
import * as f from './format';

afterEach(() => f.configureFormat({ lang: 'fa', persianDigits: false }));

describe('numbers (MarketFormat.cs port)', () => {
  it('rounds half away from zero like .NET "N" formats and groups thousands', () => {
    expect(f.fixedAscii(1234567.891, 0)).toBe('1,234,568');
    expect(f.fixedAscii(1.005, 2)).toBe('1.01');
    expect(f.fixedAscii(2.5, 0)).toBe('3');
    expect(f.fixedAscii(-2.5, 0)).toBe('−3');
    expect(f.fixedAscii(999.95, 1)).toBe('1,000.0');
    expect(f.fixedAscii(0.004, 2)).toBe('0.00');
    expect(f.fixedAscii(-0.004, 2)).toBe('0.00'); // no "−0.00"
  });

  it('formats rial amounts in hemat, billion toman and million toman', () => {
    expect(f.hemat(3.08e14)).toBe('30.80');
    expect(f.hemat(-6.8e13)).toBe('−6.80');
    expect(f.billionToman(8.998e11)).toBe('90.0');
    expect(f.millionToman(84_600_000)).toBe('8');
    expect(f.price(8265157)).toBe('8,265,157');
    expect(f.ratio(1.234)).toBe('1.23');
    expect(f.num(null)).toBe('–');
    expect(f.hemat(undefined)).toBe('–');
  });

  it('switches to Persian digits and separators only when asked', () => {
    f.configureFormat({ persianDigits: true });
    expect(f.num(1234.5, 1)).toBe('۱٬۲۳۴٫۵');
    expect(f.percent(45.25, 1)).toBe('۴۵٫۳٪');
    expect(f.localDigits('1405/07/25')).toBe('۱۴۰۵/۰۷/۲۵');
    expect(f.asciiDigits('۱۴۰۵/۰۷/۲۵')).toBe('1405/07/25');
    expect(f.asciiDigits('٢٠٢٦-٠١-٠١')).toBe('2026-01-01');
  });

  it('writes signed changes with an arrow and the ±0.005 dead band', () => {
    expect(f.changeText(2.5)).toBe('▲ +2.50%');
    expect(f.changeText(-1.91)).toBe('▼ −1.91%');
    expect(f.changeText(0.004)).toBe('● 0.00%');
    expect(f.changeText(-0.004)).toBe('● 0.00%');
    expect(f.changeText(null)).toBe('–');
    expect(f.fractionChangeText(0.25)).toBe('▲ +25.00%');
    expect(f.tone(0.006)).toBe('up');
    expect(f.tone(-0.006)).toBe('down');
    expect(f.tone(0.005)).toBe('flat');
    expect(f.flowTone(-1)).toBe('down');
    expect(f.flowTone(0)).toBe('flat');
  });

  it('isolates signed runs left-to-right for RTL text', () => {
    const text = f.change(-3.37);
    expect(text.startsWith(f.LRI)).toBe(true);
    expect(text.endsWith(f.PDI)).toBe(true);
    expect(text).toContain('▼ −3.37%');
    expect(f.change(null)).toBe('–');
  });

  it('formats signed amounts', () => {
    expect(f.signed(1234)).toBe('+1,234');
    expect(f.signed(-1234)).toBe('−1,234');
    expect(f.signed(0)).toBe('0');
    expect(f.signed(-0.04, 1)).toBe('0.0');
  });
});

describe('dates', () => {
  it('converts to Jalali through Intl (fa) and keeps ISO under en', () => {
    expect(f.jalali('2026-10-17')).toEqual({ year: 1405, month: 7, day: 25 });
    expect(f.jalali('2025-03-21')).toEqual({ year: 1404, month: 1, day: 1 });
    expect(f.date('2026-10-17')).toBe('1405/07/25');
    f.configureFormat({ persianDigits: true });
    expect(f.date('2026-10-17')).toBe('۱۴۰۵/۰۷/۲۵');
    f.configureFormat({ lang: 'en', persianDigits: false });
    expect(f.date('2026-10-17')).toBe('2026-10-17');
    expect(f.date(null)).toBe('–');
  });

  it('names Jalali months under fa and Gregorian months under en', () => {
    expect(f.monthName('2026-10-17')).toBe('مهر');
    expect(f.yearLabel('2026-10-17')).toBe('1405');
    f.configureFormat({ lang: 'en' });
    expect(f.monthName('2026-10-17')).toBe('Oct');
    expect(f.yearLabel('2026-10-17')).toBe('2026');
  });

  it('does calendar arithmetic on yyyy-MM-dd', () => {
    expect(f.addCalendar('2026-03-31', -1)).toBe('2026-02-28');
    expect(f.addCalendar('2026-10-17', -12)).toBe('2025-10-17');
    expect(f.addDays('2026-10-17', -365)).toBe('2025-10-17');
    expect(f.utcToIso(f.isoToUtc('2024-02-29'))).toBe('2024-02-29');
  });

  it('formats DateTimeOffsets in local time', () => {
    const text = f.dateTime('2026-10-17T13:35:02.123+03:30');
    expect(text).toMatch(/^\d{4}\/\d{2}\/\d{2} \d{2}:\d{2}$/);
    expect(f.dateTime(null)).toBe('–');
  });
});

describe('search normalisation', () => {
  it('folds Arabic yeh/kaf, ZWNJ and case', () => {
    expect(f.normalizeSearch('فولاد مباركه')).toBe(f.normalizeSearch('فولاد مبارکه'));
    expect(f.normalizeSearch('ايران‌خودرو')).toBe('ایرانخودرو');
    expect(f.normalizeSearch('  TEDPIX  ')).toBe('tedpix');
    expect(f.persianText('سيمان كرمان')).toBe('سیمان کرمان');
  });
});
