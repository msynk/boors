// A tiny typed dictionary. Pure module: the shell remounts the routes when the language changes.
import { localDigits, type Lang } from '../format/format';
import { en, type MessageKey, type Messages } from './en';
import { fa } from './fa';

export type { MessageKey } from './en';

const dictionaries: Record<Lang, Messages> = { fa, en };
let current: Messages = fa;
let currentLang: Lang = 'fa';

export function setLanguage(lang: Lang): void {
  currentLang = lang;
  current = dictionaries[lang];
}

export function language(): Lang {
  return currentLang;
}

/** The string for `key`, with {0}, {1}, … replaced by `args` (numbers are formatted with the digit setting). */
export function t(key: MessageKey, ...args: (string | number)[]): string {
  const text = current[key] ?? en[key] ?? key;
  if (args.length === 0) return text;
  return text.replace(/\{(\d+)\}/g, (_, i: string) => {
    const value = args[Number(i)];
    if (value === undefined) return '';
    return typeof value === 'number' ? localDigits(String(value)) : value;
  });
}

/** TSETMC class names arrive as C# enum names (TseStock) or rules keys (tse_stock); both normalise to "tsestock". */
export function classKey(instrumentClass: string | null | undefined): string {
  return (instrumentClass ?? '').replace(/_/g, '').toLowerCase();
}

const classLabels: Record<string, MessageKey> = {
  tsestock: 'clsTseStock',
  ifbstock: 'clsIfbStock',
  ifbbaseyellow: 'clsBase',
  ifbbaseorange: 'clsBase',
  ifbbasered: 'clsBase',
  rights: 'clsRights',
  etfequity: 'clsEtfEquity',
  etffixedincome: 'clsEtfFixedIncome',
  etfrealestate: 'clsEtfRealEstate',
  etfmixed: 'clsEtfMixed',
  fundleveraged: 'clsFundLeveraged',
  fundgoldsilver: 'clsFundGoldSilver',
  fundoffunds: 'clsFundOfFunds',
  option: 'clsOption',
  debt: 'clsDebt',
};

export function className(instrumentClass: string | null | undefined): string {
  return t(classLabels[classKey(instrumentClass)] ?? 'clsOther');
}

const dayStatusLabels: Record<string, MessageKey> = {
  Traded: 'dsTraded',
  NoTrade: 'dsNoTrade',
  SymbolHalted: 'dsSymbolHalted',
  MarketClosed: 'dsMarketClosed',
  ReopenAuction: 'dsReopenAuction',
  QueueLockedUp: 'dsQueueLockedUp',
  QueueLockedDown: 'dsQueueLockedDown',
};

export function dayStatus(status: string | null | undefined): string {
  if (!status) return '–';
  const key = dayStatusLabels[status];
  return key ? t(key) : status;
}

export function messageKind(kind: string): string {
  switch (kind.toLowerCase()) {
    case 'halt':
      return t('msgKindHalt');
    case 'reopen':
      return t('msgKindReopen');
    default:
      return t('msgKindOther');
  }
}

export function haltReason(reason: string): string {
  switch (reason.toLowerCase()) {
    case 'disclosure':
      return t('reasonDisclosure');
    case 'generalmeeting':
    case 'general_meeting':
      return t('reasonGeneralMeeting');
    case 'supervisory':
      return t('reasonSupervisory');
    case 'underlying':
      return t('reasonUnderlying');
    default:
      return t('reasonUnknown');
  }
}
