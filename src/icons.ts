// Inner SVG markup for a 24x24, stroke-based icon set. Static strings only.
const dot = (cx: number, cy: number) => `<circle cx="${cx}" cy="${cy}" r="1.3" fill="currentColor" stroke="none"/>`;

export const ICONS = {
  palette: `<path d="M12 3a9 9 0 1 0 0 18c1.1 0 1.8-.8 1.8-1.8 0-.5-.2-.9-.5-1.2-.3-.3-.5-.8-.5-1.2 0-1 .8-1.8 1.8-1.8H17a4 4 0 0 0 4-4C21 6.6 17 3 12 3z"/>${dot(7.5, 11.5)}${dot(9.5, 7.5)}${dot(14.5, 7.5)}${dot(17, 11)}`,
  pin: `<path d="M9 3.5h6l-1 5.5 3.2 3.2V14H6.8v-1.8L10 9 9 3.5zM12 14v6.5"/>`,
  send: `<path d="M21 3 3.5 10.2l7 2.9 2.9 7L21 3z"/><path d="m10.5 13.1 5-5"/>`,
  plus: `<path d="M12 5v14M5 12h14"/>`,
  settings: `<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>`,
  close: `<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>`,
  check: `<path d="m5 12.5 4.5 4.5L19 7.5"/>`,
  copy: `<rect x="8.5" y="8.5" width="11.5" height="11.5" rx="2"/><path d="M15.5 8.5V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7.5a2 2 0 0 0 2 2h2.5"/>`,
  terminal: `<rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="m7 9.5 2.5 2.5L7 14.5M12.5 14.5h4"/>`,
  external: `<path d="M14 4h6v6M20 4l-8.5 8.5M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>`,
  mail: `<rect x="3" y="5.5" width="18" height="13" rx="2"/><path d="m3.5 7 8.5 6 8.5-6"/>`,
  download: `<path d="M12 4v11m-4.5-4.5L12 15l4.5-4.5M5 20h14"/>`,
  folder: `<path d="M3 7a2 2 0 0 1 2-2h4l2 2.2h8a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>`,
  back: `<path d="m15 6-6 6 6 6"/>`,
} as const;

export type IconName = keyof typeof ICONS;
