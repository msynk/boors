// Market-watch page state, kept at module level so it survives navigation and the remount on language/digit change.
import { createSignal } from 'solid-js';
import type { SortSpec } from '../../grid/logic';

export type ClassGroup = 'shares' | 'funds' | 'rights' | 'all';

/** Instrument-class groups, as the contract's enum names (instrumentClass). */
export const CLASS_GROUPS: Record<ClassGroup, string[] | null> = {
  shares: ['TseStock', 'IfbStock', 'IfbBaseYellow', 'IfbBaseOrange', 'IfbBaseRed'],
  funds: ['EtfEquity', 'EtfFixedIncome', 'EtfRealEstate', 'EtfMixed', 'FundLeveraged', 'FundGoldSilver', 'FundOfFunds'],
  rights: ['Rights'],
  all: null,
};

/** The nine example filters of the Blazor page (TSETMC syntax). */
export const PRESETS = [
  { key: 'presetUp3', value: '(pc)>(py)*1.03' },
  { key: 'presetBuyQueue', value: '(pd1)==(tmax) && (qd1)>0' },
  { key: 'presetSellQueue', value: '(po1)==(tmin) && (qo1)>0' },
  { key: 'presetBuyerPower', value: '(ct).Buy_CountI>0 && (ct).Sell_CountI>0 && ((ct).Buy_I_Volume/(ct).Buy_CountI)/((ct).Sell_I_Volume/(ct).Sell_CountI)>2' },
  { key: 'presetIndividuals', value: '(ct).Buy_I_Volume>(ct).Sell_I_Volume*1.2' },
  { key: 'presetVolume', value: '(bvol)>0 && (tvol)>(bvol)*3' },
  { key: 'presetLastAbove', value: '(pl)>(pc) && (pl)>(py)' },
  { key: 'presetPe', value: '(pe)>0 && (pe)<6' },
  {
    key: 'presetHigh20',
    value: 'true==function(){if([ih].length<19)return false;for(var i=0;i<19;i++){if([ih][i].PClosing>=(pc))return false;}return true;}()',
  },
] as const;

export const [group, setGroup] = createSignal<ClassGroup>('shares');
export const [tradedOnly, setTradedOnly] = createSignal(true);
/** The filter that was applied (sent to the server). */
export const [appliedFilter, setAppliedFilter] = createSignal('');
/** The text in the box (may differ from the applied filter until Apply). */
export const [draftFilter, setDraftFilter] = createSignal('');
export const [sort, setSort] = createSignal<SortSpec | null>({ key: 'value', dir: 'desc' });
export const [quick, setQuick] = createSignal('');
