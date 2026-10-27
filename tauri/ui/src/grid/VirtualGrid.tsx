// A virtualized, sortable grid with a frozen first column. Only the rows on screen (plus overscan) exist in the DOM;
// the window is one translated block, so scrolling by a row adds one row and removes one (keyed <For>), and sorting
// only re-slices an index array. Fixed row height: no measuring, no layout thrash.
import { For, Show, createMemo, createSignal, onCleanup, onMount, type JSX } from 'solid-js';
import type { Tone } from '../format/format';
import { Icon } from '../components/Icon';
import { windowFor, type SortDir, type SortKind, type SortSpec, type SortValue } from './logic';

export interface GridColumn<T> {
  key: string;
  header: string;
  /** Header tooltip (full label when the header is abbreviated). */
  title?: string;
  width: number;
  numeric?: boolean;
  frozen?: boolean;
  text: (row: T) => string;
  tone?: (row: T) => Tone | undefined;
  render?: (row: T) => JSX.Element;
  href?: (row: T) => string;
  sort?: { value: (row: T) => SortValue; kind: SortKind; firstDir?: SortDir };
}

export function VirtualGrid<T>(props: {
  rows: readonly T[];
  order: readonly number[];
  columns: GridColumn<T>[];
  rowHeight: number;
  height: string;
  sort: SortSpec | null;
  onSort: (column: GridColumn<T>) => void;
  onOpen?: (row: T) => void;
  label: string;
  help?: string;
  empty?: JSX.Element;
  ref?: (el: HTMLDivElement) => void;
}) {
  let scroller!: HTMLDivElement;
  const [scrollTop, setScrollTop] = createSignal(0);
  const [viewport, setViewport] = createSignal(600);
  const [active, setActive] = createSignal(-1);

  const template = createMemo(() => props.columns.map((c) => `${c.width}px`).join(' '));
  const totalWidth = createMemo(() => props.columns.reduce((sum, c) => sum + c.width, 0));
  const range = createMemo(() => windowFor(scrollTop(), viewport(), props.rowHeight, props.order.length));
  const items = createMemo(() => {
    const { start, end } = range();
    const out: T[] = new Array(Math.max(0, end - start));
    for (let i = start; i < end; i++) out[i - start] = props.rows[props.order[i]];
    return out;
  });

  onMount(() => {
    props.ref?.(scroller);
    const observer = new ResizeObserver(() => setViewport(scroller.clientHeight));
    observer.observe(scroller);
    setViewport(scroller.clientHeight);
    onCleanup(() => observer.disconnect());
  });

  const scrollToRow = (index: number) => {
    const header = props.rowHeight + 2;
    const top = index * props.rowHeight;
    const bottom = top + props.rowHeight;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (bottom > scroller.scrollTop + scroller.clientHeight - header) scroller.scrollTop = bottom - scroller.clientHeight + header;
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const count = props.order.length;
    if (count === 0) return;
    const page = Math.max(1, Math.floor(viewport() / props.rowHeight) - 2);
    let next = active();
    switch (event.key) {
      case 'ArrowDown':
        next = Math.min(count - 1, next + 1);
        break;
      case 'ArrowUp':
        next = Math.max(0, next - 1);
        break;
      case 'PageDown':
        next = Math.min(count - 1, next + page);
        break;
      case 'PageUp':
        next = Math.max(0, next - page);
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = count - 1;
        break;
      case 'Enter':
        if (next >= 0 && props.onOpen) {
          event.preventDefault();
          props.onOpen(props.rows[props.order[next]]);
        }
        return;
      default:
        return;
    }
    event.preventDefault();
    setActive(next);
    scrollToRow(next);
  };

  const ariaSort = (column: GridColumn<T>): 'ascending' | 'descending' | 'none' | undefined => {
    if (!column.sort) return undefined;
    if (props.sort?.key !== column.key) return 'none';
    return props.sort.dir === 'asc' ? 'ascending' : 'descending';
  };

  return (
    <div
      ref={scroller}
      class="vg"
      style={{ height: props.height }}
      role="grid"
      aria-label={props.label}
      aria-rowcount={props.order.length + 1}
      aria-colcount={props.columns.length}
      aria-describedby={undefined}
      tabIndex={0}
      title={props.help}
      onScroll={() => setScrollTop(scroller.scrollTop)}
      onKeyDown={onKeyDown}
      onFocus={() => active() < 0 && props.order.length > 0 && setActive(Math.floor(scroller.scrollTop / props.rowHeight))}
    >
      <div class="vg-head" role="row" aria-rowindex={1} style={{ 'grid-template-columns': template(), width: `${totalWidth()}px`, height: `${props.rowHeight + 2}px` }}>
        <For each={props.columns}>
          {(column) => (
            <div role="columnheader" class={`vg-hcell ${column.numeric ? 'numeric' : ''} ${column.frozen ? 'frozen' : ''}`} aria-sort={ariaSort(column)} title={column.title ?? column.header}>
              <Show when={column.sort} fallback={<span class="vg-hlabel">{column.header}</span>}>
                <button type="button" class="vg-sort" tabIndex={-1} onClick={() => props.onSort(column)}>
                  <span class="vg-hlabel">{column.header}</span>
                  <Show when={props.sort?.key === column.key}>
                    <Icon name={props.sort?.dir === 'asc' ? 'sortUp' : 'sortDown'} size={12} />
                  </Show>
                </button>
              </Show>
            </div>
          )}
        </For>
      </div>
      <Show when={props.order.length > 0} fallback={<div class="vg-empty">{props.empty}</div>}>
        <div class="vg-body" style={{ height: `${props.order.length * props.rowHeight}px`, width: `${totalWidth()}px` }}>
          <div class="vg-win" classList={{ odd: range().start % 2 === 1 }} style={{ transform: `translate3d(0, ${range().start * props.rowHeight}px, 0)` }}>
            <For each={items()}>
              {(row, i) => (
                <div
                  class="vg-row"
                  role="row"
                  aria-rowindex={range().start + i() + 2}
                  classList={{ active: active() === range().start + i() }}
                  style={{ 'grid-template-columns': template(), height: `${props.rowHeight}px` }}
                  onClick={(event) => {
                    setActive(range().start + i());
                    if ((event.target as HTMLElement).closest('a')) return;
                    props.onOpen?.(row);
                  }}
                >
                  <For each={props.columns}>
                    {(column) => {
                      const tone = column.tone?.(row);
                      return (
                        <div role="gridcell" class={`vg-cell ${column.numeric ? 'numeric num' : ''} ${column.frozen ? 'frozen' : ''} ${tone ? `chg ${tone}` : ''}`}>
                          {column.render ? column.render(row) : column.href ? <a href={column.href(row)} tabIndex={-1}>{column.text(row)}</a> : column.text(row)}
                        </div>
                      );
                    }}
                  </For>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  );
}
