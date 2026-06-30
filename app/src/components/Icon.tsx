// Minimal inline icon set (Lucide-style paths). No emoji icons (design rule).
const P: Record<string, string> = {
  dashboard: '<rect x="3" y="3" width="7" height="9" rx="1.5"/><rect x="14" y="3" width="7" height="5" rx="1.5"/><rect x="14" y="12" width="7" height="9" rx="1.5"/><rect x="3" y="16" width="7" height="5" rx="1.5"/>',
  workspace: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/><path d="M9 9h12"/>',
  agents: '<rect x="4" y="8" width="16" height="11" rx="2.5"/><path d="M12 8V5"/><circle cx="12" cy="3.5" r="1.3"/><circle cx="9" cy="13" r="1.1"/><circle cx="15" cy="13" r="1.1"/><path d="M9 17h6"/>',
  terminal: '<path d="M5 7l4 4-4 4"/><path d="M12 16h6"/><rect x="2.5" y="3.5" width="19" height="17" rx="2.5"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M5 5l2 2M17 17l2 2M2 12h3M19 12h3M5 19l2-2M17 7l2-2"/>',
  branch: '<circle cx="6" cy="5" r="2.2"/><circle cx="6" cy="19" r="2.2"/><circle cx="18" cy="8" r="2.2"/><path d="M6 7.2v9.6"/><path d="M18 10.2c0 4-4 3.4-12 6"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2.4h8a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  file: '<path d="M14 3v5h5"/><path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>',
  activity: '<path d="M3 12h4l3 8 4-16 3 8h4"/>',
  cpu: '<rect x="6" y="6" width="12" height="12" rx="2"/><rect x="9" y="9" width="6" height="6" rx="1"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/>',
  coins: '<ellipse cx="9" cy="7" rx="6" ry="3"/><path d="M3 7v5c0 1.6 2.7 3 6 3"/><path d="M3 12c0 1.6 2.7 3 6 3"/><ellipse cx="15" cy="14" rx="6" ry="3"/><path d="M21 14v3c0 1.6-2.7 3-6 3s-6-1.4-6-3"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  play: '<path d="M7 5v14l11-7z"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  chat: '<path d="M21 15a2 2 0 0 1-2 2H8l-4 4V5a2 2 0 0 1 2-2h13a2 2 0 0 1 2 2z"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>',
  chevron: '<path d="m9 6 6 6-6 6"/>',
  zap: '<path d="M13 2 4 14h7l-1 8 9-12h-7z"/>',
  sparkle: '<path d="M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M18 6l-2.5 2.5M8.5 15.5 6 18"/>',
  code: '<path d="m8 6-5 6 5 6M16 6l5 6-5 6"/>',
  layers: '<path d="m12 3 9 5-9 5-9-5z"/><path d="m3 13 9 5 9-5"/>',
  diff: '<path d="M12 3v6M9 6h6"/><path d="M9 18h6"/><rect x="4" y="3" width="16" height="18" rx="2"/>',
  dots: '<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>',
  bell: '<path d="M18 8a6 6 0 1 0-12 0c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M10.3 21a1.9 1.9 0 0 0 3.4 0"/>',
  pause: '<rect x="6" y="5" width="4" height="14" rx="1"/><rect x="14" y="5" width="4" height="14" rx="1"/>',
  arrowRight: '<path d="M5 12h14M13 6l6 6-6 6"/>',
  minus: '<path d="M5 12h14"/>',
  square: '<rect x="5" y="5" width="14" height="14" rx="2"/>',
  folderOpen: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2.4h8a2 2 0 0 1 2 2"/><path d="M3 7v11a2 2 0 0 0 2 2h12.5a2 2 0 0 0 1.9-1.4l2-6A1 1 0 0 0 20.4 11H6.5a2 2 0 0 0-1.9 1.4z"/>',
};

export default function Icon({ name, cls = 'icon' }: { name: string; cls?: string }) {
  return <svg className={cls} viewBox="0 0 24 24" dangerouslySetInnerHTML={{ __html: P[name] ?? '' }} />;
}
