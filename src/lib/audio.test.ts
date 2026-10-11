import { describe, it, expect } from 'vitest';
import { downsample, concatChunks, TARGET_SAMPLE_RATE } from './audio';

describe('downsample', () => {
  it('converts 48 kHz to 16 kHz by averaging groups of three', () => {
    const input = new Float32Array([1, 2, 3, 4, 5, 6, 7, 8, 9]);
    expect(Array.from(downsample(input, 48000))).toEqual([2, 5, 8]);
  });

  it('handles non-integer ratios (44.1 kHz) with the expected length', () => {
    const input = new Float32Array(44100).fill(0.5);
    const out = downsample(input, 44100);
    expect(out.length).toBe(TARGET_SAMPLE_RATE);
    expect(out.every((v) => Math.abs(v - 0.5) < 1e-6)).toBe(true);
  });

  it('returns input unchanged when already at or below the target rate', () => {
    const input = new Float32Array([0.1, 0.2]);
    expect(downsample(input, 16000)).toBe(input);
  });
});

describe('concatChunks', () => {
  it('joins chunks in order', () => {
    const out = concatChunks([new Float32Array([1, 2]), new Float32Array([]), new Float32Array([3])]);
    expect(Array.from(out)).toEqual([1, 2, 3]);
  });
});
