import { describe, expect, it, vi } from 'vitest';
import { ApiClient } from './client';
import { ProblemError, buildUrl, cleanQuery, createHttpTransport, createTauriTransport, type ApiRequest, type Transport } from './transport';

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

describe('HTTP transport', () => {
  it('builds /api/v1 URLs and drops null/undefined/empty query values', () => {
    expect(buildUrl('/api/v1/', 'Market/GetOverview', { date: null })).toBe('/api/v1/Market/GetOverview');
    expect(buildUrl('/api/v1/', 'Market/Search', { text: 'فولاد', take: 10, x: undefined, y: '' })).toBe(
      '/api/v1/Market/Search?text=%D9%81%D9%88%D9%84%D8%A7%D8%AF&take=10',
    );
    expect(cleanQuery({ a: null })).toBeUndefined();
  });

  it('sends JSON bodies and parses JSON answers, keeping 64-bit ids as strings', async () => {
    const fetch = vi.fn(async () => json([{ insCode: '46348559193224090', symbol: 'فولاد' }]));
    const transport = createHttpTransport({ fetch: fetch as unknown as typeof globalThis.fetch });
    const result = (await transport({ method: 'POST', path: 'Market/GetMarketWatch', body: { tradedOnly: true } })) as { insCode: string }[];
    expect(result[0].insCode).toBe('46348559193224090');
    const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/v1/Market/GetMarketWatch');
    expect(init.method).toBe('POST');
    expect((init.headers as Record<string, string>)['Content-Type']).toBe('application/json');
    expect(init.body).toBe('{"tradedOnly":true}');
  });

  it('turns RFC 7807 bodies into ProblemErrors', async () => {
    const transport = createHttpTransport({ fetch: (async () => json({ title: 'Invalid filter', detail: 'SyntaxError: Unexpected token', status: 400 }, 400)) as unknown as typeof fetch });
    const error = await transport({ method: 'GET', path: 'x' }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ProblemError);
    expect(error).toMatchObject({ title: 'Invalid filter', detail: 'SyntaxError: Unexpected token', status: 400, kind: 'problem' });
  });

  it('reports non-JSON failures, network failures and aborts distinctly', async () => {
    const plain = createHttpTransport({ fetch: (async () => new Response('gateway down', { status: 502, statusText: 'Bad Gateway' })) as unknown as typeof fetch });
    await expect(plain({ method: 'GET', path: 'x' })).rejects.toMatchObject({ title: 'Bad Gateway', status: 502 });

    const offline = createHttpTransport({ fetch: (async () => { throw new TypeError('Failed to fetch'); }) as unknown as typeof fetch });
    await expect(offline({ method: 'GET', path: 'x' })).rejects.toMatchObject({ kind: 'network', status: 0 });

    const controller = new AbortController();
    const slow = createHttpTransport({
      fetch: ((_: string, init: RequestInit) =>
        new Promise((_, reject) => init.signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError'))))) as unknown as typeof fetch,
    });
    const pending = slow({ method: 'GET', path: 'x', signal: controller.signal });
    controller.abort();
    await expect(pending).rejects.toMatchObject({ kind: 'aborted' });
  });

  it('sends the operator token only on operator routes', async () => {
    const fetch = vi.fn(async () => json('42'));
    const transport = createHttpTransport({ fetch: fetch as unknown as typeof globalThis.fetch, operatorToken: () => 'secret' });
    await transport({ method: 'POST', path: 'MarketData/RunJob/eod', operator: true });
    await transport({ method: 'GET', path: 'Market/GetOverview' });
    const headers = (fetch.mock.calls as unknown as [string, RequestInit][]).map(([, init]) => init.headers as Record<string, string>);
    expect(headers[0].Authorization).toBe('Bearer secret');
    expect(headers[1].Authorization).toBeUndefined();
  });
});

describe('Tauri transport', () => {
  it("calls invoke('api', { method, path, query, body })", async () => {
    const invoke = vi.fn(async () => ({ ok: true }));
    const transport = createTauriTransport(async () => invoke);
    await transport({ method: 'GET', path: 'Instrument/GetBars/46348559193224090', query: { from: '2026-01-01', x: null } });
    expect(invoke).toHaveBeenCalledWith('api', { method: 'GET', path: 'Instrument/GetBars/46348559193224090', query: { from: '2026-01-01' }, body: null });
  });

  it('maps rejected problem objects (and JSON strings) to ProblemErrors', async () => {
    const objectReject = createTauriTransport(async () => async () => Promise.reject({ title: 'Not Found', detail: 'Instrument 1 is unknown.', status: 404 }));
    await expect(objectReject({ method: 'GET', path: 'x' })).rejects.toMatchObject({ title: 'Not Found', status: 404 });
    const stringReject = createTauriTransport(async () => async () => Promise.reject('{"title":"Bad","detail":null,"status":400}'));
    await expect(stringReject({ method: 'GET', path: 'x' })).rejects.toMatchObject({ title: 'Bad', status: 400, detail: null });
  });
});

describe('ApiClient: de-duplication, cache, abort', () => {
  function deferredTransport() {
    const calls: { request: ApiRequest; resolve: (v: unknown) => void; reject: (e: unknown) => void }[] = [];
    const transport: Transport = (request) =>
      new Promise((resolve, reject) => {
        calls.push({ request, resolve, reject });
        request.signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
      });
    return { calls, transport };
  }

  it('shares one request between concurrent identical calls', async () => {
    const { calls, transport } = deferredTransport();
    const client = new ApiClient(transport, 'http');
    const a = client.call({ method: 'GET', path: 'Market/GetSectors', query: { date: null } });
    const b = client.call({ method: 'GET', path: 'Market/GetSectors' });
    expect(calls).toHaveLength(1);
    calls[0].resolve([1, 2]);
    await expect(a).resolves.toEqual([1, 2]);
    await expect(b).resolves.toEqual([1, 2]);
  });

  it('serves GETs from the cache until the TTL expires; force bypasses it', async () => {
    let now = 1000;
    const transport = vi.fn(async () => ({ n: transport.mock.calls.length }));
    const client = new ApiClient(transport, 'http', () => now);
    const request = { method: 'GET' as const, path: 'Market/GetOverview' };
    expect(await client.call(request, { ttl: 5000 })).toEqual({ n: 1 });
    expect(await client.call(request, { ttl: 5000 })).toEqual({ n: 1 });
    now += 6000;
    expect(await client.call(request, { ttl: 5000 })).toEqual({ n: 2 });
    expect(await client.call(request, { ttl: 5000, force: true })).toEqual({ n: 3 });
    // POSTs are not cached by default.
    await client.call({ method: 'POST', path: 'MarketData/RunJob/eod' });
    await client.call({ method: 'POST', path: 'MarketData/RunJob/eod' });
    expect(transport).toHaveBeenCalledTimes(5);
  });

  it('never caches failures', async () => {
    let fail = true;
    const client = new ApiClient(async () => {
      if (fail) throw { title: 'Boom', detail: null, status: 500 };
      return 'ok';
    }, 'http');
    await expect(client.call({ method: 'GET', path: 'x' })).rejects.toBeInstanceOf(ProblemError);
    fail = false;
    await expect(client.call({ method: 'GET', path: 'x' })).resolves.toBe('ok');
  });

  it('aborts a waiter without cancelling a request others still wait for', async () => {
    const { calls, transport } = deferredTransport();
    const client = new ApiClient(transport, 'http');
    const first = new AbortController();
    const a = client.call({ method: 'GET', path: 'x' }, { signal: first.signal });
    const b = client.call({ method: 'GET', path: 'x' });
    first.abort();
    await expect(a).rejects.toMatchObject({ kind: 'aborted' });
    expect(calls[0].request.signal?.aborted).toBe(false);
    calls[0].resolve('shared');
    await expect(b).resolves.toBe('shared');
  });

  it('cancels the underlying request when its last waiter aborts (route change)', async () => {
    const { calls, transport } = deferredTransport();
    const client = new ApiClient(transport, 'http');
    const controller = new AbortController();
    const a = client.call({ method: 'GET', path: 'y' }, { signal: controller.signal });
    controller.abort();
    await expect(a).rejects.toMatchObject({ kind: 'aborted' });
    expect(calls[0].request.signal?.aborted).toBe(true);
    // A new call after the abort starts a fresh request.
    const again = client.call({ method: 'GET', path: 'y' });
    expect(calls).toHaveLength(2);
    calls[1].resolve(7);
    await expect(again).resolves.toBe(7);
  });

  it('rejects immediately when the signal is already aborted', async () => {
    const transport = vi.fn(async () => 1);
    const client = new ApiClient(transport, 'http');
    const controller = new AbortController();
    controller.abort();
    await expect(client.call({ method: 'GET', path: 'z' }, { signal: controller.signal })).rejects.toMatchObject({ kind: 'aborted' });
    expect(transport).not.toHaveBeenCalled();
  });
});
