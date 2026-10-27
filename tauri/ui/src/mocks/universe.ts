// The mock instrument universe (~3,100 listings) and the latest session's snapshot. Headline numbers match the live
// sample in docs/development-plan.md §9.3: TEDPIX 8,265,157 (−1.91%), 146 up / 604 down, 30.8 hemat traded, 71 buy
// queues (2.19 hemat), 143 sell queues (1.59 hemat), individual net flow −6.8 hemat.
import type { BreadthDayDto, MarketOverviewDto, MarketWatchEntryDto, SectorPerformanceDto } from '../api/types';
import { SESSION, allTradingDays } from './calendar';
import { clamp, insCodeFrom, int, lognormal, mulberry32, normal, pick, rngFor, shuffle, uniform, type Rng } from './rng';

export type MockClass =
  | 'TseStock'
  | 'IfbStock'
  | 'IfbBaseYellow'
  | 'IfbBaseOrange'
  | 'IfbBaseRed'
  | 'Rights'
  | 'EtfEquity'
  | 'EtfFixedIncome'
  | 'EtfRealEstate'
  | 'EtfMixed'
  | 'FundLeveraged'
  | 'FundGoldSilver'
  | 'FundOfFunds'
  | 'Option'
  | 'Debt';

export const EQUITY_CLASSES: readonly MockClass[] = ['TseStock', 'IfbStock', 'IfbBaseYellow', 'IfbBaseOrange', 'IfbBaseRed'];
export const FUND_CLASSES: readonly MockClass[] = ['EtfEquity', 'EtfFixedIncome', 'EtfRealEstate', 'EtfMixed', 'FundLeveraged', 'FundGoldSilver', 'FundOfFunds'];

export interface MockInstrument {
  insCode: string;
  symbol: string;
  name: string;
  cls: MockClass;
  sector: string;
  shares: number;
  /** Median daily value traded, rials. */
  liquidity: number;
  /** Daily price limit in percent. */
  limit: number;
  /** First trading day. */
  listed: string;
  /** Target one-year adjusted return (fraction) for the generated history. */
  oneYear: number;
  parentInsCode: string | null;
}

export interface MockSnapshot {
  entry: MarketWatchEntryDto;
  yesterday: number;
  open: number;
  low: number;
  high: number;
  buyIndividualVolume: number;
  sellIndividualVolume: number;
  buyIndividualCount: number;
  sellIndividualCount: number;
  hasFlows: boolean;
}

export const SECTOR_NAMES: Record<string, string> = {
  '01': 'زراعت و خدمات وابسته',
  '10': 'استخراج زغال‌سنگ',
  '11': 'استخراج نفت، گاز و خدمات جنبی',
  '13': 'استخراج کانه‌های فلزی',
  '14': 'استخراج سایر معادن',
  '17': 'منسوجات',
  '19': 'محصولات چرمی',
  '20': 'محصولات چوبی',
  '21': 'محصولات کاغذی',
  '22': 'انتشار، چاپ و تکثیر',
  '23': 'فراورده‌های نفتی، کک و سوخت هسته‌ای',
  '25': 'لاستیک و پلاستیک',
  '26': 'محصولات رایانه‌ای، الکترونیکی و نوری',
  '27': 'فلزات اساسی',
  '28': 'ساخت محصولات فلزی',
  '29': 'ماشین‌آلات و تجهیزات',
  '31': 'ماشین‌آلات و دستگاه‌های برقی',
  '32': 'دستگاه‌ها و وسایل ارتباطی',
  '33': 'ابزار پزشکی، اپتیکی و اندازه‌گیری',
  '34': 'خودرو و ساخت قطعات',
  '35': 'سایر تجهیزات حمل‌ونقل',
  '36': 'مبلمان و مصنوعات دیگر',
  '38': 'قند و شکر',
  '39': 'شرکت‌های چندرشته‌ای صنعتی',
  '40': 'عرضهٔ برق، گاز، بخار و آب گرم',
  '42': 'محصولات غذایی و آشامیدنی',
  '43': 'مواد و محصولات دارویی',
  '44': 'محصولات شیمیایی',
  '45': 'پیمانکاری صنعتی',
  '46': 'تجارت عمده‌فروشی',
  '47': 'خرده‌فروشی',
  '49': 'کاشی و سرامیک',
  '53': 'سیمان، آهک و گچ',
  '54': 'سایر محصولات کانی غیرفلزی',
  '55': 'هتل و رستوران',
  '56': 'سرمایه‌گذاری‌ها',
  '57': 'بانک‌ها و مؤسسات اعتباری',
  '58': 'سایر واسطه‌گری‌های مالی',
  '60': 'حمل‌ونقل، انبارداری و ارتباطات',
  '64': 'مخابرات',
  '65': 'واسطه‌گری‌های مالی و پولی',
  '66': 'بیمه و صندوق بازنشستگی',
  '67': 'فعالیت‌های کمکی به نهادهای مالی',
  '68': 'صندوق سرمایه‌گذاری قابل معامله',
  '69': 'اوراق بهادار مبتنی بر دارایی',
  '70': 'انبوه‌سازی، املاک و مستغلات',
  '72': 'رایانه و فعالیت‌های وابسته',
  '73': 'اطلاعات و ارتباطات',
  '74': 'خدمات فنی و مهندسی',
  '77': 'فعالیت‌های اداری و پشتیبانی',
};

const INDUSTRY: Record<string, string> = {
  '01': 'کشت و صنعت',
  '10': 'زغال‌سنگ',
  '11': 'نفت و گاز',
  '13': 'معدنی',
  '14': 'معادن',
  '17': 'نساجی',
  '19': 'چرم',
  '20': 'صنایع چوب',
  '21': 'کاغذسازی',
  '22': 'چاپ و نشر',
  '23': 'پالایش نفت',
  '25': 'لاستیک',
  '26': 'الکترونیک',
  '27': 'فولاد',
  '28': 'صنایع فلزی',
  '29': 'ماشین‌سازی',
  '31': 'صنایع برق',
  '32': 'تجهیزات ارتباطی',
  '33': 'تجهیزات پزشکی',
  '34': 'قطعه‌سازی',
  '35': 'واگن‌سازی',
  '36': 'مبلمان',
  '38': 'قند',
  '39': 'صنعتی',
  '40': 'تولید نیرو',
  '42': 'صنایع غذایی',
  '43': 'داروسازی',
  '44': 'پتروشیمی',
  '45': 'پیمانکاری',
  '46': 'بازرگانی',
  '47': 'فروشگاه‌های زنجیره‌ای',
  '49': 'کاشی',
  '53': 'سیمان',
  '54': 'شیشه',
  '55': 'هتل‌داری',
  '56': 'سرمایه‌گذاری',
  '57': 'بانک',
  '58': 'لیزینگ',
  '60': 'حمل‌ونقل',
  '64': 'مخابرات',
  '65': 'تأمین سرمایه',
  '66': 'بیمه',
  '67': 'کارگزاری',
  '70': 'ساختمان',
  '72': 'رایانه',
  '73': 'فناوری اطلاعات',
  '74': 'مهندسی',
  '77': 'خدمات',
};

const PLACES = ['تهران', 'اصفهان', 'خراسان', 'فارس', 'کرمان', 'تبریز', 'قزوین', 'البرز', 'مازندران', 'خوزستان', 'سپاهان', 'ارومیه', 'همدان', 'زنجان', 'کردستان', 'گیلان', 'یزد', 'شمال', 'جنوب', 'غرب', 'شرق', 'ایرانیان', 'پارس', 'آریا', 'دماوند', 'سهند', 'زاگرس', 'نوین', 'سینا', 'امید', 'پاسارگاد', 'کاوه', 'سبلان', 'خلیج', 'هرمز', 'قشم', 'کیش', 'لرستان', 'سمنان', 'بوشهر'];

// Sector weights for the synthetic equities (how many listings each industry has).
const SECTOR_WEIGHTS: [string, number][] = [
  ['44', 9], ['27', 6], ['57', 4], ['56', 7], ['43', 6], ['53', 5], ['42', 6], ['34', 6], ['23', 3], ['13', 3], ['14', 2], ['25', 3], ['28', 3], ['29', 3],
  ['31', 3], ['38', 2], ['39', 3], ['40', 2], ['45', 2], ['46', 2], ['47', 1], ['49', 2], ['54', 3], ['55', 1], ['58', 2], ['60', 2], ['64', 1], ['65', 1],
  ['66', 3], ['67', 2], ['70', 3], ['72', 3], ['73', 2], ['74', 2], ['01', 3], ['10', 1], ['11', 1], ['17', 2], ['19', 1], ['20', 1], ['21', 2], ['22', 1],
  ['26', 1], ['32', 1], ['33', 1], ['35', 1], ['36', 1], ['77', 1],
];

interface Named {
  insCode: string;
  symbol: string;
  name: string;
  cls: MockClass;
  sector: string;
  close: number;
  shares: number;
  liquidity: number;
  oneYear: number;
}

// Real symbols with plausible (not real) figures. فولاد's InsCode and price level come from the golden files.
const NAMED: Named[] = [
  { insCode: '46348559193224090', symbol: 'فولاد', name: 'فولاد مبارکه اصفهان', cls: 'TseStock', sector: '27', close: 4410, shares: 1.935e12, liquidity: 6.5e12, oneYear: 1.14 },
  { insCode: '35425587644337450', symbol: 'فملی', name: 'ملی صنایع مس ایران', cls: 'TseStock', sector: '27', close: 7850, shares: 5.7e11, liquidity: 4.8e12, oneYear: 0.92 },
  { insCode: '7745894403636165', symbol: 'شپنا', name: 'پالایش نفت اصفهان', cls: 'TseStock', sector: '23', close: 9120, shares: 3.4e11, liquidity: 3.1e12, oneYear: 0.78 },
  { insCode: '65883838195688438', symbol: 'خودرو', name: 'ایران‌خودرو', cls: 'TseStock', sector: '34', close: 3250, shares: 4.3e11, liquidity: 4.2e12, oneYear: 0.55 },
  { insCode: '44891482026867833', symbol: 'خساپا', name: 'سایپا', cls: 'TseStock', sector: '34', close: 2410, shares: 4.9e11, liquidity: 2.6e12, oneYear: 0.41 },
  { insCode: '778253364357513', symbol: 'وبملت', name: 'بانک ملت', cls: 'TseStock', sector: '57', close: 3980, shares: 1.06e12, liquidity: 3.4e12, oneYear: 1.05 },
  { insCode: '63917421733088077', symbol: 'وتجارت', name: 'بانک تجارت', cls: 'TseStock', sector: '57', close: 2140, shares: 9.6e11, liquidity: 2.2e12, oneYear: 0.88 },
  { insCode: '2400322364771558', symbol: 'شستا', name: 'سرمایه‌گذاری تأمین اجتماعی', cls: 'TseStock', sector: '56', close: 1520, shares: 2.4e12, liquidity: 3.0e12, oneYear: 0.66 },
  { insCode: '33808206014018431', symbol: 'فارس', name: 'صنایع پتروشیمی خلیج فارس', cls: 'TseStock', sector: '44', close: 16800, shares: 4.0e11, liquidity: 2.9e12, oneYear: 0.71 },
  { insCode: '35700344742885862', symbol: 'کگل', name: 'معدنی و صنعتی گل‌گهر', cls: 'TseStock', sector: '13', close: 9450, shares: 3.6e11, liquidity: 1.9e12, oneYear: 0.84 },
  { insCode: '18027801615184692', symbol: 'کچاد', name: 'معدنی و صنعتی چادرملو', cls: 'TseStock', sector: '13', close: 11300, shares: 2.2e11, liquidity: 1.4e12, oneYear: 0.62 },
  { insCode: '22811176775480091', symbol: 'شبندر', name: 'پالایش نفت بندرعباس', cls: 'TseStock', sector: '23', close: 12600, shares: 2.5e11, liquidity: 1.6e12, oneYear: 0.73 },
  { insCode: '22560050433388046', symbol: 'شتران', name: 'پالایش نفت تهران', cls: 'TseStock', sector: '23', close: 4980, shares: 3.0e11, liquidity: 1.3e12, oneYear: 0.69 },
  { insCode: '23214828924506640', symbol: 'اخابر', name: 'مخابرات ایران', cls: 'TseStock', sector: '64', close: 6120, shares: 3.1e11, liquidity: 1.1e12, oneYear: 0.37 },
  { insCode: '46752599569017089', symbol: 'همراه', name: 'ارتباطات سیار ایران', cls: 'TseStock', sector: '64', close: 12950, shares: 1.6e11, liquidity: 0.9e12, oneYear: 0.29 },
  { insCode: '6131290133202745', symbol: 'پارس', name: 'پتروشیمی پارس', cls: 'TseStock', sector: '44', close: 168000, shares: 3.0e10, liquidity: 0.8e12, oneYear: 0.58 },
  { insCode: '7183333492448248', symbol: 'نوری', name: 'پتروشیمی نوری', cls: 'TseStock', sector: '44', close: 205000, shares: 2.4e10, liquidity: 0.9e12, oneYear: 0.64 },
  { insCode: '43479730079120887', symbol: 'جم', name: 'پتروشیمی جم', cls: 'TseStock', sector: '44', close: 41800, shares: 6.1e10, liquidity: 0.7e12, oneYear: 0.51 },
  { insCode: '22299372452767046', symbol: 'تاپیکو', name: 'نفت و گاز و پتروشیمی تأمین', cls: 'TseStock', sector: '44', close: 15300, shares: 2.6e11, liquidity: 1.2e12, oneYear: 0.6 },
  { insCode: '67126881188552864', symbol: 'رمپنا', name: 'گروه مپنا', cls: 'TseStock', sector: '74', close: 18400, shares: 1.0e11, liquidity: 1.0e12, oneYear: 0.95 },
  { insCode: '41302407127223434', symbol: 'ذوب', name: 'ذوب‌آهن اصفهان', cls: 'TseStock', sector: '27', close: 3050, shares: 4.7e11, liquidity: 1.5e12, oneYear: 0.81 },
  { insCode: '28320293733348826', symbol: 'فخوز', name: 'فولاد خوزستان', cls: 'TseStock', sector: '27', close: 6700, shares: 2.4e11, liquidity: 1.2e12, oneYear: 0.99 },
  { insCode: '3863538898378476', symbol: 'کاوه', name: 'فولاد کاوه جنوب کیش', cls: 'TseStock', sector: '27', close: 9800, shares: 1.0e11, liquidity: 0.6e12, oneYear: 0.77 },
  { insCode: '9536587154100457', symbol: 'ومعادن', name: 'سرمایه‌گذاری توسعه معادن و فلزات', cls: 'TseStock', sector: '56', close: 9900, shares: 1.6e11, liquidity: 0.8e12, oneYear: 0.7 },
  { insCode: '23441366113375722', symbol: 'وپارس', name: 'بانک پارسیان', cls: 'TseStock', sector: '57', close: 2350, shares: 7.0e11, liquidity: 1.1e12, oneYear: 0.49 },
  { insCode: '34144395039913458', symbol: 'وبصادر', name: 'بانک صادرات ایران', cls: 'TseStock', sector: '57', close: 1980, shares: 1.1e12, liquidity: 1.6e12, oneYear: 0.57 },
  { insCode: '25357135030606405', symbol: 'ونوین', name: 'بانک اقتصاد نوین', cls: 'TseStock', sector: '57', close: 3560, shares: 2.1e11, liquidity: 0.5e12, oneYear: 0.44 },
  { insCode: '26014913469567886', symbol: 'وغدیر', name: 'سرمایه‌گذاری غدیر', cls: 'TseStock', sector: '56', close: 6800, shares: 2.3e11, liquidity: 0.7e12, oneYear: 0.52 },
  { insCode: '9211775239375291', symbol: 'ستران', name: 'سیمان تهران', cls: 'TseStock', sector: '53', close: 7200, shares: 2.9e10, liquidity: 0.3e12, oneYear: 0.61 },
  { insCode: '48990026850202503', symbol: 'سفارس', name: 'سیمان فارس و خوزستان', cls: 'TseStock', sector: '53', close: 8400, shares: 6.0e10, liquidity: 0.4e12, oneYear: 0.66 },
  { insCode: '17834623106317041', symbol: 'دجابر', name: 'داروسازی جابرابن‌حیان', cls: 'TseStock', sector: '43', close: 21000, shares: 1.2e10, liquidity: 0.2e12, oneYear: 0.39 },
  { insCode: '40411537531154482', symbol: 'غکورش', name: 'صنعت غذایی کورش', cls: 'TseStock', sector: '42', close: 6950, shares: 5.4e10, liquidity: 0.4e12, oneYear: 0.32 },
  { insCode: '35366681030756042', symbol: 'خگستر', name: 'گسترش سرمایه‌گذاری ایران‌خودرو', cls: 'TseStock', sector: '34', close: 4120, shares: 1.2e11, liquidity: 0.9e12, oneYear: 0.47 },
  { insCode: '59612098290740355', symbol: 'البرز', name: 'بیمه البرز', cls: 'TseStock', sector: '66', close: 3450, shares: 6.5e10, liquidity: 0.3e12, oneYear: 0.42 },
  { insCode: '17617474823279712', symbol: 'دی', name: 'بانک دی', cls: 'IfbStock', sector: '57', close: 1890, shares: 3.6e11, liquidity: 0.8e12, oneYear: 0.35 },
  { insCode: '20946530370469828', symbol: 'شپدیس', name: 'پتروشیمی پردیس', cls: 'IfbStock', sector: '44', close: 31500, shares: 6.0e10, liquidity: 0.9e12, oneYear: 0.69 },
  { insCode: '67988012428906654', symbol: 'مارون', name: 'پتروشیمی مارون', cls: 'IfbStock', sector: '44', close: 96000, shares: 1.0e10, liquidity: 0.3e12, oneYear: 0.48 },
  { insCode: '38568786927478796', symbol: 'زاگرس', name: 'پتروشیمی زاگرس', cls: 'IfbStock', sector: '44', close: 64000, shares: 1.7e10, liquidity: 0.3e12, oneYear: 0.53 },
  { insCode: '45174198424472334', symbol: 'آریا', name: 'پلیمر آریا ساسول', cls: 'IfbStock', sector: '44', close: 152000, shares: 7.0e9, liquidity: 0.2e12, oneYear: 0.44 },
  // Funds (sector 68)
  { insCode: '25559236668122210', symbol: 'اهرم', name: 'صندوق اهرمی کاریزما', cls: 'FundLeveraged', sector: '68', close: 41500, shares: 1.5e9, liquidity: 2.6e12, oneYear: 1.6 },
  { insCode: '34144395039913465', symbol: 'عیار', name: 'صندوق طلای عیار مفید', cls: 'FundGoldSilver', sector: '68', close: 92400, shares: 1.9e9, liquidity: 3.1e12, oneYear: 1.35 },
  { insCode: '46700660505281786', symbol: 'طلا', name: 'صندوق طلای لوتوس', cls: 'FundGoldSilver', sector: '68', close: 64200, shares: 2.1e9, liquidity: 2.2e12, oneYear: 1.33 },
  { insCode: '51617145873056483', symbol: 'کمند', name: 'صندوق درآمد ثابت کمند', cls: 'EtfFixedIncome', sector: '68', close: 31800, shares: 6.0e9, liquidity: 1.4e12, oneYear: 0.29 },
  { insCode: '62235397452612911', symbol: 'اطلس', name: 'صندوق سهامی اطلس مفید', cls: 'EtfEquity', sector: '68', close: 18500, shares: 3.0e9, liquidity: 0.7e12, oneYear: 0.71 },
  { insCode: '13227061779734125', symbol: 'آگاس', name: 'صندوق سهامی آگاه', cls: 'EtfEquity', sector: '68', close: 21800, shares: 2.4e9, liquidity: 0.5e12, oneYear: 0.66 },
  { insCode: '56871139881800017', symbol: 'توسکا', name: 'صندوق سرمایه‌گذاری توسعه فولاد ثابت', cls: 'EtfFixedIncome', sector: '68', close: 26100, shares: 1.8e9, liquidity: 0.3e12, oneYear: 0.28 },
  { insCode: '14231831499205396', symbol: 'پادا', name: 'صندوق صندوق‌های پاداش', cls: 'FundOfFunds', sector: '68', close: 15400, shares: 9.0e8, liquidity: 0.1e12, oneYear: 0.62 },
  { insCode: '27299841173245405', symbol: 'املاک', name: 'صندوق املاک و مستغلات ایرانیان', cls: 'EtfRealEstate', sector: '68', close: 12900, shares: 8.0e8, liquidity: 0.1e12, oneYear: 0.85 },
  { insCode: '40505767761868155', symbol: 'نهال', name: 'صندوق مختلط نهال سرمایه ایرانیان', cls: 'EtfMixed', sector: '68', close: 14700, shares: 7.0e8, liquidity: 0.08e12, oneYear: 0.46 },
];

const LETTERS = 'ابپتجحخدرزسشصطعغفقکگلمنوهی';

function synthSymbol(rng: Rng, taken: Set<string>): string {
  for (;;) {
    const length = int(rng, 3, 5);
    let s = '';
    for (let i = 0; i < length; i++) s += pick(rng, LETTERS.split(''));
    if (!taken.has(s)) {
      taken.add(s);
      return s;
    }
  }
}

function weightedSector(rng: Rng): string {
  const total = SECTOR_WEIGHTS.reduce((s, [, w]) => s + w, 0);
  let x = rng() * total;
  for (const [code, w] of SECTOR_WEIGHTS) {
    x -= w;
    if (x <= 0) return code;
  }
  return '44';
}

function limitFor(cls: MockClass): number {
  if (cls === 'IfbBaseYellow' || cls === 'IfbBaseOrange' || cls === 'IfbBaseRed') return 5;
  if (FUND_CLASSES.includes(cls)) return 5;
  if (cls === 'Option') return 15;
  if (cls === 'Debt') return 3;
  return 3;
}

interface Universe {
  instruments: MockInstrument[];
  byCode: Map<string, MockInstrument>;
  snapshot: Map<string, MockSnapshot>;
  entries: MarketWatchEntryDto[];
  /** Session close per instrument (bars end here). */
  closes: Map<string, number>;
}

let universe: Universe | null = null;

const roundPrice = (p: number) => (p >= 10000 ? Math.round(p / 10) * 10 : Math.round(p));

export function getUniverse(): Universe {
  if (universe) return universe;
  const rng = mulberry32(14050725);
  const taken = new Set(NAMED.map((n) => n.symbol));
  const instruments: MockInstrument[] = [];
  const closes = new Map<string, number>();
  const days = allTradingDays();

  const add = (i: MockInstrument, close: number) => {
    instruments.push(i);
    closes.set(i.insCode, close);
  };

  for (const n of NAMED) {
    add({ insCode: n.insCode, symbol: n.symbol, name: n.name, cls: n.cls, sector: n.sector, shares: n.shares, liquidity: n.liquidity, limit: limitFor(n.cls), listed: days[0], oneYear: n.oneYear, parentInsCode: null }, n.close);
  }

  const synth = (cls: MockClass, count: number, sector: () => string, naming: (sector: string) => string, price: () => number, shares: () => number, liquidity: () => number) => {
    for (let k = 0; k < count; k++) {
      const s = sector();
      const listedIndex = rng() < 0.25 ? int(rng, 200, days.length - 60) : 0;
      add(
        {
          insCode: insCodeFrom(rng),
          symbol: synthSymbol(rng, taken),
          name: naming(s),
          cls,
          sector: s,
          shares: shares(),
          liquidity: liquidity(),
          limit: limitFor(cls),
          listed: days[listedIndex],
          oneYear: clamp(normal(rng, 0.45, 0.45), -0.5, 2.2),
          parentInsCode: null,
        },
        roundPrice(price()),
      );
    }
  };

  const companyName = (s: string) => `${INDUSTRY[s] ?? 'صنایع'} ${pick(rng, PLACES)}${rng() < 0.3 ? ' ' + pick(rng, PLACES) : ''}`;
  const equityPrice = () => clamp(lognormal(rng, 7000, 0.9), 600, 400000);
  const equityShares = () => clamp(lognormal(rng, 8e9, 1.2), 2e8, 3e11);
  const equityLiquidity = () => clamp(lognormal(rng, 1.4e11, 1.3), 2e8, 3e12);

  const namedEquities = NAMED.filter((n) => EQUITY_CLASSES.includes(n.cls)).length;
  synth('TseStock', 400 - NAMED.filter((n) => n.cls === 'TseStock').length, () => weightedSector(rng), companyName, equityPrice, equityShares, equityLiquidity);
  synth('IfbStock', 290 - NAMED.filter((n) => n.cls === 'IfbStock').length, () => weightedSector(rng), companyName, equityPrice, equityShares, equityLiquidity);
  synth('IfbBaseYellow', 92, () => weightedSector(rng), companyName, equityPrice, () => equityShares() / 4, () => equityLiquidity() / 6);
  synth('IfbBaseOrange', 44, () => weightedSector(rng), companyName, equityPrice, () => equityShares() / 4, () => equityLiquidity() / 8);
  synth('IfbBaseRed', 24, () => weightedSector(rng), companyName, equityPrice, () => equityShares() / 4, () => equityLiquidity() / 10);
  void namedEquities;

  const fundNames: Record<string, string> = {
    EtfEquity: 'صندوق سهامی',
    EtfFixedIncome: 'صندوق درآمد ثابت',
    EtfRealEstate: 'صندوق املاک',
    EtfMixed: 'صندوق مختلط',
    FundLeveraged: 'صندوق اهرمی',
    FundGoldSilver: 'صندوق طلای',
    FundOfFunds: 'صندوق در صندوق',
  };
  const fundCounts: [MockClass, number][] = [
    ['EtfEquity', 150],
    ['EtfFixedIncome', 210],
    ['EtfRealEstate', 14],
    ['EtfMixed', 50],
    ['FundLeveraged', 26],
    ['FundGoldSilver', 34],
    ['FundOfFunds', 18],
  ];
  for (const [cls, count] of fundCounts) {
    synth(
      cls,
      count,
      () => '68',
      () => `${fundNames[cls]} ${pick(rng, PLACES)} ${pick(rng, ['مفید', 'آگاه', 'کاریزما', 'تدبیر', 'سپهر', 'امین', 'یکم', 'نوین', 'الماس', 'فیروزه'])}`,
      () => clamp(lognormal(rng, 18000, 0.6), 9000, 160000),
      () => lognormal(rng, 6e8, 1),
      () => clamp(lognormal(rng, 2.5e10, 1.4), 1e8, 2e12),
    );
  }

  // Rights (حق تقدم) of some equities.
  const equities = instruments.filter((i) => EQUITY_CLASSES.includes(i.cls));
  for (const parent of shuffle(rngFor('rights'), equities.slice()).slice(0, 110)) {
    const parentClose = closes.get(parent.insCode) ?? 5000;
    let symbol = parent.symbol + 'ح';
    if (taken.has(symbol)) symbol += '1';
    taken.add(symbol);
    add(
      {
        insCode: insCodeFrom(rng),
        symbol,
        name: `ح . ${parent.name}`,
        cls: 'Rights',
        sector: parent.sector,
        shares: parent.shares * uniform(rng, 0.2, 1),
        liquidity: parent.liquidity * uniform(rng, 0.05, 0.4),
        limit: 3,
        listed: days[days.length - int(rng, 20, 90)],
        oneYear: 0,
        parentInsCode: null,
      },
      roundPrice(Math.max(100, parentClose - 1000) * uniform(rng, 0.8, 1.05)),
    );
  }

  // Options on the most liquid names.
  const underlyings = equities.slice().sort((a, b) => b.liquidity - a.liquidity).slice(0, 70);
  let optionCount = 0;
  for (const u of underlyings) {
    const uClose = closes.get(u.insCode) ?? 5000;
    for (let k = 0; k < 20 && optionCount < 1390; k++, optionCount++) {
      const call = k % 2 === 0;
      const strike = roundPrice(uClose * (0.7 + 0.05 * Math.floor(k / 2)));
      const month = ['1405/08/10', '1405/09/14', '1405/10/12', '1405/11/09'][k % 4];
      let symbol = `${call ? 'ض' : 'ط'}${u.symbol.slice(0, 4)}${String(8000 + optionCount).slice(-4)}`;
      while (taken.has(symbol)) symbol += '1';
      taken.add(symbol);
      add(
        {
          insCode: insCodeFrom(rng),
          symbol,
          name: `${call ? 'اختیارخ' : 'اختیارف'} ${u.symbol}-${strike}-${month}`,
          cls: 'Option',
          sector: u.sector,
          shares: 0,
          liquidity: clamp(lognormal(rng, 6e9, 1.6), 1e7, 6e11),
          limit: 15,
          listed: days[days.length - int(rng, 30, 160)],
          oneYear: 0,
          parentInsCode: null,
        },
        Math.max(1, roundPrice(Math.max(uClose - strike, uClose * 0.02) * uniform(rng, 0.8, 1.3))),
      );
    }
  }

  // Treasury bills and corporate sukuk.
  for (let k = 0; k < 250; k++) {
    const treasury = k < 140;
    let symbol = treasury ? `اخزا${String(301 + k).padStart(3, '0')}` : `صکوک${pick(rng, ['مرابحه', 'اجاره', 'منفعت'])}${k}`;
    while (taken.has(symbol)) symbol += '1';
    taken.add(symbol);
    add(
      {
        insCode: insCodeFrom(rng),
        symbol,
        name: treasury ? `اسناد خزانه اسلامی ${1405 + (k % 4)}` : `صکوک ${pick(rng, PLACES)} ${1406 + (k % 5)}`,
        cls: 'Debt',
        sector: '69',
        shares: 0,
        liquidity: clamp(lognormal(rng, 3e10, 1.5), 1e7, 2e12),
        limit: 3,
        listed: days[days.length - int(rng, 60, 900)],
        oneYear: 0.3,
        parentInsCode: null,
      },
      roundPrice(clamp(lognormal(rng, 820000, 0.12), 500000, 1000000)),
    );
  }

  const byCode = new Map(instruments.map((i) => [i.insCode, i]));
  const snapshot = buildSnapshot(instruments, closes);
  universe = { instruments, byCode, snapshot, entries: instruments.map((i) => snapshot.get(i.insCode)!.entry), closes };
  return universe;
}

/** Equity day outcomes that reproduce the headline breadth of the live sample. */
const EQUITY_OUTCOMES = { buyQueue: 71, up: 146 - 71, unchanged: 38, sellQueue: 143, down: 604 - 143, noTrade: 17, halted: 45 };
export const HEADLINE = {
  equityValue: 3.08e14,
  fundValue: 1.12e14,
  buyQueueValue: 2.19e13,
  sellQueueValue: 1.59e13,
  individualNetFlow: -6.8e13,
};

function buildSnapshot(instruments: MockInstrument[], closes: Map<string, number>): Map<string, MockSnapshot> {
  const rng = mulberry32(777);
  const out = new Map<string, MockSnapshot>();
  const equities = instruments.filter((i) => EQUITY_CLASSES.includes(i.cls));
  const order = shuffle(rng, equities.slice());
  const outcome = new Map<string, keyof typeof EQUITY_OUTCOMES>();
  let cursor = 0;
  for (const [kind, count] of Object.entries(EQUITY_OUTCOMES) as [keyof typeof EQUITY_OUTCOMES, number][]) {
    for (let k = 0; k < count && cursor < order.length; k++) outcome.set(order[cursor++].insCode, kind);
  }
  // Remaining equities (if any) trade down a little.
  for (; cursor < order.length; cursor++) outcome.set(order[cursor].insCode, 'down');
  // فولاد: down 2.88%, no queue (keeps the symbol page consistent with the docs' sample).
  outcome.set('46348559193224090', 'down');

  for (const i of instruments) {
    const close0 = closes.get(i.insCode)!;
    const kind = outcome.get(i.insCode);
    const isEquity = kind !== undefined;
    let changePct: number;
    let traded = true;
    let dayStatus: string | null = 'Traded';
    let queue: string | null = null;
    if (isEquity) {
      switch (kind) {
        case 'buyQueue':
          changePct = i.limit;
          queue = 'buy';
          dayStatus = rng() < 0.6 ? 'QueueLockedUp' : 'Traded';
          break;
        case 'up':
          changePct = uniform(rng, 0.15, i.limit - 0.1);
          break;
        case 'unchanged':
          changePct = 0;
          break;
        case 'sellQueue':
          changePct = -i.limit;
          queue = 'sell';
          dayStatus = rng() < 0.6 ? 'QueueLockedDown' : 'Traded';
          break;
        case 'noTrade':
          changePct = 0;
          traded = false;
          dayStatus = 'NoTrade';
          break;
        case 'halted':
          changePct = 0;
          traded = false;
          dayStatus = 'SymbolHalted';
          break;
        default:
          changePct = -uniform(rng, 0.2, i.limit - 0.1);
      }
      if (i.insCode === '46348559193224090') changePct = -2.88;
    } else if (FUND_CLASSES.includes(i.cls)) {
      changePct = i.cls === 'EtfFixedIncome' ? uniform(rng, -0.05, 0.12) : i.cls === 'FundGoldSilver' ? uniform(rng, 0.2, 1.4) : normal(rng, -1.2, 1.1);
      traded = rng() < 0.93;
    } else if (i.cls === 'Rights') {
      changePct = normal(rng, -2.5, 2.5);
      traded = rng() < 0.7;
    } else if (i.cls === 'Option') {
      changePct = normal(rng, -4, 6);
      traded = rng() < 0.22;
    } else {
      changePct = normal(rng, 0.08, 0.15);
      traded = rng() < 0.35;
    }
    changePct = clamp(changePct, -i.limit, i.limit);
    if (!traded) {
      changePct = 0;
      if (dayStatus === 'Traded') dayStatus = 'NoTrade';
    }

    const close = close0;
    const yesterday = close / (1 + changePct / 100);
    const allowedHigh = roundPrice(yesterday * (1 + i.limit / 100));
    const allowedLow = roundPrice(yesterday * (1 - i.limit / 100));
    const last = queue || !traded ? close : roundPrice(clamp(close * (1 + normal(rng, 0, 0.006)), allowedLow, allowedHigh));
    const value = traded ? i.liquidity * Math.exp(normal(rng, 0, 0.5)) * (1 + Math.abs(changePct) / i.limit) : 0;
    const volume = traded ? Math.max(1, Math.round(value / close)) : 0;
    const trades = traded ? Math.max(1, Math.round(value / lognormal(rng, 4e8, 0.6))) : 0;
    const hasFlows = traded && i.cls !== 'Debt' && !(i.cls === 'Option' && rng() < 0.5);
    const buyShare = clamp(normal(rng, 0.72 + changePct / 40, 0.12), 0.15, 1);
    const sellShare = clamp(normal(rng, 0.78 - changePct / 40, 0.12), 0.15, 1);
    const buyIndividualVolume = Math.round(volume * buyShare);
    const sellIndividualVolume = Math.round(volume * sellShare);
    const buyIndividualCount = Math.max(1, Math.round((buyIndividualVolume * close) / lognormal(rng, 1.1e8, 0.5)));
    const sellIndividualCount = Math.max(1, Math.round((sellIndividualVolume * close) / lognormal(rng, 1.4e8, 0.5)));
    const open = traded ? roundPrice(clamp(yesterday * (1 + normal(rng, changePct / 200, 0.008)), allowedLow, allowedHigh)) : close;
    const high = traded ? Math.max(open, close, last, queue === 'buy' ? close : roundPrice(Math.min(allowedHigh, Math.max(open, close) * (1 + Math.abs(normal(rng, 0, 0.006)))))) : close;
    const low = traded ? Math.min(open, close, last, queue === 'sell' ? close : roundPrice(Math.max(allowedLow, Math.min(open, close) * (1 - Math.abs(normal(rng, 0, 0.006)))))) : close;

    const entry: MarketWatchEntryDto = {
      insCode: i.insCode,
      symbol: i.symbol,
      name: i.name,
      instrumentClass: i.cls,
      sectorCode: i.sector,
      dayStatus,
      close,
      changePercent: traded || dayStatus === 'NoTrade' ? Math.round(changePct * 100) / 100 : null,
      last,
      lastChangePercent: traded ? Math.round((last / yesterday - 1) * 10000) / 100 : null,
      volume,
      value,
      trades,
      allowedLow,
      allowedHigh,
      queue,
      queueValue: queue ? lognormal(rng, 2.5e11, 0.9) : null,
      priceEarnings: isEquity ? (rng() < 0.08 ? null : Math.round(clamp(lognormal(rng, 7.5, 0.5), 1.5, 60) * 10) / 10) : null,
      marketCap: i.shares > 0 ? close * i.shares : null,
      individualNetValue: hasFlows ? (buyIndividualVolume - sellIndividualVolume) * close : null,
      perCapitaBuy: hasFlows ? (buyIndividualVolume * close) / buyIndividualCount : null,
      perCapitaSell: hasFlows ? (sellIndividualVolume * close) / sellIndividualCount : null,
      buyerPower: null,
      individualBuyShare: hasFlows && volume > 0 ? buyIndividualVolume / volume : null,
    };
    if (entry.perCapitaBuy !== null && entry.perCapitaSell) entry.buyerPower = Math.round((entry.perCapitaBuy / entry.perCapitaSell) * 100) / 100;
    out.set(i.insCode, { entry, yesterday, open, low, high, buyIndividualVolume, sellIndividualVolume, buyIndividualCount, sellIndividualCount, hasFlows });
  }

  // Scale equity value, fund value, queue values and individual flow to the headline totals.
  const snaps = [...out.values()];
  const eq = snaps.filter((s) => EQUITY_CLASSES.includes(s.entry.instrumentClass as MockClass));
  const funds = snaps.filter((s) => FUND_CLASSES.includes(s.entry.instrumentClass as MockClass));
  const scale = (list: MockSnapshot[], total: number) => {
    const sum = list.reduce((s, x) => s + x.entry.value, 0) || 1;
    const k = total / sum;
    for (const s of list) {
      s.entry.value *= k;
      s.entry.volume = s.entry.value > 0 ? Math.max(1, Math.round(s.entry.value / s.entry.close)) : 0;
      if (s.entry.individualNetValue !== null) s.entry.individualNetValue *= k;
      s.buyIndividualVolume = Math.round(s.buyIndividualVolume * k);
      s.sellIndividualVolume = Math.round(s.sellIndividualVolume * k);
    }
  };
  scale(eq, HEADLINE.equityValue);
  scale(funds, HEADLINE.fundValue);
  const scaleField = (list: MockSnapshot[], field: 'queueValue', total: number) => {
    const sum = list.reduce((s, x) => s + (x.entry[field] ?? 0), 0) || 1;
    for (const s of list) if (s.entry[field] !== null) s.entry[field] = (s.entry[field] as number) * (total / sum);
  };
  scaleField(eq.filter((s) => s.entry.queue === 'buy'), 'queueValue', HEADLINE.buyQueueValue);
  scaleField(eq.filter((s) => s.entry.queue === 'sell'), 'queueValue', HEADLINE.sellQueueValue);
  // Shift individual flows so their equity total is the headline figure (a crash day: retail selling).
  const flowing = eq.filter((s) => s.entry.individualNetValue !== null);
  const flowSum = flowing.reduce((s, x) => s + (x.entry.individualNetValue ?? 0), 0);
  const valueSum = flowing.reduce((s, x) => s + x.entry.value, 0) || 1;
  for (const s of flowing) {
    s.entry.individualNetValue = (s.entry.individualNetValue ?? 0) + ((HEADLINE.individualNetFlow - flowSum) * s.entry.value) / valueSum;
  }
  return out;
}

const INDEX_NAMES: Record<string, string> = {
  '32097828799138957': 'شاخص کل',
  '67130298613737946': 'شاخص کل (هم‌وزن)',
  '43685683301327984': 'شاخص کل فرابورس',
};

export function overview(usd: { value: number; changePercent: number; date: string }, coin: { value: number; changePercent: number; date: string }): MarketOverviewDto {
  const u = getUniverse();
  const eq = u.entries.filter((e) => EQUITY_CLASSES.includes(e.instrumentClass as MockClass));
  const flowing = eq.filter((e) => e.individualNetValue !== null && e.perCapitaBuy !== null);
  const buyValue = flowing.reduce((s, e) => s + Math.max(0, e.value * (e.individualBuyShare ?? 0)), 0);
  void buyValue;
  return {
    session: SESSION,
    indices: [
      { insCode: '32097828799138957', name: INDEX_NAMES['32097828799138957'], value: 8265157, changePercent: -1.91 },
      { insCode: '67130298613737946', name: INDEX_NAMES['67130298613737946'], value: 2654321, changePercent: -1.42 },
      { insCode: '43685683301327984', name: INDEX_NAMES['43685683301327984'], value: 96430, changePercent: -1.18 },
    ],
    equityValue: HEADLINE.equityValue,
    fundValue: HEADLINE.fundValue,
    equityTrades: eq.reduce((s, e) => s + e.trades, 0),
    advancers: 146,
    decliners: 604,
    unchanged: 38,
    notTraded: 17,
    halted: 45,
    buyQueues: 71,
    buyQueueValue: HEADLINE.buyQueueValue,
    sellQueues: 143,
    sellQueueValue: HEADLINE.sellQueueValue,
    individualNetFlow: HEADLINE.individualNetFlow,
    individualPerCapitaBuy: 84_600_000,
    individualPerCapitaSell: 121_300_000,
    flowValuesEstimated: true,
    usd: { series: 'price_dollar_rl', date: usd.date, value: usd.value, changePercent: usd.changePercent },
    coin: { series: 'sekee', date: coin.date, value: coin.value, changePercent: coin.changePercent },
    tedpixUsdChangePercent: -3.37,
  };
}

export function breadth(sessions: number, tedpixReturns: Map<string, number>): BreadthDayDto[] {
  const days = allTradingDays().slice(-sessions);
  return days.map((date, i) => {
    if (date === days[days.length - 1]) {
      return { date, advancers: 146, decliners: 604, unchanged: 38, notTraded: 17, halted: 45, lockedUp: 43, lockedDown: 86, equityValue: HEADLINE.equityValue };
    }
    const r = rngFor('breadth' + date);
    const ret = tedpixReturns.get(date) ?? 0;
    const total = 850;
    const halted = int(r, 25, 60);
    const notTraded = int(r, 8, 30);
    const unchanged = int(r, 20, 60);
    const active = total - halted - notTraded - unchanged;
    const upShare = clamp(0.5 + ret * 22 + normal(r, 0, 0.06), 0.05, 0.95);
    const advancers = Math.round(active * upShare);
    return {
      date,
      advancers,
      decliners: active - advancers,
      unchanged,
      notTraded,
      halted,
      lockedUp: Math.round(advancers * uniform(r, 0.05, 0.35)),
      lockedDown: Math.round((active - advancers) * uniform(r, 0.05, 0.3)),
      equityValue: lognormal(r, 2.2e14, 0.3) * (1 + Math.abs(ret) * 10) * (0.8 + (0.4 * i) / days.length),
    };
  });
}

export function sectors(): SectorPerformanceDto[] {
  const u = getUniverse();
  const groups = new Map<string, MarketWatchEntryDto[]>();
  for (const e of u.entries) {
    if (!EQUITY_CLASSES.includes(e.instrumentClass as MockClass) || !e.sectorCode) continue;
    const list = groups.get(e.sectorCode) ?? [];
    list.push(e);
    groups.set(e.sectorCode, list);
  }
  const out: SectorPerformanceDto[] = [];
  for (const [code, list] of groups) {
    const traded = list.filter((e) => e.trades > 0);
    const capSum = traded.reduce((s, e) => s + (e.marketCap ?? 0), 0);
    const weighted = capSum > 0 ? traded.reduce((s, e) => s + (e.marketCap ?? 0) * ((e.changePercent ?? 0) / 100), 0) / capSum : null;
    const returns = traded.map((e) => (e.changePercent ?? 0) / 100).sort((a, b) => a - b);
    const median = returns.length ? returns[Math.floor(returns.length / 2)] : null;
    out.push({
      sectorCode: code,
      sectorName: SECTOR_NAMES[code] ?? null,
      instruments: list.length,
      traded: traded.length,
      value: traded.reduce((s, e) => s + e.value, 0),
      weightedReturn: weighted,
      medianReturn: median,
      individualNetFlow: traded.reduce((s, e) => s + (e.individualNetValue ?? 0), 0),
      advancers: traded.filter((e) => (e.changePercent ?? 0) > 0).length,
      decliners: traded.filter((e) => (e.changePercent ?? 0) < 0).length,
    });
  }
  return out.sort((a, b) => b.value - a.value);
}
