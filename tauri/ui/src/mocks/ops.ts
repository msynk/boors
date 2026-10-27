// Data-health, job queue, sync and app-info fixtures. The job queue is a small state machine driven by timers.
import type { AppInfoDto, JobStatusDto, MarketDataHealthDto, SyncPullResultDto } from '../api/types';
import { ProblemError } from '../api/transport';
import { SESSION } from './calendar';

const at = (date: string, time: string) => `${date}T${time}+03:30`;

export function health(stale: boolean): MarketDataHealthDto {
  const expected = stale ? '2026-10-18' : SESSION;
  const latest = SESSION;
  return {
    generatedAt: at('2026-10-17', '18:42:10.115'),
    node: 'iran-1',
    expectedSession: expected,
    healthy: false,
    datasets: [
      { dataset: 'instrument', rows: 3112, latestDate: latest, expectedDate: expected, status: stale ? 'Stale' : 'Ok' },
      { dataset: 'daily_bar', rows: 4_812_337, latestDate: latest, expectedDate: expected, status: stale ? 'Stale' : 'Ok' },
      { dataset: 'client_type', rows: 3_904_118, latestDate: latest, expectedDate: expected, status: stale ? 'Stale' : 'Ok' },
      { dataset: 'index_daily', rows: 104_766, latestDate: latest, expectedDate: expected, status: stale ? 'Stale' : 'Ok' },
      { dataset: 'market_watch_snapshot', rows: 3023, latestDate: latest, expectedDate: expected, status: stale ? 'Stale' : 'Ok' },
      { dataset: 'best_limit', rows: 15_115, latestDate: latest, expectedDate: expected, status: stale ? 'Stale' : 'Ok' },
      { dataset: 'inst_stats', rows: 2_871, latestDate: latest, expectedDate: expected, status: 'Ok' },
      { dataset: 'macro_daily', rows: 19_420, latestDate: latest, expectedDate: expected, status: 'Ok' },
      { dataset: 'fund_daily', rows: 288_004, latestDate: '2026-10-14', expectedDate: expected, status: 'Stale' },
      { dataset: 'codal_letter', rows: 1_204_551, latestDate: latest, expectedDate: expected, status: 'Ok' },
      { dataset: 'supervisor_message', rows: 61_902, latestDate: latest, expectedDate: expected, status: 'Ok' },
      { dataset: 'shareholder', rows: 41_377, latestDate: '2026-10-15', expectedDate: null, status: 'Info' },
      { dataset: 'price_adjustment', rows: 21_640, latestDate: '2026-10-12', expectedDate: null, status: 'Info' },
      { dataset: 'instrument_state', rows: 0, latestDate: null, expectedDate: expected, status: 'Empty' },
    ],
    runs: [
      { job: 'eod', node: 'iran-1', startedAt: at('2026-10-17', '16:30:00'), finishedAt: at('2026-10-17', '16:41:18'), status: 'Succeeded', items: 3112, failures: 0, message: null },
      { job: 'messages', node: 'iran-1', startedAt: at('2026-10-17', '16:42:00'), finishedAt: at('2026-10-17', '16:42:09'), status: 'Succeeded', items: 214, failures: 0, message: null },
      { job: 'halts', node: 'iran-1', startedAt: at('2026-10-17', '16:43:00'), finishedAt: at('2026-10-17', '16:44:52'), status: 'Succeeded', items: 45, failures: 0, message: null },
      { job: 'codal', node: 'iran-1', startedAt: at('2026-10-17', '17:00:00'), finishedAt: at('2026-10-17', '17:21:40'), status: 'Partial', items: 1930, failures: 12, message: 'search.codal.ir answered 429 Too Many Requests 12 times; slowed to one request every 2 s.' },
      { job: 'funds', node: 'iran-1', startedAt: at('2026-10-17', '17:30:00'), finishedAt: at('2026-10-17', '17:30:20'), status: 'Failed', items: 0, failures: 1, message: 'fipiran.com: the operation timed out after 20 s.' },
      { job: 'macro', node: 'iran-1', startedAt: at('2026-10-17', '17:35:00'), finishedAt: at('2026-10-17', '17:35:07'), status: 'Succeeded', items: 5, failures: 0, message: null },
      { job: 'indices', node: 'iran-1', startedAt: at('2026-10-17', '16:25:00'), finishedAt: at('2026-10-17', '16:25:31'), status: 'Succeeded', items: 57, failures: 0, message: null },
      { job: 'instruments', node: 'iran-1', startedAt: at('2026-10-17', '07:30:00'), finishedAt: at('2026-10-17', '07:34:12'), status: 'Succeeded', items: 3112, failures: 0, message: null },
      { job: 'backfill', node: 'abroad-1', startedAt: at('2026-10-16', '02:00:00'), finishedAt: at('2026-10-16', '03:12:44'), status: 'Succeeded', items: 275, failures: 0, message: 'Sibling (TAL/block) listings skipped: 275.' },
    ],
    lastProbeAt: at('2026-10-17', '18:40:00'),
    sources: [
      { name: 'cdn.tsetmc.com', verdict: 'Reachable', status: 200, error: null },
      { name: 'old.tsetmc.com', verdict: 'Reachable', status: 200, error: null },
      { name: 'search.codal.ir', verdict: 'Degraded', status: 429, error: 'Too Many Requests' },
      { name: 'api.tgju.org', verdict: 'Reachable', status: 200, error: null },
      { name: 'fund.fipiran.ir', verdict: 'Unreachable', status: null, error: 'connect timeout after 20 s' },
    ],
  };
}

const jobs: JobStatusDto[] = [];
let sequence = 1041;

const nowIso = () => new Date().toISOString();

export function runJob(job: string): string {
  const known = ['instruments', 'indices', 'backfill', 'eod', 'funds', 'macro', 'codal', 'messages', 'halts', 'all'];
  if (!known.includes(job)) throw new ProblemError('Unknown job', `"${job}" is not one of ${known.join(', ')}.`, 400);
  const id = String(++sequence);
  const status: JobStatusDto = { id, job, state: 'Queued', queuedAt: nowIso(), startedAt: null, finishedAt: null, message: null };
  jobs.unshift(status);
  const durations: Record<string, number> = { macro: 2500, messages: 3500, halts: 4000, indices: 3000, instruments: 6000, eod: 9000, codal: 8000, funds: 3000, backfill: 12000, all: 15000 };
  setTimeout(() => {
    status.state = 'Running';
    status.startedAt = nowIso();
    status.message = 'Fetching…';
  }, 700);
  setTimeout(() => {
    status.finishedAt = nowIso();
    if (job === 'funds') {
      status.state = 'Failed';
      status.message = 'fipiran.com: the operation timed out after 20 s.';
    } else {
      status.state = 'Succeeded';
      status.message = job === 'codal' ? '1,930 letters, 12 retried after 429.' : 'Done.';
    }
  }, 700 + (durations[job] ?? 4000));
  while (jobs.length > 60) jobs.pop();
  return id;
}

export function getJobs(): JobStatusDto[] {
  return jobs.map((j) => ({ ...j }));
}

export async function syncPull(body: unknown): Promise<SyncPullResultDto> {
  const b = (body ?? {}) as { nodeUrl?: unknown; since?: unknown };
  if (typeof b.nodeUrl !== 'string' || !/^https?:\/\//.test(b.nodeUrl)) throw new ProblemError('Invalid node URL', 'nodeUrl must be an http(s) URL.', 400);
  if (b.since !== null && b.since !== undefined && (typeof b.since !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(b.since)))
    throw new ProblemError('Invalid date', 'since must be yyyy-MM-dd or null.', 400);
  await new Promise((r) => setTimeout(r, 1200));
  if (b.nodeUrl.includes('fail')) throw new ProblemError('Bad Gateway', `GET ${b.nodeUrl.replace(/\/$/, '')}/api/v1/Sync/ExportBundle failed: connection refused`, 502);
  return { imported: 18432, skipped: 211 };
}

export function appInfo(): AppInfoDto {
  return { version: '0.1.0-mock', node: 'mock', dataRoot: 'C:\\Users\\you\\AppData\\Roaming\\boors\\.boors-data', transport: 'http', collect: false };
}
