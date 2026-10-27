// A plain (non-virtualized) table for short lists: movers, screens, events, health. Optional client-side sorting.
import { For, Show, createMemo, createSignal, type JSX } from 'solid-js';
import { sortIndices, type SortDir, type SortKind, type SortValue } from '../grid/logic';
import { Icon } from './Icon';

export interface TableColumn<T> {
  key: string;
  header: string;
  numeric?: boolean;
  width?: string;
  cell: (row: T, index: number) => JSX.Element;
  sort?: { value: (row: T) => SortValue; kind: SortKind };
  class?: string;
}

const collator = new Intl.Collator('fa');

export function Table<T>(props: {
  rows: readonly T[];
  columns: TableColumn<T>[];
  label: string;
  onRowClick?: (row: T) => void;
  empty?: JSX.Element;
  maxHeight?: string;
  class?: string;
}) {
  const [sort, setSort] = createSignal<{ key: string; dir: SortDir } | null>(null);
  // Rows are keyed by object identity: a refetch (new objects) re-renders, a re-sort only moves rows.
  const ordered = createMemo(() => {
    const spec = sort();
    const column = spec ? props.columns.find((c) => c.key === spec.key) : undefined;
    if (!spec || !column?.sort) return props.rows.slice();
    return sortIndices(props.rows, column.sort.value, spec.dir, column.sort.kind, collator).map((i) => props.rows[i]);
  });
  const toggle = (column: TableColumn<T>) => {
    const current = sort();
    if (current?.key === column.key) setSort(current.dir === 'desc' ? { key: column.key, dir: 'asc' } : null);
    else setSort({ key: column.key, dir: column.sort?.kind === 'text' ? 'asc' : 'desc' });
  };

  return (
    <div class={`table-wrap ${props.class ?? ''}`} style={{ 'max-height': props.maxHeight }}>
      <table class="table" aria-label={props.label}>
        <thead>
          <tr>
            <For each={props.columns}>
              {(column) => (
                <th
                  scope="col"
                  class={`${column.numeric ? 'numeric' : ''} ${column.class ?? ''}`}
                  style={{ width: column.width }}
                  aria-sort={column.sort ? (sort()?.key === column.key ? (sort()!.dir === 'asc' ? 'ascending' : 'descending') : 'none') : undefined}
                >
                  <Show when={column.sort} fallback={column.header}>
                    <button type="button" class="th-sort" onClick={() => toggle(column)}>
                      {column.header}
                      <Show when={sort()?.key === column.key}>
                        <Icon name={sort()!.dir === 'asc' ? 'sortUp' : 'sortDown'} size={12} />
                      </Show>
                    </button>
                  </Show>
                </th>
              )}
            </For>
          </tr>
        </thead>
        <tbody>
          <For each={ordered()} fallback={<tr><td class="table-empty" colSpan={props.columns.length}>{props.empty}</td></tr>}>
            {(row, i) => {
              return (
                <tr
                  class={props.onRowClick ? 'clickable' : undefined}
                  onClick={(event) => {
                    if (!props.onRowClick || (event.target as HTMLElement).closest('a,button')) return;
                    props.onRowClick(row);
                  }}
                >
                  <For each={props.columns}>
                    {(column) => <td class={`${column.numeric ? 'numeric num' : ''} ${column.class ?? ''}`}>{column.cell(row, i())}</td>}
                  </For>
                </tr>
              );
            }}
          </For>
        </tbody>
      </table>
    </div>
  );
}
