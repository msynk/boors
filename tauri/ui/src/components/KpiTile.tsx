// A headline number (stat tile): caption, value, an optional signed change, a note and a sparkline.
import { Show, type JSX } from 'solid-js';
import type { Tone } from '../format/format';
import { Sparkline } from './Sparkline';

export function KpiTile(props: {
  caption: string;
  value: JSX.Element;
  valueTone?: Tone;
  change?: JSX.Element;
  note?: JSX.Element;
  spark?: number[] | null;
  sparkTone?: Tone;
  sparkLabel?: string;
  href?: string;
  class?: string;
  /** Smaller value text for long values (ranges). */
  small?: boolean;
}) {
  return (
    <div class={`kpi ${props.class ?? ''}`}>
      <div class="kpi-caption">{props.caption}</div>
      <div class={`kpi-value ${props.small ? 'small' : ''} ${props.valueTone ? `chg ${props.valueTone}` : ''}`}>{props.value}</div>
      <Show when={props.change}>
        <div class="kpi-change">{props.change}</div>
      </Show>
      <Show when={props.note}>
        <div class="kpi-note">{props.note}</div>
      </Show>
      <Show when={props.spark && props.spark.length > 1}>
        <Sparkline values={props.spark!} tone={props.sparkTone ?? 'flat'} label={props.sparkLabel ?? props.caption} />
      </Show>
    </div>
  );
}
