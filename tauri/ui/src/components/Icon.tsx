// Inline stroke icons (24×24, currentColor). Decorative by default (aria-hidden); pass `label` to expose one.
import type { JSX } from 'solid-js';

const paths = {
  overview: 'M4 20V10M10 20V4M16 20v-7M22 20H2',
  watch: 'M3 5h18M3 10h18M3 15h18M3 20h18M8 5v15',
  screens: 'M4 5h16l-6 7v6l-4 2v-8z',
  health: 'M3 12h4l2-6 4 12 2-6h6',
  search: 'M11 18a7 7 0 1 1 0-14 7 7 0 0 1 0 14zM16.5 16.5L21 21',
  sun: 'M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4',
  moon: 'M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z',
  language: 'M4 5h9M8.5 3v2M6 5c0 4 3 7 6 8M11 5c0 4-3 7-7 9M13 21l4-9 4 9M14.5 18h5',
  digits: 'M5 8l2-2v12M11 9a2 2 0 1 1 4 0c0 2-4 3-4 7h4M18 6h3l-2 4a2 2 0 1 1-2 3',
  palette: 'M12 3a9 9 0 1 0 0 18c1 0 1.5-.7 1.5-1.5 0-1.2-1-1.5-1-2.5s.8-1.5 2-1.5H17a4 4 0 0 0 4-4c0-4.7-4-8.5-9-8.5zM7.5 11.5h.01M10 7.5h.01M15 7.5h.01',
  density: 'M4 6h16M4 12h16M4 18h16',
  densityLite: 'M4 7h16M4 17h16',
  external: 'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5',
  check: 'M5 12.5l4.5 4.5L19 7.5',
  warning: 'M12 4l9.5 16.5h-19zM12 10v4.5M12 17.5h.01',
  error: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM9 9l6 6M15 9l-6 6',
  info: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5M12 8h.01',
  okCircle: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM8 12.5l2.5 2.5L16 9.5',
  close: 'M6 6l12 12M18 6L6 18',
  chevronDown: 'M6 9l6 6 6-6',
  sortUp: 'M12 5v14M6 11l6-6 6 6',
  sortDown: 'M12 19V5M6 13l6 6 6-6',
  refresh: 'M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7',
  download: 'M12 4v11M7 10l5 5 5-5M5 20h14',
  play: 'M7 5l12 7-12 7z',
  filter: 'M4 5h16l-6 7v6l-4 2v-8z',
  menu: 'M4 6h16M4 12h16M4 18h16',
  command: 'M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z',
  sync: 'M4 12a8 8 0 0 1 14-5.3M20 4v5h-5M20 12a8 8 0 0 1-14 5.3M4 20v-5h5',
  clock: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2',
  symbol: 'M4 19l5-7 4 4 7-10M4 5v14h16',
} as const;

export type IconName = keyof typeof paths;

export function Icon(props: { name: IconName; size?: number; label?: string; class?: string; style?: JSX.CSSProperties }) {
  return (
    <svg
      class={`icon ${props.class ?? ''}`}
      width={props.size ?? 18}
      height={props.size ?? 18}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden={props.label ? undefined : 'true'}
      role={props.label ? 'img' : undefined}
      aria-label={props.label}
      style={props.style}
    >
      <path d={paths[props.name]} />
    </svg>
  );
}
