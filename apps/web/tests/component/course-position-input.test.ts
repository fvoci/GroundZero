import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CourseView, EpisodeView } from '$lib/api/client';
import { api } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

function course(position: number | null): CourseView {
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
      note: '',
    },
    version: 1,
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
    access: 'write' as const,
  };
}

// 시청 위치 입력: 빈칸은 지우기, 읽을 수 없는 입력은 저장하지 않고 칸 옆에 알리며 저장된 값(420초)을 그대로 둔다.
// 전에는 'abc'가 0초로, '12:xx'가 지우기로 저장됐다.
describe('episode watch position input', () => {
  beforeEach(async () => {
    me.userId = 'user-a';
    localStorage.clear();
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
    vi.spyOn(api, 'course').mockResolvedValue(course(420));
  });
  afterEach(() => {
    me.userId = '';
    vi.restoreAllMocks();
  });

  async function openForm(): Promise<HTMLInputElement> {
    render(CoursePanel, { props: { itemId: 'item-1' } });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.courses_notes_aria({ number: 1 }) }),
    );
    const field = await screen.findByLabelText(m.courses_last_position());
    if (!(field instanceof HTMLInputElement)) throw new Error('position input');
    expect(field.value).toBe('0:07:00');
    return field;
  }
  const save = async (field: HTMLInputElement, value: string): Promise<void> => {
    await fireEvent.input(field, { target: { value } });
    const form = field.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
  };

  it('refuses text that is not a duration and keeps the stored position', async () => {
    const set = vi.spyOn(api, 'setEpisodeProgress').mockResolvedValue(undefined);
    const field = await openForm();
    for (const bad of ['abc', '12:xx', '1:2:3:4', '6001']) {
      await save(field, bad);
      await waitFor(() => {
        expect(screen.getByRole('alert').textContent).toContain(m.courses_position_invalid());
      });
      expect(field.getAttribute('aria-invalid')).toBe('true');
      expect(set).not.toHaveBeenCalled();
    }
  });

  it('sends null for an empty field, 0 for zero, and seconds for valid durations', async () => {
    const set = vi.spyOn(api, 'setEpisodeProgress').mockResolvedValue(undefined);
    const field = await openForm();
    const sent: (number | null)[] = [];
    for (const [value, expected] of [
      ['', null],
      ['0', 0],
      ['0:00', 0],
      ['12:30', 750],
      ['45', 2700],
      [' 1:02:03 ', 3723],
    ] as const) {
      await save(field, value);
      await waitFor(() => {
        expect(set).toHaveBeenCalledTimes(sent.length + 1);
      });
      sent.push(expected);
      expect(set.mock.calls.at(-1)?.[1]).toMatchObject({ last_position_seconds: expected });
      expect(screen.queryByRole('alert')).toBeNull();
    }
  });
});
