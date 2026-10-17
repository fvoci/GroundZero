import { afterEach, describe, expect, it } from 'vitest';

import type { RecordFieldDef, RecordKindView } from '$lib/api/client';
import {
  recordChoiceLabel,
  recordDateLabel,
  recordFieldLabel,
  recordKindLabel,
  recordRoleLabel,
} from '$lib/features/records/labels';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(async () => {
  await setLocale('ko', { reload: false });
});

// 서버(`records.rs`)가 보내는 한국어 이름 그대로.
const status: RecordFieldDef = {
  key: 'status',
  label: '상태',
  type: 'choice',
  choices: [{ value: 'under_review', label: '심사' }],
};
const submission: RecordKindView = {
  kind: 'submission',
  label: '투고',
  module_kind: 'research',
  date_label: '제출 예정일',
  fields: [status],
  file_roles: [{ value: 'other', label: '제출본·심사 의견·답변서' }],
};

const unknown: RecordKindView = {
  kind: 'future_kind',
  label: '서버 이름',
  module_kind: 'research',
  date_label: '서버 날짜',
  fields: [
    {
      key: 'mood',
      label: '기분',
      type: 'choice',
      choices: [{ value: 'calm', label: '차분' }],
    },
  ],
  file_roles: [{ value: 'sketch', label: '스케치' }],
};

describe('record labels', () => {
  it('shows the Korean labels the server sends', async () => {
    await setLocale('ko', { reload: false });
    expect(recordKindLabel(submission)).toBe('투고');
    expect(recordDateLabel(submission)).toBe('제출 예정일');
    expect(recordFieldLabel(submission, status)).toBe('상태');
    expect(recordChoiceLabel(submission, status, { value: 'under_review', label: '심사' })).toBe(
      '심사',
    );
    expect(recordRoleLabel(submission, { value: 'other', label: 'x' })).toBe(
      '제출본·심사 의견·답변서',
    );
  });

  it('translates kinds, fields, choices and roles by key in English', async () => {
    await setLocale('en', { reload: false });
    expect(recordKindLabel(submission)).toBe('Submission');
    expect(recordDateLabel(submission)).toBe('Planned submission date');
    expect(recordFieldLabel(submission, status)).toBe('Status');
    expect(recordChoiceLabel(submission, status, { value: 'under_review', label: '심사' })).toBe(
      'Under review',
    );
    expect(recordRoleLabel(submission, { value: 'other', label: '제출본·심사 의견·답변서' })).toBe(
      'Manuscript, reviews and responses',
    );
  });

  it('falls back to the server label for unknown keys', async () => {
    await setLocale('en', { reload: false });
    const mood = unknown.fields[0];
    expect(mood).toBeDefined();
    if (mood === undefined) return;
    expect(recordKindLabel(unknown)).toBe('서버 이름');
    expect(recordDateLabel(unknown)).toBe('서버 날짜');
    expect(recordDateLabel({ ...submission, date_label: null })).toBeNull();
    expect(recordFieldLabel(unknown, mood)).toBe('기분');
    expect(recordChoiceLabel(unknown, mood, { value: 'calm', label: '차분' })).toBe('차분');
    expect(recordRoleLabel(unknown, { value: 'sketch', label: '스케치' })).toBe('스케치');
    // 알려진 종류의 모르는 칸·값도 서버 이름을 쓴다.
    expect(recordChoiceLabel(submission, status, { value: 'withdrawn', label: '철회' })).toBe(
      '철회',
    );
  });
});
