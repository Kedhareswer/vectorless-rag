/**
 * App-wide keyboard shortcuts. One list drives both the key handling
 * (useShortcuts) and the hints shown in tooltips (shortcutLabel), so they
 * can't drift apart. `mod` is Cmd on macOS and Ctrl elsewhere.
 */
export const SHORTCUTS = {
  newChat: { keys: 'mod+n', description: 'New chat' },
  switchChat: { keys: 'mod+k', description: 'Switch chat' },
  documents: { keys: 'mod+d', description: 'Documents' },
  trace: { keys: 'mod+j', description: 'Trace & evaluation' },
  batch: { keys: 'mod+b', description: 'Batch questions' },
  settings: { keys: 'mod+,', description: 'Settings' },
  focusInput: { keys: '/', description: 'Focus message box' },
} as const;

export type ShortcutId = keyof typeof SHORTCUTS;

export const isMac = (): boolean =>
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** Human-readable keys, e.g. "⌘K" on macOS, "Ctrl+K" elsewhere. */
export function shortcutLabel(id: ShortcutId, mac = isMac()): string {
  const parts = SHORTCUTS[id].keys.split('+');
  const key = parts[parts.length - 1].toUpperCase();
  if (!parts.includes('mod')) return key;
  return mac ? `⌘${key}` : `Ctrl+${key}`;
}

/** Tooltip text with the shortcut appended, e.g. "Settings (Ctrl+,)". */
export function withShortcut(label: string, id: ShortcutId): string {
  return `${label} (${shortcutLabel(id)})`;
}

/** Whether a keyboard event matches a shortcut definition. */
export function matchesShortcut(e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>, id: ShortcutId, mac = isMac()): boolean {
  const parts = SHORTCUTS[id].keys.split('+');
  const key = parts[parts.length - 1];
  const wantsMod = parts.includes('mod');
  const mod = mac ? e.metaKey : e.ctrlKey;
  if (wantsMod !== mod || e.altKey) return false;
  return e.key.toLowerCase() === key;
}
