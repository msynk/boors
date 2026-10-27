// The two transports of the contract (tauri/docs/api-contract.md): Tauri IPC `invoke('api', …)` inside the desktop
// shell, HTTP `/api/v1/{path}` everywhere else. Both resolve with the JSON value and reject with a ProblemError.

export type QueryValue = string | number | boolean | null | undefined;

export interface ApiRequest {
  method: 'GET' | 'POST';
  /** Everything after /api/v1/, path parameters included (already URI-encoded). */
  path: string;
  query?: Record<string, QueryValue>;
  body?: unknown;
  signal?: AbortSignal;
  /** Send the operator token (RunJob, Sync/*). Ignored over IPC. */
  operator?: boolean;
}

export type Transport = (request: ApiRequest) => Promise<unknown>;

export type ProblemKind = 'problem' | 'network' | 'aborted';

/** An RFC 7807 problem (`{ title, detail, status }`), a network failure, or an aborted request. */
export class ProblemError extends Error {
  readonly title: string;
  readonly detail: string | null;
  readonly status: number;
  readonly kind: ProblemKind;

  constructor(title: string, detail: string | null, status: number, kind: ProblemKind = 'problem') {
    super(detail ? `${title}: ${detail}` : title);
    this.name = 'ProblemError';
    this.title = title;
    this.detail = detail;
    this.status = status;
    this.kind = kind;
  }

  get isAbort(): boolean {
    return this.kind === 'aborted';
  }
}

export function abortError(): ProblemError {
  return new ProblemError('Aborted', null, 0, 'aborted');
}

export function isAbort(error: unknown): boolean {
  return (error instanceof ProblemError && error.kind === 'aborted') || (error instanceof DOMException && error.name === 'AbortError');
}

/** Turns whatever a transport rejected with into a ProblemError. */
export function toProblem(error: unknown, fallbackStatus = 500): ProblemError {
  if (error instanceof ProblemError) return error;
  if (error instanceof DOMException && error.name === 'AbortError') return abortError();
  if (error && typeof error === 'object') {
    const e = error as Record<string, unknown>;
    if (typeof e.title === 'string') {
      return new ProblemError(e.title, typeof e.detail === 'string' ? e.detail : null, typeof e.status === 'number' ? e.status : fallbackStatus);
    }
    if (typeof e.message === 'string') return new ProblemError('Request failed', e.message, fallbackStatus);
  }
  if (typeof error === 'string') {
    // Tauri rejects with the serialised value; a string may itself be a JSON problem.
    try {
      return toProblem(JSON.parse(error), fallbackStatus);
    } catch {
      return new ProblemError('Request failed', error, fallbackStatus);
    }
  }
  return new ProblemError('Request failed', null, fallbackStatus);
}

/** Drops null/undefined so an omitted argument means "the default" on the server. */
export function cleanQuery(query: Record<string, QueryValue> | undefined): Record<string, string | number | boolean> | undefined {
  if (!query) return undefined;
  const out: Record<string, string | number | boolean> = {};
  let any = false;
  for (const [key, value] of Object.entries(query)) {
    if (value === null || value === undefined || value === '') continue;
    out[key] = value;
    any = true;
  }
  return any ? out : undefined;
}

export function buildUrl(base: string, path: string, query?: Record<string, QueryValue>): string {
  const clean = cleanQuery(query);
  let url = base + path;
  if (clean) {
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(clean)) params.set(key, String(value));
    url += '?' + params.toString();
  }
  return url;
}

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export interface HttpTransportOptions {
  base?: string;
  fetch?: typeof fetch;
  /** The operator token for RunJob and Sync/* (sent as a Bearer token when set). */
  operatorToken?: () => string | null;
}

export function createHttpTransport(options: HttpTransportOptions = {}): Transport {
  const base = options.base ?? '/api/v1/';
  return async (request) => {
    const doFetch = options.fetch ?? globalThis.fetch.bind(globalThis);
    const headers: Record<string, string> = { Accept: 'application/json' };
    if (request.body !== undefined) headers['Content-Type'] = 'application/json';
    const token = request.operator ? options.operatorToken?.() : null;
    if (token) headers.Authorization = `Bearer ${token}`;

    let response: Response;
    try {
      response = await doFetch(buildUrl(base, request.path, request.query), {
        method: request.method,
        headers,
        body: request.body === undefined ? undefined : JSON.stringify(request.body),
        signal: request.signal,
      });
    } catch (error) {
      if (isAbort(error) || request.signal?.aborted) throw abortError();
      throw new ProblemError('Network error', error instanceof Error ? error.message : String(error), 0, 'network');
    }

    const text = await response.text();
    if (!response.ok) {
      let problem: ProblemError | null;
      try {
        problem = toProblem(JSON.parse(text), response.status);
      } catch {
        problem = null;
      }
      throw problem ?? new ProblemError(response.statusText || `HTTP ${response.status}`, text.slice(0, 300) || null, response.status);
    }
    if (text.length === 0) return null;
    try {
      return JSON.parse(text);
    } catch {
      throw new ProblemError('Invalid response', `Expected JSON from ${request.path}`, response.status);
    }
  };
}

/** Rejects as soon as `signal` aborts (IPC calls cannot be cancelled, so their late result is ignored). */
export function raceAbort<T>(promise: Promise<T>, signal: AbortSignal | undefined): Promise<T> {
  if (!signal) return promise;
  if (signal.aborted) return Promise.reject(abortError());
  return new Promise<T>((resolve, reject) => {
    const onAbort = () => reject(abortError());
    signal.addEventListener('abort', onAbort, { once: true });
    promise.then(
      (value) => {
        signal.removeEventListener('abort', onAbort);
        resolve(value);
      },
      (error) => {
        signal.removeEventListener('abort', onAbort);
        reject(error);
      },
    );
  });
}

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

export function createTauriTransport(loadInvoke?: () => Promise<Invoke>): Transport {
  let invokePromise: Promise<Invoke> | null = null;
  const load = loadInvoke ?? (async () => (await import('@tauri-apps/api/core')).invoke as Invoke);
  return async (request) => {
    invokePromise ??= load();
    const invoke = await invokePromise;
    const call = invoke('api', {
      method: request.method,
      path: request.path,
      query: cleanQuery(request.query) ?? null,
      body: request.body === undefined ? null : request.body,
    }).catch((error: unknown) => {
      throw toProblem(error);
    });
    return raceAbort(call, request.signal);
  };
}
