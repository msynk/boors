// Request layer over a Transport: de-duplicates identical in-flight requests, keeps a small in-memory cache with a
// per-call TTL, and lets each caller abort its own wait (the shared request is aborted when its last waiter leaves).

import { abortError, cleanQuery, createHttpTransport, createTauriTransport, isTauri, ProblemError, toProblem, type ApiRequest, type Transport } from './transport';

export interface CallOptions {
  signal?: AbortSignal;
  /** Cache lifetime in ms. Defaults: 30 s for GET, 0 (no cache) for POST. */
  ttl?: number;
  /** Skip the cache (the fresh answer still refreshes it). */
  force?: boolean;
  /** Share an identical in-flight request (default true; mutations pass false). */
  dedupe?: boolean;
}

export type TransportKind = 'ipc' | 'http' | 'mock';

interface Shared {
  promise: Promise<unknown>;
  controller: AbortController;
  waiters: number;
}

interface CacheEntry {
  at: number;
  value: unknown;
}

const MAX_CACHE_ENTRIES = 200;

export class ApiClient {
  private transport: Transport;
  private kind: TransportKind;
  private readonly cache = new Map<string, CacheEntry>();
  private readonly inflight = new Map<string, Shared>();
  private readonly now: () => number;

  constructor(transport: Transport, kind: TransportKind, now: () => number = () => Date.now()) {
    this.transport = transport;
    this.kind = kind;
    this.now = now;
  }

  get transportKind(): TransportKind {
    return this.kind;
  }

  setTransport(transport: Transport, kind: TransportKind): void {
    this.transport = transport;
    this.kind = kind;
    this.clear();
  }

  clear(): void {
    this.cache.clear();
  }

  /** Drops cached answers whose path starts with `prefix` (e.g. after a sync import). */
  invalidate(prefix = ''): void {
    for (const key of [...this.cache.keys()]) if (key.includes(' ' + prefix)) this.cache.delete(key);
  }

  static keyOf(request: ApiRequest): string {
    const query = cleanQuery(request.query);
    const q = query ? Object.keys(query).sort().map((k) => `${k}=${String(query[k])}`).join('&') : '';
    const body = request.body === undefined ? '' : JSON.stringify(request.body);
    return `${request.method} ${request.path}?${q}#${body}`;
  }

  call<T>(request: ApiRequest, options: CallOptions = {}): Promise<T> {
    const key = ApiClient.keyOf(request);
    const ttl = options.ttl ?? (request.method === 'GET' ? 30_000 : 0);
    const signal = options.signal;
    if (signal?.aborted) return Promise.reject(abortError());

    if (ttl > 0 && !options.force) {
      const hit = this.cache.get(key);
      if (hit && this.now() - hit.at < ttl) return Promise.resolve(hit.value as T);
    }

    const dedupe = options.dedupe ?? true;
    let shared = dedupe ? this.inflight.get(key) : undefined;
    if (!shared) {
      const controller = new AbortController();
      const created: Shared = {
        controller,
        waiters: 0,
        promise: this.transport({ ...request, signal: controller.signal }).then(
          (value) => {
            if (ttl > 0) this.store(key, value);
            return value;
          },
          (error: unknown) => {
            throw toProblem(error);
          },
        ),
      };
      created.promise.then(
        () => this.release(key, created),
        () => this.release(key, created),
      );
      if (dedupe) this.inflight.set(key, created);
      shared = created;
    }
    return this.wait<T>(shared, signal);
  }

  private release(key: string, shared: Shared): void {
    if (this.inflight.get(key) === shared) this.inflight.delete(key);
  }

  private store(key: string, value: unknown): void {
    this.cache.delete(key);
    this.cache.set(key, { at: this.now(), value });
    while (this.cache.size > MAX_CACHE_ENTRIES) {
      const oldest = this.cache.keys().next().value;
      if (oldest === undefined) break;
      this.cache.delete(oldest);
    }
  }

  private wait<T>(shared: Shared, signal: AbortSignal | undefined): Promise<T> {
    shared.waiters++;
    return new Promise<T>((resolve, reject) => {
      let settled = false;
      const leave = () => {
        signal?.removeEventListener('abort', onAbort);
      };
      const onAbort = () => {
        if (settled) return;
        settled = true;
        leave();
        shared.waiters--;
        if (shared.waiters <= 0) shared.controller.abort();
        reject(abortError());
      };
      signal?.addEventListener('abort', onAbort, { once: true });
      shared.promise.then(
        (value) => {
          if (settled) return;
          settled = true;
          leave();
          shared.waiters--;
          resolve(value as T);
        },
        (error: unknown) => {
          if (settled) return;
          settled = true;
          leave();
          shared.waiters--;
          reject(error instanceof ProblemError ? error : toProblem(error));
        },
      );
    });
  }
}

const OPERATOR_TOKEN_KEY = 'boors.operatorToken';

export function getOperatorToken(): string | null {
  try {
    return sessionStorage.getItem(OPERATOR_TOKEN_KEY) ?? localStorage.getItem(OPERATOR_TOKEN_KEY);
  } catch {
    return null;
  }
}

export function setOperatorToken(token: string | null, remember: boolean): void {
  try {
    sessionStorage.removeItem(OPERATOR_TOKEN_KEY);
    localStorage.removeItem(OPERATOR_TOKEN_KEY);
    if (token) (remember ? localStorage : sessionStorage).setItem(OPERATOR_TOKEN_KEY, token);
  } catch {
    // Storage blocked: the token simply is not kept.
  }
}

function defaultTransport(): [Transport, TransportKind] {
  if (isTauri()) return [createTauriTransport(), 'ipc'];
  return [createHttpTransport({ operatorToken: getOperatorToken }), 'http'];
}

const [initialTransport, initialKind] = defaultTransport();

/** The app-wide client. main.tsx swaps in the mock transport when VITE_MOCK is set. */
export const client = new ApiClient(initialTransport, initialKind);
