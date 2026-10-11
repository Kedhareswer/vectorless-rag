import { describe, it, expect } from 'vitest';
import { matchesShortcut, shortcutLabel } from './shortcuts';

const key = (k: string, mods: Partial<KeyboardEvent> = {}) => ({
  key: k, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods,
});

describe('shortcuts', () => {
  it('labels shortcuts per platform', () => {
    expect(shortcutLabel('switchChat', true)).toBe('⌘K');
    expect(shortcutLabel('switchChat', false)).toBe('Ctrl+K');
    expect(shortcutLabel('settings', false)).toBe('Ctrl+,');
    expect(shortcutLabel('focusInput', false)).toBe('/');
  });

  it('matches the platform modifier only', () => {
    expect(matchesShortcut(key('k', { ctrlKey: true }), 'switchChat', false)).toBe(true);
    expect(matchesShortcut(key('k', { metaKey: true }), 'switchChat', false)).toBe(false);
    expect(matchesShortcut(key('k', { metaKey: true }), 'switchChat', true)).toBe(true);
    expect(matchesShortcut(key('K', { ctrlKey: true }), 'switchChat', false)).toBe(true);
    expect(matchesShortcut(key('k'), 'switchChat', false)).toBe(false);
    expect(matchesShortcut(key('k', { ctrlKey: true, altKey: true }), 'switchChat', false)).toBe(false);
    expect(matchesShortcut(key('/'), 'focusInput', false)).toBe(true);
    expect(matchesShortcut(key('/', { ctrlKey: true }), 'focusInput', false)).toBe(false);
  });
});
