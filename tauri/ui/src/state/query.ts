// A small data-loading primitive: re-fetches when its key changes, aborts the previous request and on unmount (route
// change), and keeps the previous data while reloading so the page does not flash back to a skeleton.
import { batch, createEffect, createSignal, on, onCleanup, untrack, type Accessor } from 'solid-js';
import { isAbort, ProblemError } from '../api';
import { toProblem } from '../api/transport';

export interface Query<T> {
  data: Accessor<T | undefined>;
  error: Accessor<ProblemError | undefined>;
  loading: Accessor<boolean>;
  /** True once a first answer (data or error) arrived. */
  settled: Accessor<boolean>;
  refetch: (force?: boolean) => void;
  mutate: (value: T | undefined) => void;
}

export function createQuery<T, K>(
  source: Accessor<K | false | null | undefined>,
  fetcher: (key: K, signal: AbortSignal, force: boolean) => Promise<T>,
): Query<T> {
  const [data, setData] = createSignal<T | undefined>(undefined);
  const [error, setError] = createSignal<ProblemError | undefined>(undefined);
  const [loading, setLoading] = createSignal(Boolean(untrack(source)));
  const [settled, setSettled] = createSignal(false);
  let controller: AbortController | undefined;
  let generation = 0;

  const run = (key: K | false | null | undefined, force: boolean) => {
    controller?.abort();
    if (key === false || key === null || key === undefined) {
      setLoading(false);
      return;
    }
    const current = new AbortController();
    controller = current;
    const id = ++generation;
    setLoading(true);
    fetcher(key, current.signal, force).then(
      (value) => {
        if (id !== generation) return;
        batch(() => {
          setData(() => value);
          setError(undefined);
          setLoading(false);
          setSettled(true);
        });
      },
      (reason: unknown) => {
        if (id !== generation || isAbort(reason)) return;
        batch(() => {
          setError(toProblem(reason));
          setLoading(false);
          setSettled(true);
        });
      },
    );
  };

  createEffect(on(source, (key) => run(key, false)));
  onCleanup(() => {
    generation++;
    controller?.abort();
  });

  return {
    data,
    error,
    loading,
    settled,
    refetch: (force = true) => run(untrack(source), force),
    mutate: (value) => setData(() => value),
  };
}
