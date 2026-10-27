// /market/health — dataset freshness, last collection runs, source reachability, job buttons with live progress
// (polling MarketData/GetJobs), "Sync from node", the operator token (HTTP only) and App/GetInfo.
import { For, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { JOB_NAMES, app, client, marketData, sync, type JobName, type JobStatusDto } from '../api';
import { getOperatorToken, setOperatorToken } from '../api/client';
import { Icon } from '../components/Icon';
import { Table } from '../components/Table';
import { Card, ErrorState, Field, Num, Sep, Skeleton, StatusBadge, problemText, type StatusLevel } from '../components/ui';
import * as f from '../format/format';
import { t, type MessageKey } from '../i18n';
import { refreshHealth } from '../state/health';
import { createQuery } from '../state/query';
import { usePageTitle } from '../state/title';
import { ProblemError } from '../api/transport';

export function statusLevel(status: string | null | undefined): StatusLevel {
  switch ((status ?? '').toLowerCase()) {
    case 'ok':
    case 'succeeded':
    case 'success':
    case 'reachable':
      return 'good';
    case 'stale':
    case 'partial':
    case 'degraded':
    case 'ratelimited':
      return 'warning';
    case 'running':
      return 'info';
    case 'empty':
    case 'failed':
    case 'blocked':
    case 'unreachable':
      return 'critical';
    case 'queued':
      return 'neutral';
    default:
      return 'info';
  }
}

const knownLabels: Record<string, MessageKey> = {
  ok: 'statusOk',
  stale: 'statusStale',
  empty: 'statusEmpty',
  info: 'statusInfo',
  queued: 'stateQueued',
  running: 'stateRunning',
  succeeded: 'stateSucceeded',
  failed: 'stateFailed',
  partial: 'statusPartial',
  reachable: 'verdictReachable',
  degraded: 'verdictDegraded',
  unreachable: 'verdictUnreachable',
  blocked: 'verdictBlocked',
};

function statusLabel(status: string): string {
  const key = knownLabels[status.toLowerCase()];
  return key ? t(key) : status;
}

function Badge(props: { status: string }) {
  return <StatusBadge level={statusLevel(props.status)} label={statusLabel(props.status)} />;
}

export default function Health() {
  usePageTitle(t('healthTitle'));
  const health = createQuery(() => true, (_, signal, force) => marketData.getHealth({ signal, force }));
  const info = createQuery(() => true, (_, signal, force) => app.getInfo({ signal, force }));
  const [jobs, setJobs] = createSignal<JobStatusDto[]>([]);
  const [jobsError, setJobsError] = createSignal<ProblemError | null>(null);
  const [runError, setRunError] = createSignal<ProblemError | null>(null);
  const [queuedMessage, setQueuedMessage] = createSignal<string | null>(null);
  const [starting, setStarting] = createSignal<JobName | null>(null);

  // Job polling: fast while something is queued or running, slow otherwise.
  let timer: ReturnType<typeof setTimeout> | undefined;
  let alive = true;
  let hadActive = false;
  const poll = async () => {
    clearTimeout(timer);
    try {
      const list = await marketData.getJobs();
      if (!alive) return;
      setJobs(list);
      setJobsError(null);
      const active = list.some((j) => j.state === 'Queued' || j.state === 'Running');
      if (hadActive && !active) {
        // A job just finished: datasets and runs changed.
        health.refetch(true);
        void refreshHealth(true);
      }
      hadActive = active;
      if (alive) timer = setTimeout(poll, active ? 1500 : 15000);
    } catch (error) {
      if (!alive) return;
      setJobsError(error instanceof ProblemError ? error : new ProblemError('Request failed', String(error), 0));
      timer = setTimeout(poll, 15000);
    }
  };
  void poll();
  onCleanup(() => {
    alive = false;
    clearTimeout(timer);
  });

  const run = async (job: JobName) => {
    setStarting(job);
    setRunError(null);
    try {
      const id = await marketData.runJob(job);
      setQueuedMessage(t('jobQueued', job, String(id)));
      hadActive = true;
      void poll();
    } catch (error) {
      setRunError(error instanceof ProblemError ? error : new ProblemError('Request failed', String(error), 0));
    } finally {
      setStarting(null);
    }
  };

  // Sync from node.
  const [nodeUrl, setNodeUrl] = createSignal('');
  const [since, setSince] = createSignal('');
  const [syncing, setSyncing] = createSignal(false);
  const [syncResult, setSyncResult] = createSignal<string | null>(null);
  const [syncError, setSyncError] = createSignal<string | null>(null);
  const pull = async (event: Event) => {
    event.preventDefault();
    setSyncResult(null);
    setSyncError(null);
    const url = nodeUrl().trim();
    const from = f.asciiDigits(since().trim());
    if (!/^https?:\/\/\S+$/i.test(url)) return setSyncError(t('syncInvalidUrl'));
    if (from && !/^\d{4}-\d{2}-\d{2}$/.test(from)) return setSyncError(t('syncInvalidDate'));
    setSyncing(true);
    try {
      const result = await sync.pull({ nodeUrl: url, since: from || null });
      setSyncResult(t('syncDone', f.integer(result.imported), f.integer(result.skipped)));
      client.clear();
      health.refetch(true);
    } catch (error) {
      const p = problemText(error instanceof ProblemError ? error : new ProblemError('Request failed', String(error), 0));
      setSyncError(p.detail ? `${p.title} — ${p.detail}` : p.title);
    } finally {
      setSyncing(false);
    }
  };

  // Operator token (HTTP only; IPC needs none).
  const [token, setToken] = createSignal(getOperatorToken() ?? '');
  const [remember, setRemember] = createSignal(false);
  const [tokenSaved, setTokenSaved] = createSignal(false);
  createEffect(() => {
    if (tokenSaved()) {
      const id = setTimeout(() => setTokenSaved(false), 2000);
      onCleanup(() => clearTimeout(id));
    }
  });

  return (
    <div class="page health-page">
      <header class="page-head">
        <h1 class="page-title">{t('healthTitle')}</h1>
        <Show when={health.data()}>
          {(h) => (
            <>
              <StatusBadge level={h().healthy ? 'good' : 'warning'} label={h().healthy ? t('healthy') : t('needsAttention')} />
              <span class="page-meta">
                {t('expectedSession')} <Num>{f.date(h().expectedSession)}</Num> <Sep /> {t('node')} <bdi>{h().node}</bdi> <Sep /> {t('generatedAt')} <Num>{f.dateTime(h().generatedAt)}</Num>
              </span>
            </>
          )}
        </Show>
        <div class="grow" />
        <button type="button" class="btn" onClick={() => health.refetch(true)} disabled={health.loading()}>
          <Icon name="refresh" size={15} />
          {t('reload')}
        </button>
      </header>

      <Card title={t('runJobs')} subtitle={t('runJobsNote')}>
        <div class="job-buttons">
          <For each={JOB_NAMES}>
            {(job) => (
              <button type="button" class="btn sm" onClick={() => void run(job)} disabled={starting() !== null}>
                <Show when={starting() === job} fallback={<Icon name="play" size={13} />}>
                  <span class="spinner sm" aria-hidden="true" />
                </Show>
                <span dir="ltr">{job}</span>
              </button>
            )}
          </For>
        </div>
        <Show when={queuedMessage()}>
          <div class="banner success" role="status">
            <Icon name="okCircle" size={16} />
            <span dir="auto">{queuedMessage()}</span>
            <button type="button" class="icon-btn sm" aria-label={t('close_')} onClick={() => setQueuedMessage(null)}>
              <Icon name="close" size={14} />
            </button>
          </div>
        </Show>
        <Show when={runError()}>{(e) => <ErrorState error={e()} compact />}</Show>
        <h3 class="sub-title">{t('jobs')}</h3>
        <Show when={!jobsError()} fallback={<ErrorState error={jobsError()!} compact onRetry={() => void poll()} />}>
          <Table
            rows={jobs()}
            label={t('jobs')}
            empty={t('noJobs')}
            maxHeight="320px"
            columns={[
              { key: 'job', header: t('job'), cell: (j) => <span dir="ltr">{j.job}</span> },
              { key: 'id', header: '#', cell: (j) => <bdi dir="ltr" class="num muted">{j.id}</bdi> },
              {
                key: 'state',
                header: t('status'),
                cell: (j) => (
                  <span class="job-state">
                    <Badge status={j.state} />
                    <Show when={j.state === 'Running'}>
                      <span class="progress-indeterminate" aria-hidden="true" />
                    </Show>
                  </span>
                ),
              },
              { key: 'queued', header: t('queuedAt'), cell: (j) => <Num>{f.dateTime(j.queuedAt)}</Num> },
              { key: 'duration', header: t('duration'), cell: (j) => (j.startedAt ? f.duration(j.startedAt, j.finishedAt) ?? t('running') : f.DASH) },
              { key: 'message', header: t('message'), class: 'wrap', cell: (j) => <span dir="auto">{j.message ?? ''}</span> },
            ]}
          />
        </Show>
      </Card>

      <Show when={!health.error()} fallback={<ErrorState error={health.error()!} onRetry={() => health.refetch(true)} />}>
        <Show when={health.data()} fallback={<Skeleton lines={10} />}>
          {(h) => (
            <div class="health-grid">
              <Card title={t('datasets')} class="span-7">
                <Table
                  rows={h().datasets}
                  label={t('datasets')}
                  columns={[
                    { key: 'dataset', header: t('dataset'), cell: (d) => <span dir="ltr">{d.dataset}</span>, sort: { value: (d) => d.dataset, kind: 'text' } },
                    { key: 'rows', header: t('rowsCol'), numeric: true, cell: (d) => f.integer(d.rows), sort: { value: (d) => d.rows, kind: 'number' } },
                    { key: 'latest', header: t('latest'), cell: (d) => <Num>{f.date(d.latestDate)}</Num>, sort: { value: (d) => d.latestDate, kind: 'text' } },
                    { key: 'expected', header: t('expected'), cell: (d) => <Num>{f.date(d.expectedDate)}</Num> },
                    { key: 'status', header: t('status'), cell: (d) => <Badge status={d.status} />, sort: { value: (d) => d.status, kind: 'text' } },
                  ]}
                />
              </Card>
              <Card title={t('sources')} subtitle={h().lastProbeAt ? t('lastProbe', f.dateTime(h().lastProbeAt)) : t('notProbed')} class="span-5">
                <Table
                  rows={h().sources}
                  label={t('sources')}
                  columns={[
                    { key: 'name', header: t('source'), cell: (s) => <span dir="ltr">{s.name}</span> },
                    { key: 'verdict', header: t('verdict'), cell: (s) => <Badge status={s.verdict} /> },
                    { key: 'status', header: t('httpStatus'), numeric: true, cell: (s) => (s.status === null ? f.DASH : <bdi dir="ltr">{f.localDigits(String(s.status))}</bdi>) },
                    { key: 'error', header: t('error'), class: 'wrap', cell: (s) => <span dir="auto">{s.error ?? ''}</span> },
                  ]}
                />
              </Card>
              <Card title={t('lastRuns')} class="span-12">
                <Table
                  rows={h().runs}
                  label={t('lastRuns')}
                  maxHeight="420px"
                  columns={[
                    { key: 'job', header: t('job'), cell: (r) => <span dir="ltr">{r.job}</span> },
                    { key: 'node', header: t('node'), class: 'pro-only', cell: (r) => <span dir="ltr">{r.node}</span> },
                    { key: 'started', header: t('started'), cell: (r) => <Num>{f.dateTime(r.startedAt)}</Num>, sort: { value: (r) => r.startedAt, kind: 'text' } },
                    { key: 'duration', header: t('duration'), cell: (r) => f.duration(r.startedAt, r.finishedAt) ?? t('running') },
                    { key: 'status', header: t('status'), cell: (r) => <Badge status={r.status} /> },
                    { key: 'items', header: t('items'), numeric: true, cell: (r) => f.integer(r.items) },
                    { key: 'failures', header: t('failures'), numeric: true, cell: (r) => f.integer(r.failures) },
                    { key: 'message', header: t('message'), class: 'wrap', cell: (r) => <span dir="auto">{r.message ?? ''}</span> },
                  ]}
                />
              </Card>
            </div>
          )}
        </Show>
      </Show>

      <div class="health-grid">
        <Card
          title={t('syncTitle')}
          subtitle={
            <>
              {t('syncNote')} <code dir="ltr">{'{node}/api/v1/Sync/ExportBundle'}</code>
            </>
          }
          class="span-7"
        >
          <form class="form" onSubmit={pull}>
            <label class="form-field">
              <span class="field-label">{t('syncNodeUrl')}</span>
              <input class="input" type="url" dir="ltr" inputMode="url" placeholder="https://node.example/" value={nodeUrl()} onInput={(e) => setNodeUrl(e.currentTarget.value)} required />
            </label>
            <label class="form-field narrow">
              <span class="field-label">{t('syncSince')}</span>
              <input class="input" type="text" dir="ltr" inputMode="numeric" placeholder="2026-01-01" value={since()} onInput={(e) => setSince(e.currentTarget.value)} />
            </label>
            <button type="submit" class="btn primary" disabled={syncing()}>
              <Show when={syncing()} fallback={<Icon name="sync" size={15} />}>
                <span class="spinner sm" aria-hidden="true" />
              </Show>
              {t('syncPull')}
            </button>
          </form>
          <Show when={syncResult()}>
            <div class="banner success" role="status">
              <Icon name="okCircle" size={16} />
              <span>{syncResult()}</span>
            </div>
          </Show>
          <Show when={syncError()}>
            <div class="banner error" role="alert">
              <Icon name="error" size={16} />
              <span dir="auto">{syncError()}</span>
            </div>
          </Show>
        </Card>

        <Card title={t('appInfo')} class="span-5">
          <Show when={!info.error()} fallback={<ErrorState error={info.error()!} compact onRetry={() => info.refetch(true)} />}>
            <Show when={info.data()} fallback={<Skeleton lines={4} />}>
              {(i) => (
                <dl class="fields">
                  <Field label={t('version')}>
                    <bdi dir="ltr">{i().version}</bdi>
                  </Field>
                  <Field label={t('node')}>
                    <bdi dir="ltr">{i().node}</bdi>
                  </Field>
                  <Field label={t('dataRoot')}>
                    <bdi dir="ltr" class="mono">{i().dataRoot}</bdi>
                  </Field>
                  <Field label={t('transport')}>
                    {client.transportKind === 'mock' ? t('transportMock') : i().transport === 'ipc' ? t('transportIpc') : t('transportHttp')}
                  </Field>
                  <Field label={t('collect')}>
                    <StatusBadge level={i().collect ? 'good' : 'neutral'} label={i().collect ? t('collectOn') : t('collectOff')} />
                  </Field>
                </dl>
              )}
            </Show>
          </Show>
          <Show when={client.transportKind !== 'ipc'}>
            <form
              class="form token-form"
              onSubmit={(e) => {
                e.preventDefault();
                setOperatorToken(token().trim() || null, remember());
                setTokenSaved(true);
              }}
            >
              <label class="form-field">
                <span class="field-label">{t('operatorToken')}</span>
                <input class="input" type="password" dir="ltr" autocomplete="off" value={token()} onInput={(e) => setToken(e.currentTarget.value)} />
              </label>
              <label class="check">
                <input type="checkbox" checked={remember()} onChange={(e) => setRemember(e.currentTarget.checked)} />
                {t('remember')}
              </label>
              <button type="submit" class="btn sm">
                <Show when={tokenSaved()} fallback={t('save')}>
                  <Icon name="check" size={14} />
                  {t('saved')}
                </Show>
              </button>
              <p class="field-help">{t('operatorTokenHint')}</p>
            </form>
          </Show>
        </Card>
      </div>
    </div>
  );
}
