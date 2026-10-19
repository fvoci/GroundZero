import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CourseView, EpisodeView } from '$lib/api/client';
import { api } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

function episode(over: Partial<EpisodeView> = {}): EpisodeView {
  return {
    id: 'e1',
    number: 1,
    title: '벡터',
    chapter: '1부',
    duration_seconds: 750,
    url: '',
    materials: '',
    planned_on: null,
    mine: {
      status: 'not_started',
      planned_on: null,
      completed_on: null,
      watch_count: 0,
      last_position_seconds: null,
      note: '',
    },
    ...over,
  };
}

const course = (e: EpisodeView, canWrite = true): CourseView => ({
  can_write: canWrite,
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
  episodes: [e],
});

function mockCourse(...views: CourseView[]): void {
  const spy = vi.spyOn(api, 'course');
  for (const v of views) spy.mockResolvedValueOnce(v);
  const last = views.at(-1);
  if (last !== undefined) spy.mockResolvedValue(last);
  vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
}

const field = (id: string): HTMLInputElement | HTMLTextAreaElement => {
  const el = document.getElementById(id);
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return el;
  throw new Error(id);
};

// 강의 회차 공통 정보(번호·제목·챕터·길이·계획일·링크·자료) 고치기: 기존 PUT /api/episodes/{id}를 쓴다. 읽기
// 전용(보관 등)이면 고치기 단추가 없고, 먼저 고친 사람이 있으면 덮지 않고 알린다.
describe('editing a course episode', () => {
  it('saves link and materials, shows the materials, and stops once on a newer version', async () => {
    await setLocale('ko', { reload: false });
    const saved = episode({ url: 'https://example.test/1', materials: '슬라이드 3쪽' });
    // 처음 → 저장 직전 확인(다른 사람이 제목을 고침) → 다시 저장 직전 확인 → 저장 뒤 다시 읽기.
    const other = episode({ title: '벡터 공간' });
    mockCourse(course(episode()), course(other), course(other), course(saved));
    const update = vi.spyOn(api, 'updateEpisode').mockResolvedValue(undefined);
    render(CoursePanel, { props: { itemId: 'i1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '1회 공통 정보 고치기' }));

    await fireEvent.input(field('ep-url-e1'), { target: { value: 'https://example.test/1' } });
    await fireEvent.input(field('ep-materials-e1'), { target: { value: '슬라이드 3쪽' } });
    const form = screen.getByRole('form', { name: '1회 공통 정보' });
    await fireEvent.submit(form);
    expect(await screen.findByRole('alert')).toHaveTextContent('먼저 고쳤습니다');
    expect(update).not.toHaveBeenCalled();
    expect(field('ep-materials-e1').value).toBe('슬라이드 3쪽');

    await fireEvent.submit(form);
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    expect(update.mock.calls[0]).toEqual([
      'e1',
      {
        number: 1,
        title: '벡터',
        chapter: '1부',
        duration_seconds: 750,
        url: 'https://example.test/1',
        materials: '슬라이드 3쪽',
        planned_on: null,
      },
    ]);
    expect(await screen.findByText('슬라이드 3쪽')).toBeInTheDocument();
  });

  it('offers no edit when the course is read-only', async () => {
    await setLocale('ko', { reload: false });
    mockCourse(course(episode(), false));
    render(CoursePanel, { props: { itemId: 'i1' } });
    await screen.findByText('벡터');
    expect(screen.queryByRole('button', { name: '1회 공통 정보 고치기' })).toBeNull();
  });
});
