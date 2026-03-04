import { create } from 'zustand';

type ThemeMode = 'light' | 'dark' | 'system';
type ResolvedTheme = 'light' | 'dark';

interface ThemeState {
  theme: ThemeMode;
  resolvedTheme: ResolvedTheme;
  setTheme: (theme: ThemeMode) => void;
  initialize: () => void;
}

function getSystemTheme(): ResolvedTheme {
  if (typeof window === 'undefined') return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function resolveTheme(theme: ThemeMode): ResolvedTheme {
  if (theme === 'system') return getSystemTheme();
  return theme;
}

function applyTheme(resolved: ResolvedTheme): void {
  document.documentElement.setAttribute('data-theme', resolved);
}

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: 'system',
  resolvedTheme: 'light',

  setTheme: (theme: ThemeMode) => {
    const resolved = resolveTheme(theme);
    localStorage.setItem('vectorless-theme', theme);
    applyTheme(resolved);
    set({ theme, resolvedTheme: resolved });
  },

  initialize: () => {
    const stored = localStorage.getItem('vectorless-theme') as ThemeMode | null;
    const theme = stored || 'system';
    const resolved = resolveTheme(theme);
    applyTheme(resolved);
    set({ theme, resolvedTheme: resolved });

    // Listen for system preference changes
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const handleChange = () => {
      const current = get().theme;
      if (current === 'system') {
        const newResolved = getSystemTheme();
        applyTheme(newResolved);
        set({ resolvedTheme: newResolved });
      }
    };
    mediaQuery.addEventListener('change', handleChange);
  },
}));
