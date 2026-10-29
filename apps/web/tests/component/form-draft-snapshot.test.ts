import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { me } from '$lib/me.svelte';
import { router } from '$lib/router.svelte';
import FormDraftSnapshot from './fixtures/FormDraftSnapshot.svelte';

const stored = () => Object.values(localStorage).join('\n');
const input = async (name: string, value: string) => {
  await fireEvent.input(screen.getByLabelText(name), { target: { value } });
};

beforeEach(() => {
  localStorage.clear();
  me.userId = 'snapshot-user';
});
afterEach(() => {
  cleanup();
  me.userId = '';
  router.guard = undefined;
  vi.restoreAllMocks();
});

it('reports the exact stored value only after a successful write and stays silent on write failure', async () => {
  const onPersist =
    vi.fn<
      (
        key: string | undefined,
        value: { savedAt: string; input: { long: string; short: string } } | undefined,
      ) => void
    >();
  render(FormDraftSnapshot, { props: { filtered: true, onPersist } });
  await input('Long', 'first durable writing');
  await waitFor(() => {
    expect(onPersist).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ input: { long: 'first durable writing', short: '' } }),
    );
  });
  const [key, value] = onPersist.mock.lastCall ?? [];
  if (typeof key !== 'string') throw new Error('missing persisted key');
  expect(localStorage.getItem(key)).toBe(JSON.stringify(value));
  onPersist.mockClear();
  const writer = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new DOMException('draft quota', 'QuotaExceededError');
  });
  await input('Long', 'writing while storage is unavailable');
  expect(onPersist).not.toHaveBeenCalled();
  expect(localStorage.getItem(key)).toBe(JSON.stringify(value));
  expect(screen.getByLabelText('Long')).toHaveValue('writing while storage is unavailable');
  writer.mockRestore();
  await input('Long', 'writing after storage recovers');
  await waitFor(() => {
    expect(onPersist).toHaveBeenCalledWith(
      key,
      expect.objectContaining({ input: { long: 'writing after storage recovers', short: '' } }),
    );
  });
});

it('keeps the existing full snapshot behavior when no storedSnapshot option is supplied', async () => {
  render(FormDraftSnapshot);
  await input('Long', 'long writing');
  await input('Short', 'short setting');
  await waitFor(() => {
    expect(stored()).toContain('short setting');
  });
  expect(stored()).toContain('long writing');
  cleanup();
  render(FormDraftSnapshot);
  await fireEvent.click(await screen.findByRole('button', { name: 'Restore' }));
  expect(screen.getByLabelText('Long')).toHaveValue('long writing');
  expect(screen.getByLabelText('Short')).toHaveValue('short setting');
  await fireEvent.click(screen.getByRole('button', { name: 'Capture' }));
  await fireEvent.click(screen.getByRole('button', { name: 'Complete' }));
  expect(screen.getByLabelText('Completed')).toHaveTextContent('true');
  expect(screen.getByLabelText('Long')).toHaveValue('');
  expect(stored()).not.toContain('long writing');
});

it('stores the selected snapshot while checking completion against the whole live input', async () => {
  render(FormDraftSnapshot, { props: { filtered: true } });
  await input('Long', 'long writing');
  await input('Short', 'short setting');
  await waitFor(() => {
    expect(stored()).toContain('long writing');
  });
  expect(stored()).not.toContain('short setting');
  await fireEvent.click(screen.getByRole('button', { name: 'Capture' }));
  await input('Short', 'new setting');
  await fireEvent.click(screen.getByRole('button', { name: 'Complete' }));
  expect(screen.getByLabelText('Completed')).toHaveTextContent('false');
  expect(screen.getByLabelText('Long')).toHaveValue('long writing');
  expect(screen.getByLabelText('Short')).toHaveValue('new setting');
  expect(stored()).not.toContain('new setting');
  cleanup();
  render(FormDraftSnapshot, { props: { filtered: true } });
  await fireEvent.click(await screen.findByRole('button', { name: 'Restore' }));
  expect(screen.getByLabelText('Long')).toHaveValue('long writing');
  expect(screen.getByLabelText('Short')).toHaveValue('');
});
