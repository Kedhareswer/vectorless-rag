import { describe, expect, it } from 'vitest';
import { batchLabel } from './useAddDocument';

describe('batchLabel', () => {
  it('shows the idle label when no batch is running', () => {
    expect(batchLabel(null, 'Add')).toBe('Add');
  });

  it('shows a plain busy label for a single file', () => {
    expect(batchLabel({ done: 0, total: 1, failed: 0 }, 'Add')).toBe('Adding…');
  });

  it('counts the file being added, not the ones finished', () => {
    expect(batchLabel({ done: 0, total: 5, failed: 0 }, 'Add')).toBe('Adding 1 of 5…');
    expect(batchLabel({ done: 2, total: 5, failed: 1 }, 'Add')).toBe('Adding 3 of 5…');
  });

  it('never counts past the total', () => {
    expect(batchLabel({ done: 5, total: 5, failed: 0 }, 'Add')).toBe('Adding 5 of 5…');
  });
});
