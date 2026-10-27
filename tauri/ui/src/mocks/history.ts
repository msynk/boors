// Per-instrument history (bars, flows, capital events, halts, Codal letters, holders) generated deterministically and
// cached, plus index and macro series. Bars end exactly at the session snapshot, so the symbol page agrees with the
// market watch and the overview.
import type {
  BarDto,
  CodalLetterDto,
  CorporateEventDto,
  FlowDayDto,
  InstrumentMetricsDto,
  SeriesPointDto,
  ShareholderDto,
  SupervisorMessageDto,
} from '../api/types';
import { addCalendar, addDays, isoToUtc, jalali } from '../format/format';
import { SESSION, allTradingDays, anchoredSeries, quoteDays } from './calendar';
import { clamp, int, lognormal, mulberry32, normal, pick, rngFor, uniform } from './rng';
import { EQUITY_CLASSES, FUND_CLASSES, SECTOR_NAMES, getUniverse, type MockInstrument } from './universe';

export interface History {
  bars: BarDto[];
  flows: FlowDayDto[];
  events: CorporateEventDto[];
  messages: SupervisorMessageDto[];
}

const cache = new Map<string, History>();

interface EventPlan {
  index: number;
  factor: number;
  kind: 'capital' | 'dividend';
  haltDays: number;
}

function planEvents(i: MockInstrument, days: string[], start: number): EventPlan[] {
  if (!EQUITY_CLASSES.includes(i.cls)) return [];
  const rng = rngFor('events' + i.insCode);
  const plans: EventPlan[] = [];
  // Roughly yearly: a dividend after the AGM (Tir–Mordad) and, every other year, a capital increase.
  for (let index = start + int(rng, 60, 200); index < days.length - 8; index += int(rng, 180, 300)) {
    const capital = rng() < 0.45;
    plans.push({
      index,
      factor: capital ? pick(rng, [0.5, 0.5, 0.6667, 0.8, 0.4]) : clamp(uniform(rng, 0.86, 0.97), 0.8, 0.99),
      kind: capital ? 'capital' : 'dividend',
      haltDays: capital ? int(rng, 2, 6) : int(rng, 1, 3),
    });
  }
  if (i.insCode === '46348559193224090') {
    // فولاد: a capital increase that applied on 2026-08-05 (listed 2026-08-03), as in the data findings.
    const target = days.findIndex((d) => d >= '2026-08-05');
    if (target > 0) {
      const kept = plans.filter((p) => Math.abs(p.index - target) > 40);
      kept.push({ index: target, factor: 0.5, kind: 'capital', haltDays: 3 });
      kept.sort((a, b) => a.index - b.index);
      return kept;
    }
  }
  return plans;
}

export function history(insCode: string): History | null {
  const hit = cache.get(insCode);
  if (hit) return hit;
  const u = getUniverse();
  const inst = u.byCode.get(insCode);
  if (!inst) return null;
  const snap = u.snapshot.get(insCode)!;
  const all = allTradingDays();
  const startIndex = Math.max(0, all.indexOf(inst.listed));
  const days = all.slice(startIndex);
  const n = days.length;
  const rng = rngFor('bars' + insCode);
  const isEquity = EQUITY_CLASSES.includes(inst.cls);
  const isFund = FUND_CLASSES.includes(inst.cls);
  const sigma = isEquity ? (inst.cls === 'TseStock' ? 0.017 : 0.021) : isFund ? (inst.cls === 'EtfFixedIncome' ? 0.0006 : 0.012) : inst.cls === 'Option' ? 0.06 : 0.002;
  const limit = inst.limit / 100;

  // 1. Daily returns (log), adjusted for the one-year target.
  const ret = new Array<number>(n).fill(0);
  const drift = inst.cls === 'EtfFixedIncome' ? 0.00095 : inst.cls === 'Debt' ? 0.0011 : 0.0012;
  for (let k = 1; k < n; k++) ret[k] = normal(rng, drift, sigma);
  const yearStart = Math.max(1, days.findIndex((d) => d > addCalendar(SESSION, -12)));
  if (isEquity || isFund) {
    for (let iter = 0; iter < 4; iter++) {
      let realized = 0;
      for (let k = yearStart; k < n - 1; k++) realized += ret[k];
      const sessionReturn = Math.log(1 + (snap.entry.changePercent ?? 0) / 100);
      const delta = (Math.log(1 + inst.oneYear) - sessionReturn - realized) / Math.max(1, n - 1 - yearStart);
      for (let k = yearStart; k < n - 1; k++) ret[k] = clamp(ret[k] + delta, Math.log(1 - limit), Math.log(1 + limit));
    }
  }

  // 2. Halts and corporate events.
  const status = new Array<string>(n).fill('Traded');
  const plans = planEvents(inst, days, 0);
  const factorAt = new Array<number>(n).fill(1);
  for (const p of plans) {
    for (let h = Math.max(1, p.index - p.haltDays); h <= p.index; h++) status[h] = 'SymbolHalted';
    factorAt[p.index] = p.factor;
    if (p.index + 1 < n) {
      status[p.index + 1] = 'ReopenAuction';
      ret[p.index + 1] = clamp(normal(rng, 0.02, 0.06), -0.12, 0.15);
    }
  }
  // Disclosure halts (one or two sessions, reopening with a price limit).
  for (let k = 30; k < n - 2; k += int(rng, 70, 160)) {
    if (status[k] !== 'Traded' || !isEquity) continue;
    status[k] = 'SymbolHalted';
  }
  if (!isEquity && !isFund) for (let k = 1; k < n - 1; k++) if (rng() < (inst.cls === 'Option' ? 0.55 : 0.4)) status[k] = 'NoTrade';
  for (let k = 1; k < n - 1; k++) {
    if (status[k] !== 'Traded') continue;
    if (ret[k] >= Math.log(1 + limit) - 1e-6 && rng() < 0.5) status[k] = 'QueueLockedUp';
    else if (ret[k] <= Math.log(1 - limit) + 1e-6 && rng() < 0.5) status[k] = 'QueueLockedDown';
  }
  // The session itself follows the snapshot.
  status[n - 1] = snap.entry.dayStatus ?? 'Traded';
  ret[n - 1] = Math.log(1 + (snap.entry.changePercent ?? 0) / 100);

  // 3. Raw closes forward; then scale so the last close is the session close.
  const raw = new Array<number>(n);
  raw[0] = 1000;
  const yesterdays = new Array<number>(n);
  yesterdays[0] = raw[0];
  for (let k = 1; k < n; k++) {
    const base = raw[k - 1] * factorAt[k];
    yesterdays[k] = base;
    const priced = status[k] !== 'SymbolHalted' && status[k] !== 'NoTrade';
    raw[k] = priced ? base * Math.exp(ret[k]) : base;
  }
  const k0 = snap.entry.close / raw[n - 1];
  // PIT adjustment as of the session: multiply by every later event's factor.
  const later = new Array<number>(n).fill(1);
  for (let k = n - 2; k >= 0; k--) later[k] = later[k + 1] * factorAt[k + 1];

  const bars: BarDto[] = [];
  const flows: FlowDayDto[] = [];
  const valueRng = mulberry32(rngFor('value' + insCode)() * 2 ** 32);
  for (let k = 0; k < n; k++) {
    const date = days[k];
    const close = Math.max(1, Math.round(raw[k] * k0));
    const yesterday = Math.max(1, Math.round(yesterdays[k] * k0));
    const s = status[k];
    const priced = s !== 'SymbolHalted' && s !== 'NoTrade' && s !== 'MarketClosed';
    const last = k === n - 1;
    let open = close;
    let high = close;
    let low = close;
    let value = 0;
    let trades = 0;
    if (priced) {
      if (last) {
        open = snap.open;
        high = snap.high;
        low = snap.low;
        value = snap.entry.value;
        trades = snap.entry.trades;
      } else if (s === 'QueueLockedUp' || s === 'QueueLockedDown') {
        value = inst.liquidity * Math.exp(normal(valueRng, -1.2, 0.5));
        trades = Math.max(1, Math.round(value / 3e8));
      } else {
        const bandHigh = yesterday * (1 + (s === 'ReopenAuction' ? 0.2 : limit));
        const bandLow = yesterday * (1 - (s === 'ReopenAuction' ? 0.2 : limit));
        open = Math.round(clamp(yesterday * Math.exp(normal(valueRng, ret[k] / 3, sigma / 2)), bandLow, bandHigh));
        high = Math.round(clamp(Math.max(open, close) * (1 + Math.abs(normal(valueRng, 0, sigma / 2))), Math.max(open, close), bandHigh));
        low = Math.round(clamp(Math.min(open, close) * (1 - Math.abs(normal(valueRng, 0, sigma / 2))), bandLow, Math.min(open, close)));
        value = inst.liquidity * Math.exp(normal(valueRng, 0, 0.55)) * (1 + (2 * Math.abs(ret[k])) / limit) * (0.55 + (0.45 * k) / n);
        trades = Math.max(1, Math.round(value / lognormal(valueRng, 4e8, 0.5)));
      }
    }
    const volume = priced ? Math.max(1, Math.round(value / close)) : 0;
    // The real store knows the band only for sessions collected at end of day (market-watch snapshots), not history.
    const banded = n - k <= 20 && s !== 'MarketClosed';
    bars.push({
      date,
      open,
      high,
      low,
      close,
      adjustedClose: Math.round(close * later[k] * 100) / 100,
      volume,
      value,
      trades,
      status: s,
      return: priced && k > 0 ? Math.round((close / yesterday - 1) * 1e6) / 1e6 : null,
      allowedLow: banded ? Math.round(yesterday * (1 - limit)) : null,
      allowedHigh: banded ? Math.round(yesterday * (1 + limit)) : null,
    });

    if (priced && (isEquity || isFund || inst.cls === 'Rights')) {
      let buyInd: number;
      let sellInd: number;
      let buyCount: number;
      let sellCount: number;
      if (last && snap.hasFlows) {
        buyInd = snap.buyIndividualVolume * close;
        sellInd = snap.sellIndividualVolume * close;
        buyCount = snap.buyIndividualCount;
        sellCount = snap.sellIndividualCount;
      } else {
        const r = ret[k];
        const b = clamp(normal(valueRng, 0.74 + r * 6, 0.1), 0.1, 1);
        const sl = clamp(normal(valueRng, 0.76 - r * 6, 0.1), 0.1, 1);
        buyInd = value * b;
        sellInd = value * sl;
        buyCount = Math.max(1, Math.round(buyInd / lognormal(valueRng, 1.0e8 * (1 + r * 8), 0.4)));
        sellCount = Math.max(1, Math.round(sellInd / lognormal(valueRng, 1.2e8, 0.4)));
      }
      const pcb = buyInd / buyCount;
      const pcs = sellInd / sellCount;
      flows.push({
        date,
        buyIndividualValue: buyInd,
        sellIndividualValue: sellInd,
        buyInstitutionalValue: Math.max(0, value - buyInd),
        sellInstitutionalValue: Math.max(0, value - sellInd),
        buyIndividualCount: buyCount,
        sellIndividualCount: sellCount,
        individualNetValue: last && snap.entry.individualNetValue !== null ? snap.entry.individualNetValue : buyInd - sellInd,
        perCapitaBuy: pcb,
        perCapitaSell: pcs,
        buyerPower: pcs > 0 ? Math.round((pcb / pcs) * 1000) / 1000 : null,
        valuesEstimated: last,
      });
    }
  }

  // 4. Events and supervisor messages.
  const events: CorporateEventDto[] = [];
  const messages: SupervisorMessageDto[] = [];
  let shares = inst.shares;
  const reverse = [...plans].reverse();
  const sharesAfter = new Map<number, number>();
  for (const p of reverse) {
    sharesAfter.set(p.index, shares);
    if (p.kind === 'capital') shares = Math.round(shares * p.factor);
  }
  const symbol1 = inst.symbol + '1';
  let messageId = 260551 + (Number(insCode.slice(-4)) % 1000) * 10;
  for (const p of plans) {
    const date = days[p.index];
    events.push({ date, kind: 'adjustment', source: p.kind === 'capital' && rngFor('src' + date)() < 0.5 ? 'implied' : 'tsetmc', factor: Math.round(p.factor * 10000) / 10000, sharesBefore: null, sharesAfter: null });
    if (p.kind === 'capital') {
      const after = sharesAfter.get(p.index)!;
      events.push({ date, kind: 'share_change', source: 'tsetmc', factor: null, sharesBefore: Math.round(after * p.factor), sharesAfter: after });
    }
    const haltDate = days[Math.max(0, p.index - p.haltDays)];
    messages.push({
      id: String(messageId++),
      date: haltDate,
      title: `توقف نماد (${symbol1}) به علت برگزاری مجمع عمومی ${p.kind === 'capital' ? 'فوق‌العاده' : 'عادی سالیانه'}`,
      body: `به اطلاع می‌رساند نماد معاملاتی شرکت ${inst.name} (${symbol1}) به منظور برگزاری مجمع عمومی متوقف گردید. مدیریت عملیات بازار`,
      kind: 'Halt',
      reason: 'GeneralMeeting',
      reopensWithoutLimit: false,
    });
    if (p.index + 1 < n) {
      messages.push({
        id: String(messageId++),
        date: days[p.index + 1],
        title: `بازگشایی نماد (${symbol1})`,
        body: `نماد ${symbol1} پس از برگزاری مجمع، با حراج و بدون محدودیت دامنهٔ نوسان قیمت بازگشایی می‌شود.`,
        kind: 'Reopen',
        reason: 'GeneralMeeting',
        reopensWithoutLimit: true,
      });
    }
  }
  for (let k = 0; k < n; k++) {
    if (status[k] !== 'SymbolHalted' || plans.some((p) => k >= p.index - p.haltDays && k <= p.index)) continue;
    messages.push({
      id: String(messageId++),
      date: days[k],
      title: `توقف نماد (${symbol1}) به علت افشای اطلاعات بااهمیت گروه ب`,
      body: `به اطلاع می‌رساند نماد معاملاتی شرکت ${inst.name} (${symbol1}) با توجه به افشای اطلاعات بااهمیت گروه ب، متوقف گردید. نماد مذکور حداکثر تا ساعت ۱۰:۱۵ روز معاملاتی بعد با محدودیت دامنهٔ نوسان قیمت بازگشایی خواهد شد.`,
      kind: 'Halt',
      reason: 'Disclosure',
      reopensWithoutLimit: false,
    });
  }
  events.sort((a, b) => (a.date < b.date ? 1 : -1));
  messages.sort((a, b) => (a.date < b.date ? 1 : a.date > b.date ? -1 : 0));
  const result = { bars, flows, events, messages };
  cache.set(insCode, result);
  return result;
}

export function since<T extends { date: string }>(rows: T[], from: string | null): T[] {
  return from ? rows.filter((r) => r.date >= from) : rows;
}

// ---- metrics ---------------------------------------------------------------------------------------------------

export function metrics(insCode: string, usd: SeriesPointDto[]): InstrumentMetricsDto | null {
  const u = getUniverse();
  const inst = u.byCode.get(insCode);
  const h = history(insCode);
  if (!inst || !h) return null;
  const bars = h.bars;
  const last = bars[bars.length - 1];
  const asOf = last.date;
  const adjustedOnOrBefore = (date: string) => {
    let value: number | null = null;
    for (const b of bars) {
      if (b.date > date) break;
      value = b.adjustedClose;
    }
    return value;
  };
  const lastAdjusted = last.adjustedClose;
  const returnSince = (date: string) => {
    const c = adjustedOnOrBefore(date);
    return c && c > 0 ? lastAdjusted / c - 1 : null;
  };
  const yearAgo = addDays(asOf, -365);
  const window = bars.filter((b) => b.date > yearAgo && b.trades > 0);
  const high = window.length ? Math.max(...window.map((b) => b.adjustedClose)) : null;
  const low = window.length ? Math.min(...window.map((b) => b.adjustedClose)) : null;
  const usdOn = (date: string) => {
    let value: number | null = null;
    for (const p of usd) {
      if (p.date > date) break;
      value = p.value;
    }
    return value;
  };
  const oneYear = returnSince(yearAgo);
  const usdNow = usdOn(asOf);
  const usdThen = usdOn(yearAgo);
  const recent = bars.filter((b) => b.trades > 0).slice(-20);
  const recentDates = new Set(recent.map((b) => b.date));
  const lastFlow = h.flows.find((f) => f.date === asOf) ?? null;
  const powers = h.flows.slice(-250).map((f) => f.buyerPower).filter((p): p is number => p !== null);
  const power = lastFlow?.buyerPower ?? null;
  return {
    insCode,
    symbol: inst.symbol,
    name: inst.name,
    instrumentClass: inst.cls,
    sectorCode: inst.sector,
    sectorName: SECTOR_NAMES[inst.sector] ?? null,
    asOf,
    dayStatus: last.status,
    close: last.close,
    changePercent: last.return === null ? null : Math.round(last.return * 10000) / 100,
    marketCap: inst.shares > 0 ? last.close * inst.shares : null,
    high52Week: high,
    low52Week: low,
    fromHigh52WeekPercent: high ? (lastAdjusted / high - 1) * 100 : null,
    returns: {
      '1w': returnSince(addDays(asOf, -7)),
      '1m': returnSince(addDays(asOf, -30)),
      '3m': returnSince(addDays(asOf, -91)),
      '6m': returnSince(addDays(asOf, -182)),
      '1y': oneYear,
    },
    usdReturn1Year: oneYear !== null && usdNow && usdThen ? (1 + oneYear) / (usdNow / usdThen) - 1 : null,
    averageValue20: recent.length ? recent.reduce((s, b) => s + b.value, 0) / recent.length : null,
    individualNetFlow20: h.flows.filter((f) => recentDates.has(f.date)).reduce((s, f) => s + f.individualNetValue, 0),
    perCapitaBuy: lastFlow?.perCapitaBuy ?? null,
    perCapitaSell: lastFlow?.perCapitaSell ?? null,
    buyerPower: power,
    buyerPowerPercentile: power !== null && powers.length >= 20 ? powers.filter((p) => p <= power).length / powers.length : null,
  };
}

// ---- holders and letters ---------------------------------------------------------------------------------------

const FOLAD_HOLDERS: [string, number][] = [
  ['سازمان توسعه و نوسازی معادن و صنایع معدنی ایران', 16.709],
  ['شرکت توسعه سرمایه رفاه -سهامی خاص-', 10.519],
  ['موسسه صندوق بازنشستگی، وظیفه، ازکارافتادگی و پس‌انداز کارکنان بانک‌ها', 3.56],
  ['موسسه صندوق بیمه اجتماعی روستائیان و عشایر', 2.686],
  ['شرکت سرمایه‌گذاری صدرتامین -سهامی عام-', 2.328],
  ['شرکت بهسازان انرژی تدبیر زنگان -سهامی خاص-', 1.891],
  ['شرکت سرمایه‌گذاری استان اصفهان -سهامی عام-', 1.574],
  ['شرکت گروه مالی ملت -سهامی عام-', 1.203],
];

const HOLDER_NAMES = [
  'شرکت سرمایه‌گذاری تأمین اجتماعی',
  'صندوق بازنشستگی کشوری',
  'شرکت سرمایه‌گذاری غدیر',
  'شرکت سرمایه‌گذاری صدرتامین',
  'موسسه صندوق حمایت و بازنشستگی کارکنان فولاد',
  'شرکت گروه توسعه مالی مهرآیندگان',
  'BFM-صندوق سرمایه‌گذاری اختصاصی بازارگردانی مفید',
  'شرکت سرمایه‌گذاری استان تهران',
  'بانک ملی ایران',
  'شرکت سرمایه‌گذاری پارس آریان',
];

export function shareholders(insCode: string): ShareholderDto[] {
  const inst = getUniverse().byCode.get(insCode);
  if (!inst || inst.shares <= 0) return [];
  const rng = rngFor('holders' + insCode);
  const snapshotDate = '2026-10-15';
  const rows: [string, number][] =
    insCode === '46348559193224090'
      ? FOLAD_HOLDERS
      : Array.from({ length: int(rng, 4, 8) }, (_, k) => [HOLDER_NAMES[(k + int(rng, 0, 9)) % HOLDER_NAMES.length], Math.round(lognormal(rng, 5, 0.8) * 1000) / 1000] as [string, number])
          .filter((r, k, all) => all.findIndex((x) => x[0] === r[0]) === k)
          .sort((a, b) => b[1] - a[1]);
  return rows.map(([holderName, percent], k) => {
    const changeAmount = k % 3 === 1 ? Math.round(inst.shares * uniform(rng, -0.002, 0.003)) : 0;
    return { snapshotDate, holderName, shares: Math.round((inst.shares * percent) / 100), percent, change: changeAmount === 0 ? 1 : changeAmount > 0 ? 2 : 3, changeAmount };
  });
}

function jalaliText(iso: string): string {
  const j = jalali(iso);
  const p = (x: number) => String(x).padStart(2, '0');
  return `${j.year}/${p(j.month)}/${p(j.day)}`;
}

const persianDigits = (s: string) => s.replace(/\d/g, (d) => '۰۱۲۳۴۵۶۷۸۹'[Number(d)]);

export function letters(insCode: string, take: number): CodalLetterDto[] {
  const inst = getUniverse().byCode.get(insCode);
  if (!inst || !(EQUITY_CLASSES.includes(inst.cls) || FUND_CLASSES.includes(inst.cls))) return [];
  const rng = rngFor('letters' + insCode);
  const out: CodalLetterDto[] = [];
  let tracing = 1605314 - (Number(insCode.slice(-3)) % 500) * 37;
  for (let k = 0; k < 40; k++) {
    const day = addDays(SESSION, -Math.round(k * 9.5 + uniform(rng, 0, 6)));
    const monthEnd = addDays(day, -int(rng, 6, 12));
    const kind = k % 6;
    const title =
      kind === 0 || kind === 3
        ? `گزارش فعالیت ماهانه دوره ۱ ماهه منتهی به ${persianDigits(jalaliText(monthEnd))}`
        : kind === 1
          ? `اطلاعات و صورت‌های مالی میاندوره‌ای دوره ۳ ماهه منتهی به ${persianDigits(jalaliText(addDays(monthEnd, -20)))} (حسابرسی نشده)`
          : kind === 2
            ? 'آگهی دعوت به مجمع عمومی عادی به‌طور فوق‌العاده'
            : kind === 4
              ? 'تصمیمات مجمع عمومی عادی سالیانه'
              : 'افشای اطلاعات بااهمیت - تمدید قرارداد فروش محصولات';
    const code = kind === 0 || kind === 3 ? 'ن-۳۰' : kind === 1 ? 'ن-۱۰' : kind === 2 ? 'ن-۴۱' : kind === 4 ? 'ن-۵۰' : 'ن-۲۶';
    const serial = Math.floor(rng() * 1e12).toString(36);
    const hasExcel = kind === 0 || kind === 1 || kind === 3;
    const hour = int(rng, 8, 22);
    out.push({
      tracingNo: String(tracing),
      title,
      letterCode: code,
      letterTypeId: kind === 0 || kind === 3 ? 58 : kind === 1 ? 6 : kind === 2 ? 20 : kind === 4 ? 21 : 13,
      publishedAt: `${day}T${String(hour).padStart(2, '0')}:${String(int(rng, 0, 59)).padStart(2, '0')}:00+03:30`,
      disclosureGroup: kind === 5 ? 'گروه ب' : null,
      hasExcel,
      reportUrl: `https://codal.ir/Reports/Decision.aspx?LetterSerial=${serial}&rt=0&let=${kind}&ct=0&ft=-1`,
      excelUrl: hasExcel ? `https://excel.codal.ir/service/Excel/GetAll/${serial}/0` : null,
      pdfUrl: `https://codal.ir/DownloadFile.aspx?hs=${serial}&ft=1005&let=58`,
    });
    tracing -= int(rng, 900, 4000);
  }
  return out.slice(0, take);
}

// ---- index and macro series --------------------------------------------------------------------------------------

const seriesCache = new Map<string, SeriesPointDto[]>();

function cached(key: string, build: () => SeriesPointDto[]): SeriesPointDto[] {
  let s = seriesCache.get(key);
  if (!s) {
    s = build();
    seriesCache.set(key, s);
  }
  return s;
}

function indexSeries(key: string, anchors: [string, number][], sigma: number, last: number, lastChange: number): SeriesPointDto[] {
  return cached(key, () => {
    const days = allTradingDays();
    const previous = last / (1 + lastChange / 100);
    const values = anchoredSeries(days.slice(0, -1), [...anchors, [days[days.length - 2], previous]], sigma, rngFor(key));
    return [...days.slice(0, -1).map((date, i) => ({ date, value: Math.round(values[i] * 10) / 10 })), { date: days[days.length - 1], value: last }];
  });
}

export function indexHistory(insCode: string): SeriesPointDto[] | null {
  if (insCode === '32097828799138957')
    return indexSeries(
      'tedpix',
      [['2019-03-23', 178000], ['2020-08-09', 2070000], ['2021-03-01', 1180000], ['2023-05-01', 2450000], ['2024-10-01', 1980000], ['2025-10-15', 4210000]],
      0.011,
      8265157,
      -1.91,
    );
  if (insCode === '67130298613737946')
    return indexSeries('equal', [['2019-03-23', 52000], ['2020-08-09', 620000], ['2021-03-01', 410000], ['2023-05-01', 880000], ['2024-10-01', 690000], ['2025-10-15', 1420000]], 0.009, 2654321, -1.42);
  if (insCode === '43685683301327984')
    return indexSeries('ifb', [['2019-03-23', 2300], ['2020-08-09', 25600], ['2021-03-01', 17400], ['2023-05-01', 31200], ['2024-10-01', 24800], ['2025-10-15', 51200]], 0.012, 96430, -1.18);
  return null;
}

function macroSeries(key: string, anchors: [string, number][], sigma: number, decimals = 0): SeriesPointDto[] {
  return cached(key, () => {
    const days = quoteDays(anchors[0][0], SESSION);
    const values = anchoredSeries(days, anchors, sigma, rngFor(key));
    const f = 10 ** decimals;
    return days.map((date, i) => ({ date, value: Math.round(values[i] * f) / f }));
  });
}

export function macroHistory(series: string): SeriesPointDto[] | null {
  switch (series) {
    case 'price_dollar_rl':
      return macroSeries(
        'usd',
        [['2019-03-23', 135000], ['2020-10-15', 318000], ['2021-06-01', 236000], ['2022-11-01', 358000], ['2023-03-01', 528000], ['2024-01-01', 518000], ['2025-03-01', 925000], ['2025-10-15', 1118800], ['2026-10-14', 2613700], ['2026-10-15', 2674050], ['2026-10-17', 2714431]],
        0.006,
      );
    case 'sekee':
      return macroSeries('coin', [['2019-03-23', 46000000], ['2020-10-15', 165000000], ['2022-11-01', 182000000], ['2024-01-01', 335000000], ['2025-10-15', 1070000000], ['2026-10-15', 2590000000], ['2026-10-17', 2612000000]], 0.007);
    case 'geram18':
      return macroSeries('gram', [['2019-03-23', 4800000], ['2022-11-01', 17200000], ['2025-10-15', 109000000], ['2026-10-15', 265100000], ['2026-10-17', 267400000]], 0.007);
    case 'ons':
      return macroSeries('ons', [['2019-03-23', 1310], ['2020-08-07', 2050], ['2022-11-01', 1630], ['2024-10-01', 2650], ['2025-10-15', 4180], ['2026-10-17', 3850]], 0.008, 2);
    case 'price_eur':
      return macroSeries('eur', [['2019-03-23', 150000], ['2022-11-01', 352000], ['2025-10-15', 1300000], ['2026-10-17', 3125000]], 0.006);
    default:
      return null;
  }
}

export function quote(series: string): { value: number; changePercent: number; date: string } {
  const s = macroHistory(series)!;
  const last = s[s.length - 1];
  const previous = s[s.length - 2];
  return { value: last.value, changePercent: Math.round((last.value / previous.value - 1) * 10000) / 100, date: last.date };
}

/** TEDPIX daily returns by date (for correlated breadth). */
export function tedpixReturns(): Map<string, number> {
  const s = indexHistory('32097828799138957')!;
  const out = new Map<string, number>();
  for (let i = 1; i < s.length; i++) out.set(s[i].date, s[i].value / s[i - 1].value - 1);
  return out;
}

export { isoToUtc };
