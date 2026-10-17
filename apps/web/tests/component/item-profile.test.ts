import { beforeEach, describe, expect, it } from 'vitest';

import { detailFields, peopleLabels, toDetails } from '$lib/features/modules/profile';
import { setLocale } from '$lib/paraglide/runtime.js';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
});

describe('item profile fields', () => {
  it('sends only the fields of the module kind, trimmed, with unique keywords', () => {
    expect(
      toDetails(
        'research',
        { field: ' 교육학 ', question: '', level: '중급' },
        '동기, 학습,동기, ',
      ),
    ).toEqual({ field: '교육학', keywords: ['동기', '학습'] });
    expect(toDetails('hobby', { activity_type: '공예', field: '미술' }, '키워드')).toEqual({
      activity_type: '공예',
    });
    expect(toDetails('project', { field: 'x' }, 'y')).toEqual({});
  });

  it('names people by module kind', () => {
    expect(peopleLabels('research')).toEqual({ leader: '책임자', members: '공동 연구자' });
    expect(peopleLabels('study').members).toBe('참여자');
    expect(detailFields('course')).toEqual([]);
  });
});
