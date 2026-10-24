import { describe, expect, it } from 'vitest';

import { parseEpisodesCsv } from '$lib/features/courses/csv';
import { exportFileName, zipPath } from '$lib/features/documents/export';
import { normalizeKey } from '$lib/features/modules/kinds';

describe('Unicode export boundaries', () => {
  const file = { id: '0198f1f2-0000-7000-8000-0000000000aa', version: 1 };

  it('keeps a valid ZIP path when the length limit meets an astral character', () => {
    const prefix = 'a'.repeat(79);
    const path = zipPath(file, `${prefix}😀.txt`);
    expect(decodeURIComponent(path.href)).toBe(path.entry);
    expect(path.entry).toBe(`files/000000aa-v1-${prefix}`);
    expect(exportFileName(`${prefix}😀.txt`, 'markdown')).toBe(`${prefix}.md`);
  });

  it('keeps complete pairs within the existing 80 UTF-16 unit limit', () => {
    const name = `${'한'.repeat(78)}😀`;
    const path = zipPath(file, `${name}.txt`);
    expect(decodeURIComponent(path.href)).toBe(path.entry);
    expect(path.entry).toBe(`files/000000aa-v1-${name}`);
    expect(exportFileName('😀'.repeat(41), 'zip')).toBe(`${'😀'.repeat(40)}.zip`);
  });

  it.each(['한글', 'ㄱㅏ', '가', '誤', 'Latin 123', 'e\u0301', '👩‍👩‍👧‍👦'])(
    'preserves multilingual filename text: %s',
    (name) => {
      const path = zipPath(file, name);
      expect(decodeURIComponent(path.href)).toBe(path.entry);
      expect(exportFileName(name, 'markdown')).toBe(`${name}.md`);
    },
  );
});

describe('episode CSV server character limit', () => {
  it.each([
    '한'.repeat(300),
    '誤'.repeat(300),
    '😀'.repeat(300),
    '가'.repeat(150),
    'e\u0301'.repeat(150),
  ])('accepts 300 Unicode code points in a title', (title) => {
    const result = parseEpisodesCsv(`${title},장,45,2026-10-24`);
    expect(result.errors).toEqual([]);
    expect(result.rows[0]?.title).toBe(title);
  });

  it.each(['한', '😀'])('rejects 301 code points: %s', (letter) => {
    const result = parseEpisodesCsv(`${letter.repeat(301)},장,45,2026-10-24`);
    expect(result.rows).toEqual([]);
    expect(result.errors).toHaveLength(1);
  });

  it.each(['', '   '])('rejects a blank title: %j', (title) => {
    const result = parseEpisodesCsv(`${title},장,45,2026-10-24`);
    expect(result.rows).toEqual([]);
    expect(result.errors).toHaveLength(1);
  });

  it('preserves punctuation and spaces in a title', () => {
    const title = '강의 A 123 !?';
    const result = parseEpisodesCsv(`${title},장,45,2026-10-24`);
    expect(result.errors).toEqual([]);
    expect(result.rows[0]?.title).toBe(title);
  });
});

describe('module key server contract', () => {
  it.each(['ß', 'ıd', 'ſa', 'ﬀ', 'ＡＢ', '한글', 'ㄱㅏ', '가', '誤', 'e\u0301', '😀'])(
    'rejects non-ASCII input before casing: %s',
    (key) => {
      expect(normalizeKey(key)).toBeUndefined();
    },
  );

  it('uses Rust White_Space trimming rather than trimming a BOM', () => {
    expect(normalizeKey('\ufeffDEV')).toBeUndefined();
    expect(normalizeKey('\u0085dev\u0085')).toBe('DEV');
  });

  it('retains ASCII casing, reserved keys and length limits', () => {
    expect(normalizeKey(' dev ')).toBe('DEV');
    expect(normalizeKey('a1')).toBe('A1');
    expect(normalizeKey('abcdefghij')).toBe('ABCDEFGHIJ');
    for (const key of ['', 'A', 'abcdefghijk', 'wiki', '1AB', 'A B', 'A-B', '!?']) {
      expect(normalizeKey(key)).toBeUndefined();
    }
  });
});
