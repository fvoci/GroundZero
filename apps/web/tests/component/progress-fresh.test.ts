import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CourseView, EpisodeView } from '$lib/api/client';
import { api } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

function course(note: string, position: number | null): CourseView {
  const episode: EpisodeView = {
    id: 'ep-1',
    number: 1,
    title: '1강',
    chapter: '',
    duration_seconds: 600,
    url: '',
    materials: '',
    planned_on: null,
    mine: {
      status: 'in_progress',
      planned_on: null,
      completed_on: null,
      watch_count: 0,
      last_position_seconds: position,
      note,
    },
  };
  return {
    details: {
      subject: '',
      instructor: '',
      platform: '',
      url: '',
      textbook: '',
      starts_on: null,
      expires_on: null,
      target_on: null,
    },
    episodes: [episode],
    can_write: true,
  };
}

// 다른 탭·기기에서 바꾼 메모·위치를 옛 화면 값으로 덮지 않는다(제품 로직 점검 8번 불변식).
describe('episode progress', () => {
  it('keeps the note and position another tab saved when marking done', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
    vi.spyOn(api, 'course')
      .mockResolvedValueOnce(course('옛 메모', 30))
      .mockResolvedValue(course('다른 탭에서 쓴 메모', 420));
    const set = vi.spyOn(api, 'setEpisodeProgress').mockResolvedValue(undefined);
    render(CoursePanel, { props: { itemId: 'item-1' } });

    const done = await screen.findByRole('button', { name: /완료/ });
    await fireEvent.click(done);
    await waitFor(() => {
      expect(set).toHaveBeenCalledTimes(1);
    });
    expect(set.mock.calls[0]?.[1]).toMatchObject({
      status: 'done',
      note: '다른 탭에서 쓴 메모',
      last_position_seconds: 420,
    });
  });
});
