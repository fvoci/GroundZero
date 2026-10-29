import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { me } from '$lib/me.svelte';
import { router } from '$lib/router.svelte';
import { formDraftTabKey } from '$lib/features/common/form-draft';
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

it.each(['unchanged', 'newer'])(
  'keeps the restored source after a failed copy completes (%s)',
  async (generation) => {
    const key = 'fvoci:snapshot-user:-:form-draft:snapshot-test:tab:other-tab';
    localStorage.setItem(
      key,
      JSON.stringify({
        savedAt: '2026-10-29T02:00:00Z',
        input: { long: 'Selected source writing', short: '' },
      }),
    );
    render(FormDraftSnapshot);
    const writer = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('copy failed', 'QuotaExceededError');
    });
    await fireEvent.click(await screen.findByRole('button', { name: 'Restore' }));
    expect(screen.getByLabelText('Long')).toHaveValue('Selected source writing');
    writer.mockRestore();
    const newer = JSON.stringify({
      savedAt: '2026-10-29T02:00:01Z',
      input: { long: 'New writing in the original tab', short: '' },
    });
    if (generation === 'newer') localStorage.setItem(key, newer);
    const expected = localStorage.getItem(key);
    await fireEvent.click(screen.getByRole('button', { name: 'Capture' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Complete' }));
    expect(screen.getByLabelText('Completed')).toHaveTextContent('true');
    expect(localStorage.getItem(key)).toBe(expected);
  },
);

it('keeps the offered source after copying it successfully to this tab', async () => {
  const key = 'fvoci:snapshot-user:-:form-draft:snapshot-test:tab:other-tab';
  const raw = JSON.stringify({
    savedAt: '2026-10-29T02:00:00Z',
    input: { long: 'Original writing', short: '' },
  });
  localStorage.setItem(key, raw);
  render(FormDraftSnapshot);
  await fireEvent.click(await screen.findByRole('button', { name: 'Restore' }));
  expect(screen.getByLabelText('Long')).toHaveValue('Original writing');
  expect(localStorage.getItem(key)).toBe(raw);
  await fireEvent.click(screen.getByRole('button', { name: 'Capture' }));
  await fireEvent.click(screen.getByRole('button', { name: 'Complete' }));
  expect(screen.getByLabelText('Completed')).toHaveTextContent('true');
  expect(localStorage.getItem(key)).toBe(raw);
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
  const ownKey = formDraftTabKey('snapshot-test');
  if (ownKey === undefined) throw new Error('missing current draft key');
  expect(localStorage.getItem(ownKey)).toBeNull();
  expect(stored()).toContain('long writing'); // B retains the held source for another proposal.
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
