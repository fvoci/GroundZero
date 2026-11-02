import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError, type CourseView } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { unloadBlocked } from './fixtures/leave-warning';

// #289: 새 회차·CSV 가져오기는 강좌 write일 때만 보인다. 다시 읽은 강좌가 read로 바뀌면 숨은 입력이 떠날 때 경고를
// 남기면 안 된다. 새 회차는 짧은 칸이라 빈 칸으로 돌리고, CSV는 긴 입력이라 남긴 초안을 지우지 않는다(D-27). 읽기
// 전용에도 보이는 내 메모는 그대로 보호한다(D-113 SR1-11).

function course(access: CourseView['access']): CourseView {
  return {
    access,
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
    episodes: [
      {
        id: 'e1',
        number: 1,
        title: '1강',
        chapter: '',
        duration_seconds: 60,
        url: '',
        materials: '',
        planned_on: null,
        version: 1,
        mine: {
          status: 'not_started',
          planned_on: null,
          completed_on: null,
          watch_count: 0,
          last_position_seconds: null,
          note: '',
        },
      },
    ],
  };
}

const CSV = '제목,챕터,길이,계획일\n벡터,1장,12:30,\n';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'learner';
  localStorage.clear();
  vi.spyOn(api, 'course').mockResolvedValue(course('write'));
  vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
  vi.spyOn(api, 'setEpisodeProgress').mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  me.userId = '';
  router.guard = undefined;
  vi.restoreAllMocks();
  localStorage.clear();
});

/** 개인 진도 바꾸기로 다시 읽히는 강좌가 `access`가 된다. */
async function reloadAs(access: CourseView['access']): Promise<void> {
  vi.mocked(api.course).mockResolvedValue(course(access));
  await fireEvent.click(screen.getByRole('button', { name: m.courses_mark_done_aria({ number: 1 }) }));
  await waitFor(() => {
    expect(document.getElementById('episode-title') === null).toBe(access !== 'write');
  });
}

it('drops hidden new episode and CSV input after write access ends while guarding my note', async () => {
  const add = vi.spyOn(api, 'addEpisodes');
  render(CoursePanel, { props: { itemId: 'item-hidden' } });
  await fireEvent.input(await screen.findByLabelText(m.courses_new_title()), {
    target: { value: '숨을 회차' },
  });
  await fireEvent.input(screen.getByLabelText(m.courses_csv_help()), { target: { value: CSV } });
  await fireEvent.click(screen.getByRole('button', { name: m.courses_notes_aria({ number: 1 }) }));
  await fireEvent.input(screen.getByLabelText(m.courses_my_note()), {
    target: { value: '보이는 내 메모' },
  });
  await reloadAs('read');
  expect(screen.queryByLabelText(m.courses_csv_help())).toBeNull();
  // 내 메모는 읽기 전용에도 보이므로 경고가 남는다.
  expect(screen.getByLabelText(m.courses_my_note())).toHaveValue('보이는 내 메모');
  expect(unloadBlocked()).toBe(true);
  await fireEvent.input(screen.getByLabelText(m.courses_my_note()), { target: { value: '' } });
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  await reloadAs('write');
  expect(screen.getByLabelText(m.courses_new_title())).toHaveValue('');
  expect(screen.getByLabelText(m.courses_csv_help())).toHaveValue('');
  // CSV는 긴 입력이라 남긴 초안을 되살리기로 제안한다.
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.courses_csv_help())).toHaveValue(CSV);
  expect(add).not.toHaveBeenCalled();
});

it('keeps a sent episode locked until its response, then drops the hidden input', async () => {
  let reject: (e: Error) => void = () => undefined;
  vi.spyOn(api, 'addEpisodes').mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  render(CoursePanel, { props: { itemId: 'item-hidden' } });
  await fireEvent.input(await screen.findByLabelText(m.courses_new_title()), {
    target: { value: '보내는 회차' },
  });
  await fireEvent.input(screen.getByLabelText(m.courses_duration()), {
    target: { value: '10:00' },
  });
  await fireEvent.click(screen.getByRole('button', { name: m.courses_add_episode() }));
  await reloadAs('read');
  expect(router.guard?.(vi.fn(), '/')).toBe(false);
  reject(new NetworkError('offline'));
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  await reloadAs('write');
  expect(screen.getByLabelText(m.courses_new_title())).toHaveValue('');
});
