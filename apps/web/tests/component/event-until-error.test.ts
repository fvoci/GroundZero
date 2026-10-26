import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import { api, ApiError } from '$lib/api/client';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { recurring } from '../../stories/time/fixtures';

afterEach(() => vi.restoreAllMocks());
it.each(['ko', 'en'] as const)(
  'places the real until rejection next to the end field (%s)',
  async (locale) => {
    await setLocale(locale, { reload: false });
    vi.spyOn(api, 'event').mockResolvedValue(recurring);
    vi.spyOn(api, 'updateEvent').mockRejectedValue(
      new ApiError(400, 'bad_request', 'until must not be before the first occurrence'),
    );
    render(EventDialog, {
      eventId: recurring.id,
      onclose: () => undefined,
      onchanged: () => undefined,
    });
    const title = await screen.findByLabelText(m.events_title());
    await fireEvent.input(title, { target: { value: '내 입력' } });
    const start = screen.getByLabelText(m.events_start_date());
    await fireEvent.input(start, { target: { value: '2026-10-28' } });
    await fireEvent.input(screen.getByLabelText(m.events_end_date()), {
      target: { value: '2026-10-28' },
    });
    await fireEvent.submit(title.closest('form') ?? title);
    const error = await screen.findByText(m.events_until_before_start());
    const until = screen.getByLabelText(m.events_end_until_aria());
    expect(until).toHaveAttribute('aria-describedby', error.id);
    expect(until).toHaveAttribute('aria-invalid', 'true');
    expect(title).toHaveValue('내 입력');
    expect(start).toHaveValue('2026-10-28');
  },
);
import './fixtures/event-user';
