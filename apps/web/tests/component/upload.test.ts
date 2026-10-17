import { describe, expect, it } from 'vitest';

import { formatBytes, missingParts, partRange } from '$lib/features/files/upload';

describe('chunked upload helpers', () => {
  it('splits a file into server-sized parts', () => {
    expect(partRange(1, 5, 12)).toEqual([0, 5]);
    expect(partRange(3, 5, 12)).toEqual([10, 12]);
  });

  it('resumes with only the parts the server has not received', () => {
    expect(missingParts(4, [2, 4])).toEqual([1, 3]);
    expect(missingParts(1, [1])).toEqual([]);
  });

  it('formats sizes for people', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(1536)).toBe('1.5 KB');
    expect(formatBytes(5 * 1024 * 1024 + 10)).toBe('5.0 MB');
  });
});
