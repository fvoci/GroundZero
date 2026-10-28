import { cleanup, render, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { emptyForm } from '$lib/features/records/records';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

import App from '../../src/app/App.svelte';

const legacySignature = JSON.stringify({
  item: 'never-opened-item',
  kind: 'risk',
  input: { title: 'LEGACY_REQUEST_TITLE', fields: { amount: 321 }, body: 'LEGACY_REQUEST_BODY' },
});
const requestId = '11111111-1111-4111-8111-111111111111';
const recordKey = 'fvoci:user-a:-:form-draft:record:never-opened-item:risk:new:tab:old-tab';

function draft(pending: unknown) {
  const form = { ...emptyForm(), body: 'Keep current long writing' };
  return {
    savedAt: '2026-10-28T12:00:00Z',
    input: { form, baseline: JSON.stringify(emptyForm()), version: 0, pending },
  };
}

function saved(pending: unknown): string {
  return JSON.stringify(draft(pending));
}

async function startWithoutOpeningRecords(): Promise<void> {
  render(App);
  await waitFor(() => {
    expect(api.authInfo).toHaveBeenCalledOnce();
  });
  expect(api.recordKinds).not.toHaveBeenCalled();
  expect(api.itemRecords).not.toHaveBeenCalled();
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = '';
  router.path = '/';
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  vi.spyOn(api, 'me').mockResolvedValue(undefined);
  vi.spyOn(api, 'authInfo').mockResolvedValue({ oidc_enabled: true });
  vi.spyOn(api, 'recordKinds');
  vi.spyOn(api, 'itemRecords');
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  router.guard = undefined;
});

it('clears raw retry requests at app startup across unopened records, accounts, organizations and held drafts', async () => {
  const keys = [
    recordKey,
    'fvoci:user-a:org-a:form-draft:record:other-item:research_log:new:held:old-offer',
    'fvoci:user-b:-:form-draft:record:hidden-item:reference:existing-record:tab:other-tab',
    'fvoci:user-b:org-b:form-draft:record:hidden-item:reference:existing-record:held:other-offer',
  ];
  for (const key of keys)
    localStorage.setItem(key, saved({ requestId, signature: legacySignature }));

  await startWithoutOpeningRecords();

  for (const key of keys) expect(localStorage.getItem(key)).toBeNull();
  expect(
    Object.keys(localStorage)
      .map((key) => localStorage.getItem(key))
      .join(''),
  ).not.toContain('LEGACY_REQUEST_BODY');
});

it.each(['f'.repeat(63), 'f'.repeat(65), 'not-a-digest'])(
  'clears an unsupported stored retry signature (%s) without offering the draft',
  async (signature) => {
    localStorage.setItem(recordKey, saved({ requestId, signature }));

    await startWithoutOpeningRecords();

    expect(localStorage.getItem(recordKey)).toBeNull();
  },
);

it.each([null, { requestId, signature: 'a'.repeat(64) }])(
  'keeps current long-writing drafts byte-for-byte (pending: %j)',
  async (pending) => {
    const value = saved(pending);
    localStorage.setItem(recordKey, value);

    await startWithoutOpeningRecords();

    expect(localStorage.getItem(recordKey)).toBe(value);
  },
);

it.each([
  ['truncated JSON', saved({ requestId, signature: legacySignature }).slice(0, -4)],
  ['array envelope', JSON.stringify([draft({ requestId, signature: legacySignature })])],
  ['missing input', JSON.stringify({ savedAt: 'old', request: legacySignature })],
  ['non-object input', JSON.stringify({ savedAt: 'old', input: legacySignature })],
  ['missing pending', JSON.stringify({ savedAt: 'old', input: { request: legacySignature } })],
  [
    'broken form with null pending',
    JSON.stringify({ ...draft(null), input: { ...draft(null).input, form: legacySignature } }),
  ],
  [
    'broken baseline with null pending',
    JSON.stringify({ ...draft(null), input: { ...draft(null).input, baseline: legacySignature } }),
  ],
  [
    'broken version with null pending',
    JSON.stringify({ ...draft(null), input: { ...draft(null).input, version: legacySignature } }),
  ],
  ['non-UUID request ID', saved({ requestId: legacySignature, signature: 'a'.repeat(64) })],
  ['non-hex UUID request ID', saved({ requestId: 'g'.repeat(36), signature: 'a'.repeat(64) })],
  ['non-hex digest', saved({ requestId, signature: 'g'.repeat(64) })],
])('removes an unverified record draft at startup: %s', async (_name, value) => {
  localStorage.setItem(recordKey, value);

  await startWithoutOpeningRecords();

  expect(localStorage.getItem(recordKey)).toBeNull();
});

it('leaves other domains and non-app storage untouched while cleaning a legacy record', async () => {
  const value = saved({ requestId, signature: legacySignature });
  const unrelated = [
    'fvoci:user-a:-:form-draft:comment:d1:new:tab:old-tab',
    'fvoci:user-a:-:form-draft:course:item:import:tab:old-tab',
    'other-app:form-draft:record:item:risk:new',
  ];
  localStorage.setItem(recordKey, value);
  for (const key of unrelated) localStorage.setItem(key, value);

  await startWithoutOpeningRecords();

  expect(localStorage.getItem(recordKey)).toBeNull();
  for (const key of unrelated) expect(localStorage.getItem(key)).toBe(value);
});

it('keeps app startup working when browser storage cannot be enumerated', async () => {
  vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
    throw new DOMException('storage unavailable', 'SecurityError');
  });
  localStorage.setItem(recordKey, saved({ requestId, signature: legacySignature }));

  await startWithoutOpeningRecords();
});
