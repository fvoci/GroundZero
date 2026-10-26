import { afterEach, describe, expect, it } from 'vitest';

import { applyDocFont, setDocFont, storedDocFont } from '$lib/doc-font';

afterEach(() => {
  window.localStorage.clear();
  applyDocFont('sans');
});

describe('document body typeface (D-86)', () => {
  it('defaults to sans and keeps a serif choice in this browser only', () => {
    const originalStorage = { ...window.localStorage };
    expect(storedDocFont()).toBe('sans');
    expect(document.documentElement.dataset.docFont).toBeUndefined();
    setDocFont('serif');
    expect(storedDocFont()).toBe('serif');
    expect(document.documentElement.dataset.docFont).toBe('serif');
    setDocFont('sans');
    expect(storedDocFont()).toBe('sans');
    expect({ ...window.localStorage }).toEqual(originalStorage);
    expect(document.documentElement.dataset.docFont).toBeUndefined();
  });
});
