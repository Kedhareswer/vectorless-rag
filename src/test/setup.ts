import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri IPC — all invoke calls return undefined by default.
// Individual tests can override with vi.mocked(invoke).mockResolvedValueOnce(...)
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  Channel: vi.fn().mockImplementation(() => ({
    onmessage: null,
  })),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn().mockResolvedValue(undefined),
}));
