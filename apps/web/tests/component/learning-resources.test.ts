import { afterEach, describe, expect, it, vi } from 'vitest';
import { courseResources } from '$lib/features/courses/api';
import { loadCourseView } from '$lib/features/courses/course-data';
import { studyResources } from '$lib/features/study/api';
import { loadStudyView } from '$lib/features/study/study-data';

afterEach(() => vi.restoreAllMocks());

const details = {
  subject: '',
  instructor: '',
  platform: '',
  url: '',
  textbook: '',
  starts_on: null,
  expires_on: null,
  target_on: null,
};
const episode = (id: string) => ({
  id,
  number: 1,
  title: id,
  chapter: '',
  duration_seconds: 60,
  url: '',
  materials: '',
  planned_on: '2026-10-28',
  version: 2,
});
const unit = (id: string) => ({
  id,
  number: 1,
  title: id,
  chapter: '',
  unit: 'pages',
  amount: 10,
  planned_on: '2026-10-28',
  version: 2,
});

describe('learning resource composition', () => {
  it('joins progress by episode ID, keeps resource order and defaults missing private rows', async () => {
    const info = { access: 'write' as const, details };
    const reads = [
      vi.spyOn(courseResources, 'info').mockResolvedValue(info),
      vi
        .spyOn(courseResources, 'episodes')
        .mockResolvedValue([
          episode('b'),
          { ...episode('a'), mine: { note: 'not a private progress resource' } },
        ]),
      vi.spyOn(courseResources, 'progress').mockResolvedValue([
        { episode_id: 'unrelated', note: 'unrelated note' },
        {
          episode_id: 'b',
          status: 'done',
          planned_on: null,
          completed_on: '2026-10-28',
          watch_count: 2,
          last_position_seconds: 25,
          note: 'private b',
        },
      ]),
    ];
    const view = await loadCourseView('item');
    expect(view.episodes.map((row) => row.id)).toEqual(['b', 'a']);
    expect(view.episodes[0]?.mine.note).toBe('private b');
    expect(view.episodes[1]?.mine).toEqual({
      status: 'not_started',
      planned_on: null,
      completed_on: null,
      watch_count: 0,
      last_position_seconds: null,
      note: '',
    });
    expect(view.access).toBe('write');
    for (const read of reads) expect(read).toHaveBeenCalledExactlyOnceWith('item');
  });

  it('joins study progress by unit ID without copying another row into a new unit', async () => {
    const info = { access: 'read' as const };
    const reads = [
      vi.spyOn(studyResources, 'info').mockResolvedValue(info),
      vi.spyOn(studyResources, 'units').mockResolvedValue([unit('a'), unit('b')]),
      vi.spyOn(studyResources, 'progress').mockResolvedValue([
        {
          unit_id: 'b',
          status: 'in_progress',
          done_amount: 3,
          planned_on: '2026-10-29',
          completed_on: null,
          note: 'private b',
        },
      ]),
    ];
    const view = await loadStudyView('item');
    expect(view.units[0]?.mine).toEqual({
      status: 'not_started',
      done_amount: 0,
      planned_on: null,
      completed_on: null,
      note: '',
    });
    expect(view.units[1]?.mine.note).toBe('private b');
    expect(view.units[1]?.mine.planned_on).toBe('2026-10-29');
    expect(view.access).toBe('read');
    for (const read of reads) expect(read).toHaveBeenCalledExactlyOnceWith('item');
  });

  it('rejects a course view if any required resource could not be read', async () => {
    const info = { access: 'write' as const, details };
    vi.spyOn(courseResources, 'info').mockResolvedValue(info);
    vi.spyOn(courseResources, 'episodes').mockResolvedValue([episode('a')]);
    vi.spyOn(courseResources, 'progress').mockRejectedValue(new Error('unavailable'));
    await expect(loadCourseView('item')).rejects.toThrow('unavailable');
  });

  it('rejects a study view if its shared units could not be read', async () => {
    const info = { access: 'read' as const };
    vi.spyOn(studyResources, 'info').mockResolvedValue(info);
    vi.spyOn(studyResources, 'units').mockRejectedValue(new Error('unavailable'));
    vi.spyOn(studyResources, 'progress').mockResolvedValue([]);
    await expect(loadStudyView('item')).rejects.toThrow('unavailable');
  });
});
