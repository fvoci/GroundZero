import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CourseView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

const course: CourseView = {
  can_write: true,
  details: {
    subject: '선형대수',
    instructor: '김 강사',
    platform: '',
    url: '',
    textbook: '',
    starts_on: null,
    expires_on: null,
    target_on: null,
  },
  episodes: [],
};

function mockCourse(): void {
  vi.spyOn(api, 'course').mockResolvedValue(course);
  vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
}

const instructor = (): HTMLInputElement | null => {
  const el = document.getElementById('course-instructor');
  return el instanceof HTMLInputElement ? el : null;
};

// 감사 W19: 강좌 정보 저장은 요청을 보내기 전에 폼을 닫았다. 저장이 실패하면(보관됨 409, 권한 403, 연결 실패)
// 오류만 보이고 입력한 강사·플랫폼·교재·날짜가 사라졌다(다시 고치기를 누르면 서버 값으로 채운다). 저장에 성공한
// 뒤에만 닫는다.
describe('course details save', () => {
  it('keeps the form and the typed details open when the save fails', async () => {
    await setLocale('ko', { reload: false });
    mockCourse();
    const save = vi
      .spyOn(api, 'setCourseDetails')
      .mockRejectedValue(new ApiError(409, 'invalid_state', 'the item is archived'));
    render(CoursePanel, { props: { itemId: 'i1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    const field = instructor();
    if (field === null) throw new Error('course-instructor');
    await fireEvent.input(field, { target: { value: '박 강사' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(1);
    });
    expect(await screen.findByRole('alert')).toHaveTextContent('처리하지 못했습니다');
    expect(instructor()?.value).toBe('박 강사');
  });

  it('closes the form after a successful save', async () => {
    await setLocale('ko', { reload: false });
    mockCourse();
    const save = vi.spyOn(api, 'setCourseDetails').mockResolvedValue(undefined);
    render(CoursePanel, { props: { itemId: 'i1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    const field = instructor();
    if (field === null) throw new Error('course-instructor');
    await fireEvent.input(field, { target: { value: '박 강사' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    expect(await screen.findByText('강좌 정보를 저장했습니다.')).toBeInTheDocument();
    expect(save.mock.calls[0]?.[1].instructor).toBe('박 강사');
    expect(instructor()).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  // 리뷰: 저장 응답을 기다리는 사이 더 고친 입력은 요청에 들지 않았는데, 성공하면 폼을 닫아 그 입력이 사라졌다.
  // 더 고친 칸이 있으면 폼을 열어 둔다.
  it('keeps the form open with details typed while the save was in flight', async () => {
    await setLocale('ko', { reload: false });
    mockCourse();
    let finish: () => void = () => undefined;
    const save = vi.spyOn(api, 'setCourseDetails').mockImplementation(
      () =>
        new Promise<undefined>((resolve) => {
          finish = () => {
            resolve(undefined);
          };
        }),
    );
    render(CoursePanel, { props: { itemId: 'i1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    const field = instructor();
    if (field === null) throw new Error('course-instructor');
    await fireEvent.input(field, { target: { value: '박 강사' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(1);
    });
    // 응답을 기다리는 사이 강사를 더 고친다.
    await fireEvent.input(field, { target: { value: '최 강사' } });
    finish();

    expect(await screen.findByText('강좌 정보를 저장했습니다.')).toBeInTheDocument();
    expect(save.mock.calls[0]?.[1].instructor).toBe('박 강사');
    expect(instructor()?.value).toBe('최 강사');
  });
});
