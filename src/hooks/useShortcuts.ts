import { useEffect, useRef } from 'react';
import { SHORTCUTS, matchesShortcut, type ShortcutId } from '../lib/shortcuts';

type Handlers = Partial<Record<ShortcutId, () => void>>;

function isTyping(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable);
}

/**
 * Bind app-wide shortcuts. Modifier shortcuts work everywhere; bare-key
 * shortcuts (like "/") are ignored while the user is typing.
 */
export function useShortcuts(handlers: Handlers) {
  const ref = useRef(handlers);
  ref.current = handlers;

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      for (const id of Object.keys(SHORTCUTS) as ShortcutId[]) {
        const handler = ref.current[id];
        if (!handler || !matchesShortcut(e, id)) continue;
        if (!SHORTCUTS[id].keys.includes('mod') && isTyping(e.target)) continue;
        e.preventDefault();
        handler();
        return;
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);
}
