import { cleanup, render, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { emptyForm } from '$lib/features/records/records';
import { clearLegacyRecordDrafts, normalizeRecordDraft } from '$lib/features/records/legacy-drafts';
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

it('retains invalid retry keys across records, accounts, organizations and held drafts without exposing them', async () => {
  const keys = [
    recordKey,
    'fvoci:user-a:org-a:form-draft:record:other-item:research_log:new:held:old-offer',
    'fvoci:user-b:-:form-draft:record:hidden-item:reference:existing-record:tab:other-tab',
    'fvoci:user-b:org-b:form-draft:record:hidden-item:reference:existing-record:held:other-offer',
  ];
  for (const key of keys)
    localStorage.setItem(key, saved({ requestId, signature: legacySignature }));

  await startWithoutOpeningRecords();

  for (const key of keys)
    expect(localStorage.getItem(key)).toBe(saved({ requestId, signature: legacySignature }));
  expect(document.body.textContent).not.toContain('LEGACY_REQUEST_BODY');
});

it.each(['f'.repeat(63), 'f'.repeat(65), 'not-a-digest'])(
  'retains an unsupported stored retry signature (%s) without admitting the draft',
  async (signature) => {
    localStorage.setItem(recordKey, saved({ requestId, signature }));

    await startWithoutOpeningRecords();

    expect(localStorage.getItem(recordKey)).toBe(saved({ requestId, signature }));
    expect(normalizeRecordDraft('risk', draft({ requestId, signature }).input)).toBeUndefined();
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

it('retains but rejects retry metadata with additional own properties even when UUID and digest are valid', async () => {
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
  ).toBeUndefined();
});

it.each([false, true])(
  'normalizes both old full forms to writing only without login or network (offline: %s)',
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
      const normalized: unknown = JSON.parse(value);
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

it('keeps unsent body for a known kind with no long_text fields while removing people and settings', async () => {
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
  const normalized: unknown = JSON.parse(value);
  expect(normalized).toEqual({
    ...old,
    input: { ...old.input, form: { ...emptyForm(), body: 'Unsent research log' } },
  });
});

it('keeps valid writing and returns retry if its sanitized replacement cannot be stored', async () => {
  const old = draft(null);
  old.input.form.title = 'SHORT_UNWRITABLE_TITLE';
  const raw = JSON.stringify(old);
  localStorage.setItem(recordKey, raw);
  const writer = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new DOMException('storage unavailable', 'QuotaExceededError');
  });

  await startWithoutOpeningRecords();

  expect(clearLegacyRecordDrafts()).toBe('retry');
  expect(localStorage.getItem(recordKey)).toBe(raw);
  writer.mockRestore();
  expect(clearLegacyRecordDrafts()).toBe('ready');
  expect(localStorage.getItem(recordKey)).toContain(old.input.form.body);
  expect(localStorage.getItem(recordKey)).not.toContain('SHORT_UNWRITABLE_TITLE');
});

it('normalizes an old full-form record draft written by another tab after app startup', async () => {
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

  await waitFor(() => {
    expect(JSON.parse(localStorage.getItem(recordKey) ?? 'null')).toEqual({
      ...old,
      input: {
        ...old.input,
        form: {
          ...emptyForm(),
          body: old.input.form.body,
          values: { mitigation: 'Unsent late mitigation' },
        },
        baseline: JSON.stringify({
          ...emptyForm(),
          body: 'Before late editing',
          values: { mitigation: 'Unsent late mitigation' },
        }),
      },
    });
  });
  expect(localStorage.getItem(untouchedKey)).toBe(value);
});

it('retains but rejects a legacy raw retry written by another tab after app startup', async () => {
  await startWithoutOpeningRecords();
  const value = saved({ requestId, signature: legacySignature });
  localStorage.setItem(recordKey, value);

  window.dispatchEvent(
    new StorageEvent('storage', { key: recordKey, storageArea: localStorage, newValue: value }),
  );

  await waitFor(() => {
    expect(localStorage.getItem(recordKey)).toBe(value);
  });
  expect(
    normalizeRecordDraft('risk', draft({ requestId, signature: legacySignature }).input),
  ).toBeUndefined();
});

it.each(['rewrite', 'remove'])(
  'keeps newer writing that arrives while an older storage event is normalized (%s)',
  async (operation) => {
    await startWithoutOpeningRecords();
    const old = draft(operation === 'remove' ? { requestId, signature: legacySignature } : null);
    old.input.form.title = 'SHORT_OLD_EVENT';
    const value = JSON.stringify(old);
    const newerDraft = draft(null);
    newerDraft.input.form.body = 'Newer unsent writing';
    const newer = JSON.stringify(newerDraft);
    localStorage.setItem(recordKey, value);
    const parse = JSON.parse;
    vi.spyOn(JSON, 'parse').mockImplementation((raw: string) => {
      const parsed: unknown = parse(raw);
      if (raw === value) localStorage.setItem(recordKey, newer);
      return parsed;
    });

    window.dispatchEvent(
      new StorageEvent('storage', { key: recordKey, storageArea: localStorage, newValue: value }),
    );

    expect(localStorage.getItem(recordKey)).toBe(newer);
  },
);

it.each(['comparison', 'failed rewrite'])(
  'rechecks the replacement snapshot during a full cleanup after %s',
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
    if (stage === 'comparison') {
      const parse = JSON.parse;
      vi.spyOn(JSON, 'parse').mockImplementation((value: string) => {
        const parsed: unknown = parse(value);
        if (value === raw) write(recordKey, newer);
        return parsed;
      });
    } else {
      const writeSession = sessionStorage.setItem.bind(sessionStorage);
      vi.spyOn(Storage.prototype, 'setItem').mockImplementation(function (
        this: Storage,
        key: string,
        value: string,
      ) {
        if (this === localStorage && key === recordKey && localStorage.getItem(key) === raw) {
          write(key, newer);
          throw new DOMException('rewrite unavailable', 'SecurityError');
        }
        if (this === localStorage) write(key, value);
        else writeSession(key, value);
      });
    }

    expect(clearLegacyRecordDrafts()).toBe('ready');

    const stored = localStorage.getItem(recordKey);
    expect(stored).toContain('Newest unsent writing');
    expect(stored).not.toContain('SHORT_REPLACEMENT');
    expect(stored).not.toContain('SHORT_OLD_SNAPSHOT');
  },
);

it('rechecks a writing-only snapshot replaced by a raw retry before declaring cleanup ready', () => {
  const raw = saved(null);
  const newer = saved({ requestId, signature: legacySignature });
  localStorage.setItem(recordKey, raw);
  const parse = JSON.parse;
  vi.spyOn(JSON, 'parse').mockImplementation((value: string) => {
    const parsed: unknown = parse(value);
    if (value === raw) localStorage.setItem(recordKey, newer);
    return parsed;
  });

  expect(clearLegacyRecordDrafts()).toBe('ready');

  expect(localStorage.getItem(recordKey)).toBe(newer);
  expect(
    normalizeRecordDraft('risk', draft({ requestId, signature: legacySignature }).input),
  ).toBeUndefined();
});

it('returns retry after three changing snapshots and repairs the latest writing on the next cleanup', () => {
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
  const parse = JSON.parse;
  const parser = vi.spyOn(JSON, 'parse').mockImplementation((value: string) => {
    const parsed: unknown = parse(value);
    const next = versions[replacements + 1];
    if (value === versions[replacements] && next !== undefined) {
      replacements += 1;
      localStorage.setItem(recordKey, next);
    }
    return parsed;
  });

  expect(clearLegacyRecordDrafts()).toBe('retry');

  expect(replacements).toBe(3);
  expect(localStorage.getItem(recordKey)).toBe(versions[3]);
  parser.mockRestore();
  expect(clearLegacyRecordDrafts()).toBe('ready');
  expect(localStorage.getItem(recordKey)).toContain('Unsent racing writing 3');
  expect(localStorage.getItem(recordKey)).not.toContain('SHORT_RACING_');
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
    expect(localStorage.getItem(recordKey)).toBe(value);
    enumeration.mockRestore();

    if (event === 'focus') window.dispatchEvent(new Event(event));
    else {
      vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
      document.dispatchEvent(new Event(event));
    }

    await waitFor(() => {
      expect(localStorage.getItem(recordKey)).not.toContain('SHORT_UNOPENED_TITLE');
    });
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
