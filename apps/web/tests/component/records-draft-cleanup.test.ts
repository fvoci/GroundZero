import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { tick } from 'svelte';

import { api } from '$lib/api/client';
import { formDraftTabKey } from '$lib/features/common/form-draft';
import { emptyForm } from '$lib/features/records/records';
import { clearLegacyRecordDrafts, normalizeRecordDraft } from '$lib/features/records/legacy-drafts';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
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
  const form = {
    ...emptyForm(),
    body: 'Keep current long writing',
    values: { mitigation: '' },
  };
  return {
    savedAt: '2026-10-28T12:00:00Z',
    input: {
      form,
      baseline: JSON.stringify({ ...emptyForm(), values: { mitigation: '' } }),
      version: 0,
      pending,
    },
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

it('retains raw retry sources across accounts and organizations for explicit recovery or discard', async () => {
  const keys = [
    recordKey,
    'fvoci:user-a:org-a:form-draft:record:other-item:research_log:new:held:old-offer',
    'fvoci:user-b:-:form-draft:record:hidden-item:reference:existing-record:tab:other-tab',
    'fvoci:user-b:org-b:form-draft:record:hidden-item:reference:existing-record:held:other-offer',
  ];
  for (const key of keys)
    localStorage.setItem(key, saved({ requestId, signature: legacySignature }));

  await startWithoutOpeningRecords();

  for (const key of keys) {
    expect(localStorage.getItem(key)).toBe(saved({ requestId, signature: legacySignature }));
  }
  expect(document.body.textContent).not.toContain('LEGACY_REQUEST_BODY');
});

it.each(['f'.repeat(63), 'f'.repeat(65), 'not-a-digest'])(
  'drops unsupported retry metadata (%s) on recovery reads without rewriting its source',
  async (signature) => {
    localStorage.setItem(recordKey, saved({ requestId, signature }));

    await startWithoutOpeningRecords();

    expect(localStorage.getItem(recordKey)).toBe(saved({ requestId, signature }));
    expect(normalizeRecordDraft('risk', draft({ requestId, signature }).input)?.pending).toBeNull();
  },
);

it.each(
  [null, { requestId, signature: 'a'.repeat(64) }].flatMap((pending) =>
    ['legacy key', 'page key'].map((format) => ({ pending, format })),
  ),
)(
  'keeps current long-writing drafts byte-for-byte without a cleanup notice ($format, pending: $pending)',
  async ({ pending, format }) => {
    const value = saved(pending);
    const key =
      format === 'legacy key'
        ? recordKey
        : formDraftTabKey('record:never-opened-item:risk:new', 'user-a', '-');
    if (key === undefined) throw new Error('missing record key');
    localStorage.setItem(key, value);

    await startWithoutOpeningRecords();
    me.userId = 'user-a';
    await tick();

    expect(localStorage.getItem(key)).toBe(value);
    expect(screen.queryByTestId('record-draft-unrecoverable')).not.toBeInTheDocument();
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
])('retains an unverified record key without exposing it at startup: %s', async (_name, value) => {
  localStorage.setItem(recordKey, value);

  await startWithoutOpeningRecords();

  expect(localStorage.getItem(recordKey)).toBe(value);
  expect(document.body.textContent).not.toContain('LEGACY_REQUEST_BODY');
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

  expect(localStorage.getItem(recordKey)).toBe(value);
  for (const key of unrelated) expect(localStorage.getItem(key)).toBe(value);
});

it('offers explicit discard for unnormalizable own drafts without showing raw content or other accounts', async () => {
  const raw = '{"request":"LEGACY_REQUEST_BODY","title":"SHORT_PRIVATE"';
  const other = recordKey.replace(':user-a:', ':user-b:');
  localStorage.setItem(recordKey, raw);
  localStorage.setItem(other, raw);
  await startWithoutOpeningRecords();
  expect(screen.queryByTestId('record-draft-unrecoverable')).not.toBeInTheDocument();
  me.userId = 'user-a';
  expect(await screen.findByTestId('record-draft-unrecoverable')).toBeInTheDocument();
  expect(document.body.textContent).not.toContain('LEGACY_REQUEST_BODY');
  expect(document.body.textContent).not.toContain('SHORT_PRIVATE');
  expect(localStorage.getItem(recordKey)).toBe(raw);
  await fireEvent.click(screen.getByRole('button', { name: m.common_draft_discard() }));
  expect(localStorage.getItem(recordKey)).toBeNull();
  expect(localStorage.getItem(other)).toBe(raw);
  expect(screen.queryByTestId('record-draft-unrecoverable')).not.toBeInTheDocument();
});

it('requires another discard choice if an unnormalizable source changes, and never removes new writing', async () => {
  localStorage.setItem(recordKey, '{old broken request');
  await startWithoutOpeningRecords();
  me.userId = 'user-a';
  const discard = await screen.findByRole('button', { name: m.common_draft_discard() });
  localStorage.setItem(recordKey, saved(null));
  await fireEvent.click(discard);
  expect(localStorage.getItem(recordKey)).toBe(saved(null));
  expect(screen.queryByTestId('record-draft-unrecoverable')).not.toBeInTheDocument();
});

it('preserves newer writing in the normalization comparison gap even after its owner has closed', () => {
  const old = draft(null);
  old.input.form.title = 'SHORT_OLD_TITLE';
  const raw = JSON.stringify(old);
  const replacement = draft(null);
  replacement.input.form.body = 'New writing from a now-closed owner';
  const newer = JSON.stringify(replacement);
  localStorage.setItem(recordKey, raw);
  const read = localStorage.getItem.bind(localStorage);
  const readSession = sessionStorage.getItem.bind(sessionStorage);
  let reads = 0;
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (this: Storage, key: string) {
    const value = this === localStorage ? read(key) : readSession(key);
    if (this === localStorage && key === recordKey && ++reads === 2) {
      localStorage.setItem(recordKey, newer);
    }
    return value;
  });

  clearLegacyRecordDrafts();

  expect(Object.values(localStorage).join('')).toContain('New writing from a now-closed owner');
});

it('retains unnormalizable drafts after discard storage failure and offers them again', async () => {
  const raw = '{broken request';
  localStorage.setItem(recordKey, raw);
  await startWithoutOpeningRecords();
  me.userId = 'user-a';
  const discard = await screen.findByRole('button', { name: m.common_draft_discard() });
  const remove = vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => {
    throw new DOMException('storage unavailable', 'SecurityError');
  });
  await fireEvent.click(discard);
  expect(localStorage.getItem(recordKey)).toBe(raw);
  expect(screen.getByTestId('record-draft-unrecoverable')).toBeInTheDocument();
  remove.mockRestore();
  await fireEvent.click(discard);
  expect(localStorage.getItem(recordKey)).toBeNull();
});

it('drops additional retry properties on recovery reads without changing the original', async () => {
  localStorage.setItem(
    recordKey,
    saved({ requestId, signature: 'a'.repeat(64), request: legacySignature }),
  );

  await startWithoutOpeningRecords();

  expect(localStorage.getItem(recordKey)).toBe(
    saved({ requestId, signature: 'a'.repeat(64), request: legacySignature }),
  );
  expect(
    normalizeRecordDraft(
      'risk',
      draft({ requestId, signature: 'a'.repeat(64), request: legacySignature }).input,
    ),
  ).toEqual(draft(null).input);
});

it.each([false, true])(
  'projects old full forms to writing only on reads without changing either source (offline: %s)',
  async (offline) => {
    const fetch = vi.spyOn(globalThis, 'fetch');
    if (offline) fetch.mockRejectedValue(new TypeError('offline'));
    const oldForm = {
      ...emptyForm(),
      title: 'SHORT_OLD_TITLE',
      occurred_on: '2026-10-28',
      body: 'Unsent body',
      values: {
        mitigation: 'Unsent mitigation',
        amount: '123456',
        reference: 'SHORT_REFERENCE',
        owner: 'SHORT_PERSON',
        unknown_long_looking: 'x'.repeat(1000),
      },
      people: { attendees: ['SHORT_ATTENDEE'] },
      files: [{ file_id: requestId, version: '2', role: 'SHORT_FILE_ROLE' }],
      unreadable: { amount: true },
    };
    const oldBase = {
      ...oldForm,
      title: 'SHORT_BASE_TITLE',
      body: 'Body before editing',
      values: { ...oldForm.values, mitigation: 'Mitigation before editing' },
    };
    const old = {
      ...draft(null),
      input: { form: oldForm, baseline: JSON.stringify(oldBase), version: 7, pending: null },
    };
    const keys = [
      recordKey,
      'fvoci:user-b:org-b:form-draft:record:unopened-item:risk:old-record:held:old-offer',
    ];
    for (const key of keys) localStorage.setItem(key, JSON.stringify(old));

    await startWithoutOpeningRecords();

    for (const key of keys) {
      const value = localStorage.getItem(key);
      if (value === null) throw new Error('unsent writing was discarded');
      expect(value).toBe(JSON.stringify(old));
      const normalized = { ...old, input: normalizeRecordDraft('risk', old.input) };
      expect(normalized).toEqual({
        ...old,
        input: {
          form: {
            ...emptyForm(),
            body: 'Unsent body',
            values: { mitigation: 'Unsent mitigation' },
          },
          baseline: JSON.stringify({
            ...emptyForm(),
            body: 'Body before editing',
            values: { mitigation: 'Mitigation before editing' },
          }),
          version: 7,
          pending: null,
        },
      });
    }
    expect(fetch).not.toHaveBeenCalled();
  },
);

it('retains but rejects a draft whose record kind is absent from the offline field definitions', async () => {
  const unknown = recordKey.replace(':risk:', ':unknown_kind:');
  localStorage.setItem(unknown, saved(null));

  await startWithoutOpeningRecords();

  expect(localStorage.getItem(unknown)).toBe(saved(null));
  expect(normalizeRecordDraft('unknown_kind', draft(null).input)).toBeUndefined();
});

it('recovers only body for a known kind with no long_text fields while preserving its source', async () => {
  const key = recordKey.replace(':risk:', ':research_log:');
  const old = {
    ...draft(null),
    input: {
      ...draft(null).input,
      form: {
        ...emptyForm(),
        title: 'SHORT_LOG_TITLE',
        body: 'Unsent research log',
        values: { log_type: 'feedback' },
        people: { participants: ['SHORT_PARTICIPANT'] },
      },
      baseline: JSON.stringify(emptyForm()),
    },
  };
  localStorage.setItem(key, JSON.stringify(old));

  await startWithoutOpeningRecords();

  const value = localStorage.getItem(key);
  if (value === null) throw new Error('known body-only kind was discarded');
  expect(value).toBe(JSON.stringify(old));
  const normalized = { ...old, input: normalizeRecordDraft('research_log', old.input) };
  expect(normalized).toEqual({
    ...old,
    input: { ...old.input, form: { ...emptyForm(), body: 'Unsent research log' } },
  });
});

it('keeps sources available for explicit recovery when storage refuses writes', async () => {
  const old = draft(null);
  old.input.form.title = 'SHORT_UNWRITABLE_TITLE';
  const raw = JSON.stringify(old);
  localStorage.setItem(recordKey, raw);
  const writer = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new DOMException('storage unavailable', 'QuotaExceededError');
  });

  await startWithoutOpeningRecords();

  expect(clearLegacyRecordDrafts()).toBe('ready');
  expect(localStorage.getItem(recordKey)).toBe(raw);
  expect(writer).not.toHaveBeenCalled();
  writer.mockRestore();
  expect(clearLegacyRecordDrafts()).toBe('ready');
  expect(localStorage.getItem(recordKey)).toContain(old.input.form.body);
  expect(localStorage.getItem(recordKey)).toBe(raw);
});

it('offers late full-form sources for explicit discard without rewriting either key', async () => {
  await startWithoutOpeningRecords();
  const old = draft(null);
  old.input.form.title = 'SHORT_LATE_TITLE';
  const values = { mitigation: 'Unsent late mitigation', amount: '123' };
  old.input.form.values = values;
  old.input.baseline = JSON.stringify({
    ...old.input.form,
    title: 'SHORT_LATE_BASE_TITLE',
    body: 'Before late editing',
  });
  const value = JSON.stringify(old);
  const untouchedKey = recordKey.replace('never-opened-item', 'another-unopened-item');
  localStorage.setItem(untouchedKey, value);
  localStorage.setItem(recordKey, value);

  window.dispatchEvent(
    new StorageEvent('storage', { key: recordKey, storageArea: localStorage, newValue: value }),
  );

  me.userId = 'user-a';
  await waitFor(() => {
    expect(screen.getAllByTestId('record-draft-unrecoverable')).toHaveLength(2);
  });
  expect(localStorage.getItem(recordKey)).toBe(value);
  expect(localStorage.getItem(untouchedKey)).toBe(value);
  expect(document.body.textContent).not.toContain('SHORT_LATE_TITLE');
});

it('offers a late raw retry for explicit discard while recovery reads omit its request', async () => {
  await startWithoutOpeningRecords();
  const value = saved({ requestId, signature: legacySignature });
  localStorage.setItem(recordKey, value);

  window.dispatchEvent(
    new StorageEvent('storage', { key: recordKey, storageArea: localStorage, newValue: value }),
  );

  me.userId = 'user-a';
  expect(await screen.findByTestId('record-draft-unrecoverable')).toBeInTheDocument();
  expect(localStorage.getItem(recordKey)).toBe(value);
  expect(
    normalizeRecordDraft('risk', draft({ requestId, signature: legacySignature }).input),
  ).toEqual(draft(null).input);
});

it.each(['rewrite', 'remove'])(
  'keeps newer writing when a stale legacy storage event arrives (%s)',
  async (operation) => {
    await startWithoutOpeningRecords();
    const old = draft(operation === 'remove' ? { requestId, signature: legacySignature } : null);
    old.input.form.title = 'SHORT_OLD_EVENT';
    const value = JSON.stringify(old);
    const newerDraft = draft(null);
    newerDraft.input.form.body = 'Newer unsent writing';
    const newer = JSON.stringify(newerDraft);
    localStorage.setItem(recordKey, newer);
    const writes = vi.spyOn(Storage.prototype, 'setItem');
    const removes = vi.spyOn(Storage.prototype, 'removeItem');

    window.dispatchEvent(
      new StorageEvent('storage', { key: recordKey, storageArea: localStorage, newValue: value }),
    );

    expect(localStorage.getItem(recordKey)).toBe(newer);
    expect(writes).not.toHaveBeenCalled();
    expect(removes).not.toHaveBeenCalled();
  },
);

it.each(['comparison', 'quota failure'])(
  'inspects concurrent replacements without mutating their keys during %s',
  (stage) => {
    const old = draft(null);
    old.input.form.title = 'SHORT_OLD_SNAPSHOT';
    const raw = JSON.stringify(old);
    const replacement = draft(null);
    replacement.input.form.title = 'SHORT_REPLACEMENT';
    replacement.input.form.body = 'Newest unsent writing';
    const newer = JSON.stringify(replacement);
    localStorage.setItem(recordKey, raw);
    const write = localStorage.setItem.bind(localStorage);
    const read = localStorage.getItem.bind(localStorage);
    const readSession = sessionStorage.getItem.bind(sessionStorage);
    let reads = 0;
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (
      this: Storage,
      key: string,
    ) {
      const value = this === localStorage ? read(key) : readSession(key);
      if (this === localStorage && key === recordKey && ++reads === 2) write(key, newer);
      return value;
    });
    const writes = vi.spyOn(Storage.prototype, 'setItem');
    if (stage === 'quota failure')
      writes.mockImplementation(() => {
        throw new DOMException('storage unavailable', 'QuotaExceededError');
      });

    expect(clearLegacyRecordDrafts()).toBe('ready');

    const stored = localStorage.getItem(recordKey);
    expect(stored).toContain('Newest unsent writing');
    expect(stored).toBe(newer);
    expect(stored).not.toContain('SHORT_OLD_SNAPSHOT');
    expect(writes).not.toHaveBeenCalled();
  },
);

it('rechecks a writing-only snapshot replaced by a raw retry before declaring cleanup ready', () => {
  const raw = saved(null);
  const newer = saved({ requestId, signature: legacySignature });
  localStorage.setItem(recordKey, raw);
  const read = localStorage.getItem.bind(localStorage);
  const readSession = sessionStorage.getItem.bind(sessionStorage);
  let reads = 0;
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (this: Storage, key: string) {
    const value = this === localStorage ? read(key) : readSession(key);
    if (this === localStorage && key === recordKey && ++reads === 2)
      localStorage.setItem(key, newer);
    return value;
  });

  expect(clearLegacyRecordDrafts()).toBe('ready');

  expect(localStorage.getItem(recordKey)).toBe(newer);
  expect(
    normalizeRecordDraft('risk', draft({ requestId, signature: legacySignature }).input),
  ).toEqual(draft(null).input);
});

it('returns retry after three changing snapshots and inspects the latest source on the next attempt', () => {
  const versions = Array.from({ length: 11 }, (_, index) => {
    const value = draft(null);
    value.input.form.title = `SHORT_RACING_${String(index)}`;
    value.input.form.body = `Unsent racing writing ${String(index)}`;
    return JSON.stringify(value);
  });
  const first = versions[0];
  if (first === undefined) throw new Error('missing initial racing draft');
  localStorage.setItem(recordKey, first);
  let replacements = 0;
  const read = localStorage.getItem.bind(localStorage);
  const readSession = sessionStorage.getItem.bind(sessionStorage);
  const reader = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (
    this: Storage,
    key: string,
  ) {
    const value = this === localStorage ? read(key) : readSession(key);
    const next = versions[replacements + 1];
    if (
      this === localStorage &&
      key === recordKey &&
      value === versions[replacements] &&
      next !== undefined
    ) {
      replacements += 1;
      localStorage.setItem(recordKey, next);
    }
    return value;
  });

  expect(clearLegacyRecordDrafts()).toBe('retry');

  // Both snapshot and comparison reads can observe a competing writer, bounded to three attempts.
  expect(replacements).toBe(6);
  reader.mockRestore();
  expect(localStorage.getItem(recordKey)).toBe(versions[6]);
  expect(clearLegacyRecordDrafts()).toBe('ready');
  expect(localStorage.getItem(recordKey)).toBe(versions[6]);
});

it('ignores unrelated storage events and stops record cleanup when App unmounts', async () => {
  await startWithoutOpeningRecords();
  const value = saved({ requestId, signature: legacySignature });
  const unrelatedKey = 'fvoci:user-a:-:form-draft:comment:d1:new:tab:old-tab';
  localStorage.setItem(recordKey, value);
  localStorage.setItem(unrelatedKey, value);

  for (const event of [
    new StorageEvent('storage', { key: unrelatedKey, storageArea: localStorage, newValue: value }),
    new StorageEvent('storage', { key: recordKey, storageArea: sessionStorage, newValue: value }),
    new StorageEvent('storage', { key: null, storageArea: localStorage }),
  ])
    window.dispatchEvent(event);

  expect(localStorage.getItem(recordKey)).toBe(value);
  expect(localStorage.getItem(unrelatedKey)).toBe(value);
  cleanup();
  window.dispatchEvent(
    new StorageEvent('storage', { key: recordKey, storageArea: localStorage, newValue: value }),
  );
  expect(localStorage.getItem(recordKey)).toBe(value);
});

it('keeps app startup working when browser storage cannot be enumerated', async () => {
  vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
    throw new DOMException('storage unavailable', 'SecurityError');
  });
  localStorage.setItem(recordKey, saved({ requestId, signature: legacySignature }));

  await startWithoutOpeningRecords();
});

it.each(['focus', 'visibilitychange'])(
  'retries failed startup record cleanup when the window returns through %s',
  async (event) => {
    const old = draft(null);
    old.input.form.title = 'SHORT_UNOPENED_TITLE';
    const value = JSON.stringify(old);
    localStorage.setItem(recordKey, value);
    const enumeration = vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
      throw new DOMException('enumeration unavailable', 'SecurityError');
    });
    await startWithoutOpeningRecords();
    me.userId = 'user-a';
    expect(localStorage.getItem(recordKey)).toBe(value);
    enumeration.mockRestore();

    if (event === 'focus') window.dispatchEvent(new Event(event));
    else {
      vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
      document.dispatchEvent(new Event(event));
    }

    expect(await screen.findByTestId('record-draft-unrecoverable')).toBeInTheDocument();
    expect(localStorage.getItem(recordKey)).toBe(value);
    expect(localStorage.getItem(recordKey)).toContain('Keep current long writing');
  },
);

it('does not retry record cleanup while hidden or after the app unmounts', async () => {
  await startWithoutOpeningRecords();
  const value = saved({ requestId, signature: legacySignature });
  localStorage.setItem(recordKey, value);
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
  document.dispatchEvent(new Event('visibilitychange'));
  expect(localStorage.getItem(recordKey)).toBe(value);
  cleanup();
  window.dispatchEvent(new Event('focus'));
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
  document.dispatchEvent(new Event('visibilitychange'));
  expect(localStorage.getItem(recordKey)).toBe(value);
});
