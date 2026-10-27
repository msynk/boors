// Small UI primitives: numbers and changes (bidi-isolated, tabular), cards, toggles, tabs, badges and page states.
import { A } from '@solidjs/router';
import { For, Show, createUniqueId, type JSX, type ParentProps } from 'solid-js';
import type { ProblemError } from '../api';
import * as f from '../format/format';
import { t } from '../i18n';
import { isStale, expectedSession } from '../state/health';
import { Icon, type IconName } from './Icon';

/** A number run: LTR-isolated with tabular figures, so RTL text never reorders it. */
export function Num(props: ParentProps<{ class?: string; title?: string }>) {
  return (
    <bdi dir="ltr" class={`num ${props.class ?? ''}`} title={props.title}>
      {props.children}
    </bdi>
  );
}

/** A percentage change: "▲ +2.50%" in the up colour, "▼ −1.91%" in the down colour. */
export function Chg(props: { value: number | null | undefined; fraction?: boolean; decimals?: number; class?: string }) {
  const percent = () => (props.value === null || props.value === undefined ? null : props.fraction ? props.value * 100 : props.value);
  return (
    <bdi dir="ltr" class={`num chg ${f.tone(percent())} ${props.class ?? ''}`}>
      {f.changeText(percent(), props.decimals ?? 2)}
    </bdi>
  );
}

/** A signed amount (money flow), coloured by sign. */
export function Flow(props: { value: number | null | undefined; text: string; class?: string }) {
  return (
    <bdi dir="ltr" class={`num chg ${f.flowTone(props.value)} ${props.class ?? ''}`}>
      {props.text}
    </bdi>
  );
}

export function Card(
  props: ParentProps<{ title?: JSX.Element; subtitle?: JSX.Element; actions?: JSX.Element; class?: string; id?: string; bodyClass?: string }>,
) {
  const headingId = createUniqueId();
  return (
    <section class={`card ${props.class ?? ''}`} id={props.id} aria-labelledby={props.title ? headingId : undefined}>
      <Show when={props.title || props.actions}>
        <header class="card-head">
          <div class="card-titles">
            <Show when={props.title}>
              <h2 class="card-title" id={headingId}>
                {props.title}
              </h2>
            </Show>
            <Show when={props.subtitle}>
              <p class="card-subtitle">{props.subtitle}</p>
            </Show>
          </div>
          <Show when={props.actions}>
            <div class="card-actions">{props.actions}</div>
          </Show>
        </header>
      </Show>
      <div class={`card-body ${props.bodyClass ?? ''}`}>{props.children}</div>
    </section>
  );
}

export interface Option<T extends string> {
  value: T;
  label: JSX.Element;
  title?: string;
}

/** A toggle group (exactly one pressed). */
export function Segmented<T extends string>(props: {
  options: Option<T>[];
  value: T;
  onChange: (value: T) => void;
  label: string;
  size?: 'sm' | 'md';
  class?: string;
}) {
  return (
    <div class={`segmented ${props.size === 'sm' ? 'sm' : ''} ${props.class ?? ''}`} role="group" aria-label={props.label}>
      <For each={props.options}>
        {(option) => (
          <button
            type="button"
            class="seg"
            aria-pressed={props.value === option.value}
            title={option.title}
            onClick={() => props.onChange(option.value)}
          >
            {option.label}
          </button>
        )}
      </For>
    </div>
  );
}

/** Tabs (ARIA tablist with roving arrow keys). The panel is rendered by the caller. */
export function Tabs<T extends string>(props: {
  tabs: { value: T; label: JSX.Element; count?: number }[];
  value: T;
  onChange: (value: T) => void;
  label: string;
  idPrefix?: string;
}) {
  const onKey = (event: KeyboardEvent) => {
    const keys = ['ArrowLeft', 'ArrowRight', 'Home', 'End'];
    if (!keys.includes(event.key)) return;
    event.preventDefault();
    const list = props.tabs.map((tab) => tab.value);
    const index = list.indexOf(props.value);
    const rtl = document.documentElement.dir === 'rtl';
    const forward = (event.key === 'ArrowRight') !== rtl;
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? list.length - 1 : (index + (forward ? 1 : -1) + list.length) % list.length;
    props.onChange(list[next]);
    const target = (event.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role=tab]')[next];
    target?.focus();
  };
  return (
    <div class="tabs" role="tablist" aria-label={props.label} onKeyDown={onKey}>
      <For each={props.tabs}>
        {(tab) => (
          <button
            type="button"
            role="tab"
            class="tab"
            id={props.idPrefix ? `${props.idPrefix}-${tab.value}` : undefined}
            aria-selected={props.value === tab.value}
            tabIndex={props.value === tab.value ? 0 : -1}
            onClick={() => props.onChange(tab.value)}
          >
            {tab.label}
            <Show when={tab.count !== undefined}>
              <span class="tab-count">{f.integer(tab.count)}</span>
            </Show>
          </button>
        )}
      </For>
    </div>
  );
}

export function Switch(props: { checked: boolean; onChange: (checked: boolean) => void; label: string; hint?: string }) {
  return (
    <label class="switch" title={props.hint}>
      <input type="checkbox" role="switch" checked={props.checked} onChange={(e) => props.onChange(e.currentTarget.checked)} />
      <span class="switch-track" aria-hidden="true">
        <span class="switch-thumb" />
      </span>
      <span>{props.label}</span>
    </label>
  );
}

export type StatusLevel = 'good' | 'warning' | 'serious' | 'critical' | 'info' | 'neutral';

const statusIcon: Record<StatusLevel, IconName> = {
  good: 'okCircle',
  warning: 'warning',
  serious: 'warning',
  critical: 'error',
  info: 'info',
  neutral: 'clock',
};

/** A status badge: icon + label, never colour alone. */
export function StatusBadge(props: { level: StatusLevel; label: string; class?: string }) {
  return (
    <span class={`badge status-${props.level} ${props.class ?? ''}`}>
      <Icon name={statusIcon[props.level]} size={14} />
      <span>{props.label}</span>
    </span>
  );
}

export function Tag(props: ParentProps<{ tone?: 'up' | 'down' | 'neutral'; class?: string; title?: string }>) {
  return (
    <span class={`tag ${props.tone ?? 'neutral'} ${props.class ?? ''}`} title={props.title}>
      {props.children}
    </span>
  );
}

export function Spinner(props: { label?: string }) {
  return <span class="spinner" role="status" aria-label={props.label ?? t('loading')} />;
}

/** Placeholder blocks while loading. */
export function Skeleton(props: { height?: string; lines?: number; class?: string }) {
  return (
    <div class={`skeleton-wrap ${props.class ?? ''}`} aria-busy="true" aria-label={t('loading')}>
      <Show when={props.lines} fallback={<div class="skeleton" style={{ height: props.height ?? '8rem' }} />}>
        <For each={Array.from({ length: props.lines ?? 0 })}>{(_, i) => <div class="skeleton line" style={{ width: `${92 - ((i() * 17) % 40)}%` }} />}</For>
      </Show>
    </div>
  );
}

export function EmptyState(props: ParentProps<{ icon?: IconName; title?: string }>) {
  return (
    <div class="state empty" role="status">
      <Icon name={props.icon ?? 'info'} size={22} />
      <div>
        <Show when={props.title}>
          <strong>{props.title}</strong>
        </Show>
        <div>{props.children}</div>
      </div>
    </div>
  );
}

export function problemText(error: ProblemError): { title: string; detail: string | null } {
  if (error.kind === 'network') return { title: t('errorNetwork'), detail: error.detail };
  if (error.status === 401) return { title: t('errorUnauthorized'), detail: error.detail };
  if (error.status === 404) return { title: t('errorNotFound'), detail: error.detail ?? error.title };
  return { title: error.title || t('errorTitle'), detail: error.detail };
}

/** A failed request: the problem's title and detail, and a retry button. */
export function ErrorState(props: { error: ProblemError; onRetry?: () => void; compact?: boolean }) {
  const text = () => problemText(props.error);
  return (
    <div class={`state error ${props.compact ? 'compact' : ''}`} role="alert">
      <Icon name="error" size={22} />
      <div class="state-text">
        <strong dir="auto">{text().title}</strong>
        <Show when={text().detail}>
          <div class="state-detail" dir="auto">
            {text().detail}
          </div>
        </Show>
      </div>
      <Show when={props.onRetry}>
        <button type="button" class="btn sm" onClick={() => props.onRetry?.()}>
          <Icon name="refresh" size={14} />
          {t('retry')}
        </button>
      </Show>
    </div>
  );
}

/** Shown when a page's data session is older than the session the node expected. */
export function StaleBanner(props: { session: string | null | undefined }) {
  return (
    <Show when={isStale(props.session)}>
      <div class="banner warning" role="status">
        <Icon name="warning" size={18} />
        <span>{t('staleBanner', f.date(props.session), f.date(expectedSession()))}</span>
        <A href="/market/health" class="banner-link">
          {t('staleSeeHealth')}
        </A>
      </div>
    </Show>
  );
}

/** A visual separator between inline facts (a rule, not a dot: '·' reads as the Persian zero '۰'). */
export function Sep() {
  return <span class="sep" aria-hidden="true" />;
}

/** A labelled value row (definition list item). */
export function Field(props: ParentProps<{ label: string }>) {
  return (
    <div class="field">
      <dt>{props.label}</dt>
      <dd>{props.children}</dd>
    </div>
  );
}
