import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CourseView, EpisodeView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
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
    version: 1,
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

// 강의 회차 공통 정보(번호·제목·챕터·길이·계획일·링크·자료) 고치기: PUT /api/episodes/{id}에 읽은 판을 함께 보낸다.
// 읽기 전용(보관 등)이면 고치기 단추가 없고, 먼저 고친 사람이 있으면 서버가 409로 막고 화면은 입력을 지킨 채 알린다.
describe('editing a course episode', () => {
  it('saves link and materials, shows the materials, and stops once on a newer version', async () => {
    await setLocale('ko', { reload: false });
    const saved = episode({ url: 'https://example.test/1', materials: '슬라이드 3쪽', version: 3 });
    // 처음(판 1) → 409 뒤 다시 읽기(다른 사람이 제목을 고쳐 판 2) → 저장 뒤 다시 읽기.
    const other = episode({ title: '벡터 공간', version: 2 });
    mockCourse(course(episode()), course(other), course(saved));
    const update = vi
      .spyOn(api, 'updateEpisode')
      .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'the episode changed'))
      .mockResolvedValue(undefined);
    render(CoursePanel, { props: { itemId: 'i1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '1회 공통 정보 고치기' }));

    await fireEvent.input(field('ep-url-e1'), { target: { value: 'https://example.test/1' } });
    await fireEvent.input(field('ep-materials-e1'), { target: { value: '슬라이드 3쪽' } });
    const form = screen.getByRole('form', { name: '1회 공통 정보' });
    await fireEvent.submit(form);
    expect(await screen.findByRole('alert')).toHaveTextContent('먼저 고쳤습니다');
    expect(update).toHaveBeenCalledTimes(1);
    expect(update.mock.calls[0]?.[1]).toBe(1);
    expect(field('ep-materials-e1').value).toBe('슬라이드 3쪽');

    await fireEvent.submit(form);
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual([
      'e1',
      2,
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

  // 판이 그대로인 409(보관 등)는 판 충돌로 알리지 않고 그 오류를 보인다.
  it('reports a 409 that is not a newer version as the error it is', async () => {
    await setLocale('ko', { reload: false });
    mockCourse(course(episode()));
    vi.spyOn(api, 'updateEpisode').mockRejectedValue(
      new ApiError(409, 'invalid_state', 'archived'),
    );
    render(CoursePanel, { props: { itemId: 'i1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '1회 공통 정보 고치기' }));
    await fireEvent.submit(screen.getByRole('form', { name: '1회 공통 정보' }));
    const alert = await screen.findByRole('alert');
    expect(alert).not.toHaveTextContent('먼저 고쳤습니다');
  });

  it('offers no edit when the course is read-only', async () => {
    await setLocale('ko', { reload: false });
    mockCourse(course(episode(), false));
    render(CoursePanel, { props: { itemId: 'i1' } });
    await screen.findByText('벡터');
    expect(screen.queryByRole('button', { name: '1회 공통 정보 고치기' })).toBeNull();
  });
});
