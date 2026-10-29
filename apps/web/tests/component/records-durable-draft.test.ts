import './fixtures/authenticated';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ItemView, RecordView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import { formDraftTabKey } from '$lib/features/common/form-draft';
import { clearLegacyRecordDrafts } from '$lib/features/records/legacy-drafts';
import { emptyForm } from '$lib/features/records/records';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

const item: ItemView = {
  id: 'draft-item',
  module_id: 'm1',
  workspace_id: 'ws',
  reference: 'PRJ-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};
const record: RecordView = {
  id: 'record-a',
  kind: 'risk',
  title: '기존 기록',
  occurred_on: null,
  body: '',
  fields: {},
  files: [],
  created_by: 'user-a',
  created_by_name: null,
  updated_at: '',
  version: 3,
  can_edit: true,
  can_delete: true,
};

function input(key: string): HTMLInputElement | HTMLTextAreaElement {
  const value = document.getElementById(`record-risk-${key}`);
  if (value instanceof HTMLInputElement || value instanceof HTMLTextAreaElement) return value;
  throw new Error(`Missing record input ${key}`);
}

async function restoreFirstOffer(): Promise<void> {
  const first = (await screen.findAllByRole('button', { name: m.common_draft_restore() }))[0];
  if (first === undefined) throw new Error('missing draft offer');
  await fireEvent.click(first);
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'recordKinds').mockResolvedValue([
    {
      kind: 'risk',
      label: '위험',
      module_kind: 'project',
      date_label: null,
      fields: [
        { key: 'amount', label: '수량', type: 'number', choices: [] },
        { key: 'mitigation', label: '노트', type: 'long_text', choices: [] },
      ],
      file_roles: [],
    },
  ]);
  vi.spyOn(api, 'itemRecords').mockResolvedValue([record]);
  vi.spyOn(api, 'itemFiles').mockResolvedValue([]);
  vi.spyOn(api, 'members').mockResolvedValue({
    members: [],
    my_role: 'member',
    can_manage_members: false,
    i_am_owner: false,
    pending_owner: null,
  });
});
afterEach(() => vi.restoreAllMocks());

const lateDraftKey = 'fvoci:user-a:-:form-draft:record:unopened:risk:new:held:late-offer';
function lateLegacyDraft(): string {
  return JSON.stringify({
    savedAt: '2026-10-28T12:00:00Z',
    input: {
      form: {
        ...emptyForm(),
        title: 'SHORT_LATE_TITLE',
        body: 'Unsent late body',
        values: { mitigation: 'Unsent mitigation' },
      },
      baseline: JSON.stringify({ ...emptyForm(), values: { mitigation: '' } }),
      version: 0,
      pending: null,
    },
  });
}

function failDraftRead(key: string) {
  const readLocal = localStorage.getItem.bind(localStorage);
  const readSession = sessionStorage.getItem.bind(sessionStorage);
  return vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (
    this: Storage,
    name: string,
  ) {
    if (this === localStorage && name === key)
      throw new DOMException('read unavailable', 'SecurityError');
    return this === sessionStorage ? readSession(name) : readLocal(name);
  });
}

it('allows saving after a server-equal own-key offer could not be held, preserving that key', async () => {
  const key = formDraftTabKey(`record:${item.id}:risk:${record.id}`);
  if (key === undefined) throw new Error('missing own key');
  const value = JSON.stringify({
    savedAt: '2026-10-29T02:00:00Z',
    input: {
      form: { ...emptyForm(), body: record.body, values: { mitigation: '' } },
      baseline: JSON.stringify({ ...emptyForm(), values: { mitigation: '' } }),
      version: 3,
      pending: null,
    },
  });
  localStorage.setItem(key, value);
  const writer = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new DOMException('quota', 'QuotaExceededError');
  });
  const update = vi.spyOn(api, 'updateRecord').mockResolvedValue(4);
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
  await waitFor(() => {
    expect(api.itemRecords).toHaveBeenCalledTimes(2);
  });
  await tick();
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  await fireEvent.input(input('body'), { target: { value: 'New writing after hidden offer' } });
  expect(localStorage.getItem(key)).toBe(value);
  writer.mockRestore();
  await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledOnce();
  });
  expect(localStorage.getItem(key)).toBe(value);
});

it('hides server-equal offers when comparison finishes during a failed save without deleting them', async () => {
  let compare: ((value: RecordView[]) => void) | undefined;
  const comparison = new Promise<RecordView[]>((resolve) => {
    compare = resolve;
  });
  let rejectSave: ((reason: Error) => void) | undefined;
  const saving = new Promise<number>((_resolve, reject) => {
    rejectSave = reject;
  });
  vi.mocked(api.itemRecords).mockResolvedValueOnce([record]).mockReturnValueOnce(comparison);
  vi.spyOn(api, 'updateRecord').mockReturnValue(saving);
  const key = `fvoci:user-a:-:form-draft:record:${item.id}:risk:${record.id}:held:busy`;
  const value = JSON.stringify({
    savedAt: '2026-10-29T02:00:00Z',
    input: {
      form: { ...emptyForm(), values: { mitigation: '' } },
      baseline: JSON.stringify({ ...emptyForm(), values: { mitigation: '' } }),
      version: 3,
      pending: null,
    },
  });
  localStorage.setItem(key, value);
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
  await waitFor(() => {
    expect(api.itemRecords).toHaveBeenCalledTimes(2);
  });
  await fireEvent.input(input('body'), { target: { value: 'Unsent new writing' } });
  await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
  await waitFor(() => {
    expect(api.updateRecord).toHaveBeenCalledOnce();
  });
  if (compare === undefined || rejectSave === undefined)
    throw new Error('missing deferred response');
  compare([record]);
  await tick();
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  rejectSave(new ApiError(403, 'forbidden', 'denied'));
  await waitFor(() => {
    expect(input('body')).toBeEnabled();
  });
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(localStorage.getItem(key)).toBe(value);
});

it.each(['held:server-copy', 'tab:other-tab'])(
  'hides edit writing equal to the current server record without deleting its key (%s)',
  async (suffix) => {
    const saved = {
      ...record,
      body: 'Writing already saved on the server',
      fields: { mitigation: 'Saved mitigation' },
      version: 9,
    };
    vi.mocked(api.itemRecords).mockResolvedValue([saved]);
    const key = `fvoci:user-a:-:form-draft:record:${item.id}:risk:${record.id}:${suffix}`;
    localStorage.setItem(
      key,
      JSON.stringify({
        savedAt: '2026-10-29T02:00:00Z',
        input: {
          form: { ...emptyForm(), body: saved.body, values: { mitigation: 'Saved mitigation' } },
          baseline: JSON.stringify({ ...emptyForm(), values: { mitigation: '' } }),
          version: 3,
          pending: null,
        },
      }),
    );
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await waitFor(() => {
      expect(api.itemRecords).toHaveBeenCalledTimes(2);
    });
    await tick();
    expect(localStorage.getItem(key)).toContain(saved.body);
    expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
    expect(input('body')).toHaveValue(saved.body);
    expect(input('mitigation')).toHaveValue('Saved mitigation');
  },
);

it('keeps a new-record draft even when unrelated server writing is identical', async () => {
  vi.mocked(api.itemRecords).mockResolvedValue([
    { ...record, body: 'Same text, different record' },
  ]);
  const key = `fvoci:user-a:-:form-draft:record:${item.id}:risk:new:held:unrelated`;
  localStorage.setItem(
    key,
    JSON.stringify({
      savedAt: '2026-10-29T02:00:00Z',
      input: {
        form: { ...emptyForm(), body: 'Same text, different record', values: { mitigation: '' } },
        baseline: JSON.stringify({ ...emptyForm(), values: { mitigation: '' } }),
        version: 0,
        pending: null,
      },
    }),
  );
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  expect(await screen.findByTestId('form-draft-offer')).toBeInTheDocument();
  expect(localStorage.getItem(key)).toContain('Same text, different record');
});

it('keeps new writing in memory on quota failure and warns on leave and window return', async () => {
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
  const key = formDraftTabKey('record:draft-item:risk:new');
  if (key === undefined) throw new Error('missing draft key');
  const writer = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new DOMException('storage quota', 'QuotaExceededError');
  });
  await fireEvent.input(input('body'), { target: { value: 'Writing while storage is full' } });
  expect(localStorage.getItem(key)).toBeNull();
  expect(input('body')).toHaveValue('Writing while storage is full');
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  window.dispatchEvent(new Event('focus'));
  await waitFor(() => {
    expect(screen.getByRole('status')).toHaveTextContent(m.records_draft_cleanup_failed());
    expect(screen.getByRole('button', { name: '고치기' })).toBeDisabled();
  });
  expect(input('body')).toHaveValue('Writing while storage is full');
  expect(localStorage.getItem(key)).toBeNull();
  writer.mockRestore();
  await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));
  await waitFor(() => {
    expect(localStorage.getItem(key)).toContain('Writing while storage is full');
  });
  expect(input('body')).toHaveValue('Writing while storage is full');
  expect(screen.getByRole('button', { name: '고치기' })).toBeEnabled();
});

it.each(['unavailable', 'changed source'])(
  'keeps an offered draft when the server comparison is inconclusive (%s)',
  async (reason) => {
    let finish: ((value: RecordView[]) => void) | undefined;
    let fail: ((reason: Error) => void) | undefined;
    const response = new Promise<RecordView[]>((resolve, reject) => {
      finish = resolve;
      fail = reject;
    });
    vi.mocked(api.itemRecords).mockResolvedValueOnce([record]).mockReturnValueOnce(response);
    const key = `fvoci:user-a:-:form-draft:record:${item.id}:risk:${record.id}:held:compare`;
    const copy = {
      savedAt: '2026-10-29T02:00:00Z',
      input: {
        form: { ...emptyForm(), body: 'Candidate writing', values: { mitigation: '' } },
        baseline: JSON.stringify({ ...emptyForm(), values: { mitigation: '' } }),
        version: 3,
        pending: null,
      },
    };
    localStorage.setItem(key, JSON.stringify(copy));
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await waitFor(() => {
      expect(api.itemRecords).toHaveBeenCalledTimes(2);
    });
    expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
    if (reason === 'unavailable') {
      if (fail === undefined) throw new Error('missing server rejection');
      fail(new Error('server read unavailable'));
    } else {
      localStorage.setItem(
        key,
        JSON.stringify({
          ...copy,
          savedAt: '2026-10-29T02:00:01Z',
          input: {
            ...copy.input,
            form: { ...copy.input.form, body: 'Newer writing in the source' },
          },
        }),
      );
      if (finish === undefined) throw new Error('missing server response');
      finish([{ ...record, body: 'Candidate writing' }]);
    }
    expect(await screen.findByTestId('form-draft-offer')).toBeInTheDocument();
    expect(localStorage.getItem(key)).toContain(
      reason === 'unavailable' ? 'Candidate writing' : 'Newer writing in the source',
    );
    if (reason === 'changed source') {
      await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
      expect(input('body')).toHaveValue(record.body);
      expect(localStorage.getItem(key)).toContain('Newer writing in the source');
      // The refreshed proposal needs another explicit choice.
      await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
      expect(input('body')).toHaveValue('Newer writing in the source');
    }
  },
);

it.each(['rewrite', 'remove'])(
  'inspection preserves owner writing across the comparison gap and explicit discard (%s)',
  async (operation) => {
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
    await fireEvent.input(input('title'), { target: { value: 'OWNER_SHORT_TITLE' } });
    await fireEvent.input(input('body'), { target: { value: 'Owner writing after comparison' } });
    await fireEvent.input(input('mitigation'), { target: { value: 'Owner mitigation' } });
    const key = formDraftTabKey('record:draft-item:risk:new');
    if (key === undefined) throw new Error('missing owner key');
    const newer = localStorage.getItem(key);
    if (newer === null) throw new Error('missing owner writing');
    const old: unknown = JSON.parse(lateLegacyDraft());
    if (typeof old !== 'object' || old === null || !('input' in old))
      throw new Error('missing legacy input');
    if (operation === 'remove') {
      const value = old.input;
      if (typeof value !== 'object' || value === null) throw new Error('invalid legacy input');
      Object.assign(value, { pending: { requestId: 'old request', signature: 'old raw body' } });
    }
    const raw = JSON.stringify(old);
    localStorage.setItem(key, raw);
    const readLocal = localStorage.getItem.bind(localStorage);
    const readSession = sessionStorage.getItem.bind(sessionStorage);
    let reads = 0;
    const reader = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (
      this: Storage,
      name: string,
    ) {
      const value = this === localStorage ? readLocal(name) : readSession(name);
      if (this === localStorage && name === key && ++reads === 2) localStorage.setItem(name, newer);
      return value;
    });

    expect(clearLegacyRecordDrafts()).toBe('ready');
    if (operation === 'remove') {
      // B no longer removes invalid keys during cleanup. Model the permitted explicit discard.
      expect(readLocal(key)).toBe(newer);
      localStorage.removeItem(key);
    }
    const overwritten = readLocal(key);
    if (operation === 'remove') expect(overwritten).toBeNull();
    else expect(overwritten).toBe(newer);
    reader.mockRestore();
    window.dispatchEvent(
      new StorageEvent('storage', {
        key,
        oldValue: newer,
        newValue: overwritten,
        storageArea: localStorage,
      }),
    );

    await waitFor(() => {
      expect(localStorage.getItem(key) ?? '').toContain('Owner writing after comparison');
    });
    expect(localStorage.getItem(key)).toContain('Owner mitigation');
    expect(localStorage.getItem(key)).not.toContain('OWNER_SHORT_TITLE');
    expect(input('body')).toHaveValue('Owner writing after comparison');
    expect(input('title')).toHaveValue('OWNER_SHORT_TITLE');
  },
);

it('saving a restored draft after a failed copy does not delete later writing in its source', async () => {
  const key = `fvoci:user-a:-:form-draft:record:${item.id}:risk:new:tab:other-tab`;
  const copy = {
    savedAt: '2026-10-29T02:00:00Z',
    input: {
      form: { ...emptyForm(), body: 'Selected writing', values: { mitigation: '' } },
      baseline: JSON.stringify({ ...emptyForm(), values: { mitigation: '' } }),
      version: 0,
      pending: null,
    },
  };
  localStorage.setItem(key, JSON.stringify(copy));
  vi.spyOn(api, 'createRecord').mockResolvedValue('saved-record');
  const done = vi.spyOn(feedback, 'notifyDone');
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
  const writer = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new DOMException('copy unavailable', 'QuotaExceededError');
  });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(input('body')).toHaveValue('Selected writing');
  writer.mockRestore();
  localStorage.setItem(
    key,
    JSON.stringify({
      ...copy,
      savedAt: '2026-10-29T02:00:01Z',
      input: {
        ...copy.input,
        form: { ...copy.input.form, body: 'Later writing in the other source' },
      },
    }),
  );
  await fireEvent.input(input('title'), { target: { value: 'Save selected writing' } });
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  await waitFor(() => {
    expect(done).toHaveBeenCalledWith(m.records_added({ label: '위험' }));
  });
  const remaining = localStorage.getItem(key);
  expect(remaining).not.toBeNull();
  expect(remaining ?? '').toContain('Later writing in the other source');
});

it.each(['after render', 'before render'])(
  'blocks offered writing from entering an editor during failed cleanup (%s)',
  async (timing) => {
    const offeredKey = lateDraftKey.replace('unopened', item.id);
    localStorage.setItem(offeredKey, lateLegacyDraft());
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    const restore = await screen.findByRole('button', { name: m.common_draft_restore() });
    expect(restore).toBeEnabled();
    expect(document.getElementById('record-risk-body')).toBeNull();
    const enumeration = vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
      throw new DOMException('enumeration unavailable', 'SecurityError');
    });

    window.dispatchEvent(new Event('focus'));
    if (timing === 'before render') {
      // Dispatch in the same turn, while the DOM still has the old enabled button.
      restore.click();
      await waitFor(() => {
        expect(screen.getByRole('button', { name: '새 위험' })).toBeDisabled();
      });
      expect(document.getElementById('record-risk-body')).toBeNull();
    } else {
      await waitFor(() => {
        expect(restore).toBeDisabled();
      });
      await fireEvent.click(restore);
      expect(document.getElementById('record-risk-body')).toBeNull();
    }
    expect(localStorage.getItem(offeredKey)).toContain('Unsent late body');
    const discard = screen.getByRole('button', { name: m.common_draft_discard() });
    expect(discard).toBeEnabled();
    enumeration.mockRestore();
    if (timing === 'after render') {
      // A failed cleanup must not prevent the user's explicit discard.
      await fireEvent.click(discard);
      await waitFor(() => {
        expect(localStorage.getItem(offeredKey)).toBeNull();
      });
    } else {
      await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));
      await waitFor(() => {
        expect(restore).toBeEnabled();
      });
      await fireEvent.click(restore);
      await waitFor(() => expect(input('body')).toHaveValue('Unsent late body'));
      expect(input('mitigation')).toHaveValue('Unsent mitigation');
    }
  },
);

it.each(['focus', 'visibilitychange'])(
  'blocks entry when %s cleanup fails after the record panel was ready',
  async (event) => {
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    const create = await screen.findByRole('button', { name: '새 위험' });
    expect(create).toBeEnabled();
    const raw = lateLegacyDraft();
    localStorage.setItem(lateDraftKey, raw);
    const enumeration = vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
      throw new DOMException('enumeration unavailable', 'SecurityError');
    });

    if (event === 'focus') window.dispatchEvent(new Event(event));
    else {
      vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
      document.dispatchEvent(new Event(event));
    }

    await waitFor(() => {
      expect(create).toBeDisabled();
    });
    expect(screen.getByRole('button', { name: '고치기' })).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent(
      '저장된 기록 초안을 확인하지 못했습니다. 다시 시도해 주세요.',
    );
    expect(localStorage.getItem(lateDraftKey)).toBe(raw);
    enumeration.mockRestore();
    await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));

    await waitFor(() => {
      expect(create).toBeEnabled();
    });
    expect(localStorage.getItem(lateDraftKey)).toBe(raw);
    expect(localStorage.getItem(lateDraftKey)).toContain('Unsent late body');
  },
);

it('blocks ready record entry after storage cleanup fails until all pending drafts are repaired', async () => {
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  const create = await screen.findByRole('button', { name: '새 위험' });
  expect(create).toBeEnabled();
  const raw = lateLegacyDraft();
  localStorage.setItem(lateDraftKey, raw);
  const unavailable = failDraftRead(lateDraftKey);

  window.dispatchEvent(
    new StorageEvent('storage', { key: lateDraftKey, newValue: raw, storageArea: localStorage }),
  );

  await waitFor(() => {
    expect(create).toBeDisabled();
  });
  expect(screen.getByRole('status')).toHaveTextContent(
    '저장된 기록 초안을 확인하지 못했습니다. 다시 시도해 주세요.',
  );
  const otherKey = lateDraftKey.replace('unopened', 'another-record');
  const current = JSON.stringify({
    savedAt: 'now',
    input: {
      form: { ...emptyForm(), body: 'Current writing', values: { mitigation: '' } },
      baseline: JSON.stringify({ ...emptyForm(), values: { mitigation: '' } }),
      version: 0,
      pending: null,
    },
  });
  localStorage.setItem(otherKey, current);
  window.dispatchEvent(
    new StorageEvent('storage', { key: otherKey, newValue: current, storageArea: localStorage }),
  );
  expect(create).toBeDisabled();
  unavailable.mockRestore();
  await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));

  await waitFor(() => {
    expect(create).toBeEnabled();
  });
  expect(localStorage.getItem(lateDraftKey)).toBe(raw);
  expect(localStorage.getItem(lateDraftKey)).toContain('Unsent late body');
});

it.each(['focus', 'storage'])(
  'keeps the open record session and long writing when %s cleanup fails and recovers',
  async (event) => {
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
    await fireEvent.input(input('title'), { target: { value: 'Live short title' } });
    await fireEvent.input(input('body'), { target: { value: 'Live long body' } });
    await fireEvent.input(input('mitigation'), { target: { value: 'Live long mitigation' } });
    const ownKey = formDraftTabKey('record:draft-item:risk:new');
    if (ownKey === undefined) throw new Error('missing record key');
    await waitFor(() => {
      expect(localStorage.getItem(ownKey)).toContain('Live long body');
    });
    const raw = lateLegacyDraft();
    localStorage.setItem(lateDraftKey, raw);
    const unavailable =
      event === 'storage'
        ? failDraftRead(lateDraftKey)
        : vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
            throw new DOMException('enumeration unavailable', 'SecurityError');
          });

    if (event === 'focus') window.dispatchEvent(new Event(event));
    else
      window.dispatchEvent(
        new StorageEvent('storage', {
          key: lateDraftKey,
          newValue: raw,
          storageArea: localStorage,
        }),
      );

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '고치기' })).toBeDisabled();
    });
    expect(input('body')).toHaveValue('Live long body');
    expect(input('mitigation')).toHaveValue('Live long mitigation');
    expect(input('title')).toHaveValue('Live short title');
    await fireEvent.input(input('body'), {
      target: { value: 'More long writing during cleanup failure' },
    });
    await waitFor(() => {
      expect(localStorage.getItem(ownKey)).toContain('More long writing during cleanup failure');
    });
    expect(localStorage.getItem(ownKey)).not.toContain('Live short title');
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    unavailable.mockRestore();
    await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '고치기' })).toBeEnabled();
    });
    expect(input('body')).toHaveValue('More long writing during cleanup failure');
    expect(input('mitigation')).toHaveValue('Live long mitigation');
    expect(input('title')).toHaveValue('Live short title');
    expect(localStorage.getItem(lateDraftKey)).toBe(raw);
  },
);

it('defers record draft offers after failed panel cleanup until window return repairs the original held key', async () => {
  const key = 'fvoci:user-a:-:form-draft:record:draft-item:risk:new:held:old-offer';
  const form = {
    ...emptyForm(),
    title: 'SHORT_TRANSIENT_TITLE',
    body: 'Unsent transient body',
    values: { mitigation: 'Unsent mitigation' },
  };
  const raw = JSON.stringify({
    savedAt: '2026-10-28T12:00:00Z',
    input: { form, baseline: JSON.stringify(emptyForm()), version: 3, pending: null },
  });
  localStorage.setItem(key, raw);
  const startupEnumeration = vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
    throw new DOMException('startup enumeration unavailable', 'SecurityError');
  });
  clearLegacyRecordDrafts();
  startupEnumeration.mockRestore();
  const panelEnumeration = vi.spyOn(Storage.prototype, 'key').mockImplementationOnce(() => {
    throw new DOMException('transient panel enumeration failure', 'SecurityError');
  });

  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await screen.findByRole('button', { name: '새 위험' });

  expect(panelEnumeration).toHaveBeenCalled();
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(localStorage.getItem(key)).toBe(raw);
  window.dispatchEvent(new Event('focus'));

  await screen.findByRole('button', { name: m.common_draft_restore() });
  expect(localStorage.getItem(key)).toBe(raw);
  expect(localStorage.getItem(key)).toContain('Unsent transient body');
});

it('blocks record entry after failed cleanup and repairs drafts before manual retry enables entry', async () => {
  const key = 'fvoci:user-a:-:form-draft:record:draft-item:risk:new:held:retry-offer';
  const raw = JSON.stringify({
    savedAt: '2026-10-28T12:00:00Z',
    input: {
      form: { ...emptyForm(), title: 'SHORT_RETRY_TITLE', body: 'Unsent retry body' },
      baseline: JSON.stringify(emptyForm()),
      version: 0,
      pending: null,
    },
  });
  localStorage.setItem(key, raw);
  const enumeration = vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
    throw new DOMException('enumeration unavailable', 'SecurityError');
  });
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  const create = await screen.findByRole('button', { name: '새 위험' });

  expect(create).toBeDisabled();
  expect(screen.getByRole('button', { name: '고치기' })).toBeDisabled();
  expect(screen.getByText(record.title)).toBeInTheDocument();
  expect(screen.getByRole('status')).toHaveTextContent(
    '저장된 기록 초안을 확인하지 못했습니다. 다시 시도해 주세요.',
  );
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(localStorage.getItem(key)).toBe(raw);
  enumeration.mockRestore();

  await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));

  await screen.findByRole('button', { name: m.common_draft_restore() });
  expect(localStorage.getItem(key)).toBe(raw);
  expect(localStorage.getItem(key)).toContain('Unsent retry body');
  expect(create).toBeEnabled();
  expect(screen.getByRole('button', { name: '고치기' })).toBeEnabled();
  expect(screen.getByRole('status').textContent).toBe('');
});

it('enables new and edit entry immediately on the normal first render', async () => {
  const disabled = vi.spyOn(HTMLButtonElement.prototype, 'disabled', 'set');
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  const create = await screen.findByRole('button', { name: '새 위험' });
  const edit = screen.getByRole('button', { name: '고치기' });

  expect(create).toBeEnabled();
  expect(edit).toBeEnabled();
  expect(
    disabled.mock.calls.some(
      ([value], index) =>
        value &&
        (disabled.mock.contexts[index] === create || disabled.mock.contexts[index] === edit),
    ),
  ).toBe(false);
});

it.each(['held:old-offer', 'tab:other-tab'])(
  'recovers a missed legacy full form without rewriting its original %s key',
  async (suffix) => {
    const key = `fvoci:user-a:-:form-draft:record:draft-item:risk:new:${suffix}`;
    const form = {
      ...emptyForm(),
      title: 'SHORT_ORIGINAL_TITLE',
      body: 'Unsent original body',
      values: { mitigation: 'Unsent mitigation', amount: 'SHORT_AMOUNT' },
    };
    localStorage.setItem(
      key,
      JSON.stringify({
        savedAt: '2026-10-28T12:00:00Z',
        input: {
          form,
          baseline: JSON.stringify({ ...form, title: 'SHORT_BASE', body: 'Before editing' }),
          version: 3,
          pending: null,
        },
      }),
    );
    const enumeration = vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
      throw new DOMException('enumeration unavailable', 'SecurityError');
    });
    clearLegacyRecordDrafts();
    enumeration.mockRestore();
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
    await screen.findByRole('button', { name: m.common_draft_restore() });

    const value = localStorage.getItem(key);
    expect(value).not.toBeNull();
    expect(value).toContain('SHORT_ORIGINAL_TITLE');
    expect(value).toContain('Unsent original body');
    expect(value).toContain('Unsent mitigation');
    expect(value).toContain('Before editing');
    await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
    expect(input('body')).toHaveValue('Unsent original body');
    const ownKey = formDraftTabKey('record:draft-item:risk:new');
    expect(localStorage.getItem(ownKey ?? '')).toContain('Unsent original body');
    expect(localStorage.getItem(ownKey ?? '')).not.toContain('SHORT_');
    expect(localStorage.getItem(key)).toBe(value);
  },
);

it.each(['held:old-offer', 'tab:other-tab', 'own'])(
  'retains malformed keys without offering them after startup cleanup failed (%s)',
  async (suffix) => {
    const key =
      suffix === 'own'
        ? formDraftTabKey('record:draft-item:risk:new')
        : `fvoci:user-a:-:form-draft:record:draft-item:risk:new:${suffix}`;
    if (key === undefined) throw new Error('missing draft key');
    localStorage.setItem(key, '{"input":{"form":{"title":"SHORT_BROKEN"');
    const enumeration = vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
      throw new DOMException('enumeration unavailable', 'SecurityError');
    });
    clearLegacyRecordDrafts();
    enumeration.mockRestore();
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));

    expect(localStorage.getItem(key)).toBe('{"input":{"form":{"title":"SHORT_BROKEN"');
    expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
    if (suffix === 'own') {
      vi.spyOn(api, 'createRecord').mockResolvedValue('saved-beside-malformed');
      await fireEvent.input(input('title'), { target: { value: 'New record' } });
      await fireEvent.input(input('body'), {
        target: { value: 'New writing beside malformed source' },
      });
      await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
      await waitFor(() => {
        expect(api.createRecord).toHaveBeenCalledOnce();
      });
      expect(localStorage.getItem(key)).toBe('{"input":{"form":{"title":"SHORT_BROKEN"');
    }
  },
);

it('copies only writing to a fresh own key when recovering an original at this tab key', async () => {
  const key = formDraftTabKey('record:draft-item:risk:new');
  if (key === undefined) throw new Error('missing record draft key');
  const form = {
    ...emptyForm(),
    title: 'SHORT_MISSED_TITLE',
    occurred_on: '2026-10-28',
    body: 'Unsent missed body',
    values: { mitigation: 'Unsent mitigation', amount: 'SHORT_AMOUNT', owner: 'SHORT_PERSON' },
    people: { attendees: ['SHORT_ATTENDEE'] },
    files: [{ file_id: 'SHORT_FILE', version: '2', role: 'SHORT_ROLE' }],
    unreadable: { amount: true },
  };
  localStorage.setItem(
    key,
    JSON.stringify({
      savedAt: '2026-10-28T12:00:00Z',
      input: {
        form,
        baseline: JSON.stringify({ ...form, title: 'SHORT_BASE_TITLE', body: 'Before editing' }),
        version: 7,
        pending: null,
      },
    }),
  );
  const original = localStorage.getItem(key);
  const enumeration = vi.spyOn(Storage.prototype, 'key').mockImplementation(() => {
    throw new DOMException('enumeration unavailable', 'SecurityError');
  });
  clearLegacyRecordDrafts();
  enumeration.mockRestore();
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
  const restore = await screen.findByRole('button', { name: m.common_draft_restore() });
  expect(localStorage.getItem(key)).toBe(original);
  await fireEvent.click(restore);
  const recoveredKey = Object.keys(localStorage).find(
    (value) =>
      value !== key &&
      value.startsWith('fvoci:user-a:-:form-draft:record:draft-item:risk:new:tab:'),
  );
  expect(recoveredKey).toBeDefined();
  const held = localStorage.getItem(recoveredKey ?? '');
  expect(held).not.toContain('SHORT_');
  const savedAt: unknown = expect.any(String);
  expect(JSON.parse(held ?? 'null')).toEqual({
    savedAt,
    input: {
      form: {
        ...emptyForm(),
        body: 'Unsent missed body',
        values: { mitigation: 'Unsent mitigation' },
      },
      baseline: JSON.stringify({
        ...emptyForm(),
        body: 'Before editing',
        values: { mitigation: 'Unsent mitigation' },
      }),
      version: 7,
      pending: null,
    },
  });
  await waitFor(() => expect(input('body')).toHaveValue('Unsent missed body'));
  expect(input('mitigation')).toHaveValue('Unsent mitigation');
  expect(localStorage.getItem(key)).toBe(original);
  const create = vi.spyOn(api, 'createRecord').mockResolvedValue('saved-copy');
  await fireEvent.input(input('title'), { target: { value: 'Recovered record' } });
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledOnce();
  });
  expect(localStorage.getItem(key)).toBe(original);
  expect(localStorage.getItem(recoveredKey ?? '')).toBeNull();
});

it.each(['broken baseline', 'unknown kind'])(
  'does not offer a missed record draft with %s',
  async (problem) => {
    const kind = problem === 'unknown kind' ? 'unknown_kind' : 'risk';
    vi.mocked(api.recordKinds).mockResolvedValue([
      { kind, label: '위험', module_kind: 'project', date_label: null, fields: [], file_roles: [] },
    ]);
    const key = `fvoci:user-a:-:form-draft:record:draft-item:${kind}:new:held:old-tab`;
    localStorage.setItem(
      key,
      JSON.stringify({
        savedAt: '2026-10-28T12:00:00Z',
        input: {
          form: {
            ...emptyForm(),
            body: 'Untrusted draft',
          },
          baseline: problem === 'broken baseline' ? '{' : JSON.stringify(emptyForm()),
          version: 0,
          pending: null,
        },
      }),
    );
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));

    expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  },
);

it.each([
  [
    'raw request signature',
    { requestId: '11111111-1111-4111-8111-111111111111', signature: 'LEGACY_REQUEST_BODY' },
  ],
  ['non-UUID request ID', { requestId: 'LEGACY_REQUEST_BODY', signature: 'a'.repeat(64) }],
  [
    'additional request property',
    {
      requestId: '11111111-1111-4111-8111-111111111111',
      signature: 'a'.repeat(64),
      request: 'LEGACY_REQUEST_BODY',
    },
  ],
])(
  'recovers writing read after startup while removing invalid pending: %s',
  async (_name, pending) => {
    clearLegacyRecordDrafts();
    const key = 'fvoci:user-a:-:form-draft:record:draft-item:risk:new:held:old-tab';
    localStorage.setItem(
      key,
      JSON.stringify({
        savedAt: '2026-10-28T12:00:00Z',
        input: {
          form: { ...emptyForm(), body: 'Recoverable long writing', extra: 'SHORT_BROKEN' },
          baseline: JSON.stringify(emptyForm()),
          version: 0,
          pending,
        },
      }),
    );
    const original = localStorage.getItem(key);
    const writes = vi.spyOn(Storage.prototype, 'setItem');
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
    const restore = screen.queryByRole('button', { name: m.common_draft_restore() });
    if (restore !== null) await fireEvent.click(restore);

    expect(input('body')).toHaveValue('Recoverable long writing');
    expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
    expect(localStorage.getItem(key)).toBe(original);
    const ownKey = formDraftTabKey('record:draft-item:risk:new');
    const stored = localStorage.getItem(ownKey ?? '');
    expect(stored).toContain('Recoverable long writing');
    expect(stored).not.toContain('LEGACY_');
    expect(stored).not.toContain('SHORT_BROKEN');
    expect(JSON.parse(stored ?? 'null')).toMatchObject({ input: { pending: null } });
    expect(writes.mock.calls.map((call) => call[1]).join('')).not.toContain('LEGACY_');
  },
);

it("ignores another account's late record success without changing the new input or status", async () => {
  const done = vi.spyOn(feedback, 'notifyDone');
  let finish: ((value: string) => void) | undefined;
  const pending = new Promise<string>((resolve) => {
    finish = resolve;
  });
  vi.spyOn(api, 'createRecord').mockReturnValue(pending);
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
  await fireEvent.input(input('title'), { target: { value: 'user-a input' } });
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  me.userId = 'user-b';
  await waitFor(() => expect(input('title')).toHaveValue(''));
  await fireEvent.input(input('title'), { target: { value: 'user-b input' } });
  if (finish === undefined) throw new Error('missing completion');
  finish(record.id);
  await pending;
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  expect(input('title')).toHaveValue('user-b input');
  // 일반 완료는 토스트다(D-82). 다른 계정의 늦은 성공은 알리지 않는다.
  expect(done).not.toHaveBeenCalled();
});

it('keeps raw numbers on screen, restores only long text and reuses the same retry identity', async () => {
  const create = vi
    .spyOn(api, 'createRecord')
    .mockRejectedValue(new ApiError(403, 'forbidden', 'denied'));
  const view = render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
  await fireEvent.input(input('title'), { target: { value: '보존할 제목' } });
  await fireEvent.input(input('amount'), { target: { value: '1e' } });
  await fireEvent.input(input('mitigation'), { target: { value: '긴 입력\n두 번째 줄' } });
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  expect(input('amount')).toHaveValue('1e');
  await fireEvent.click(screen.getByRole('button', { name: m.common_cancel() }));
  expect(screen.getByRole('alertdialog')).toBeInTheDocument();
  view.unmount();
  const restored = render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await restoreFirstOffer();
  await waitFor(() => expect(input('mitigation')).toHaveValue('긴 입력\n두 번째 줄'));
  expect(input('title')).toHaveValue('');
  expect(input('amount')).toHaveValue('');
  expect(input('mitigation')).toHaveValue('긴 입력\n두 번째 줄');
  expect(create).not.toHaveBeenCalled();
  await fireEvent.input(input('title'), { target: { value: '보존할 제목' } });
  await fireEvent.input(input('amount'), { target: { value: '12' } });
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledOnce();
  });
  expect(input('title')).toHaveValue('보존할 제목');
  const requestId = create.mock.calls[0]?.[1];
  restored.unmount();
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await restoreFirstOffer();
  await waitFor(() => expect(input('mitigation')).toHaveValue('긴 입력\n두 번째 줄'));
  expect(create).toHaveBeenCalledTimes(1);
  expect(input('title')).toHaveValue('');
  expect(input('amount')).toHaveValue('');
  await fireEvent.input(input('title'), { target: { value: '보존할 제목' } });
  await fireEvent.input(input('amount'), { target: { value: '12' } });
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(2);
  });
  expect(create.mock.calls[1]?.[1]).toBe(requestId);
});

it('restores the edit-start version after a remount and keeps another account isolated', async () => {
  const update = vi
    .spyOn(api, 'updateRecord')
    .mockRejectedValue(new ApiError(403, 'forbidden', 'denied'));
  const view = render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
  await fireEvent.input(input('body'), { target: { value: '개인 긴 입력' } });
  me.userId = 'user-b';
  await waitFor(() => expect(input('body')).toHaveValue(''));
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  me.userId = 'user-a';
  await restoreFirstOffer();
  await waitFor(() => expect(input('body')).toHaveValue('개인 긴 입력'));
  view.unmount();
  vi.mocked(api.itemRecords).mockResolvedValue([{ ...record, version: 8 }]);
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
  await restoreFirstOffer();
  await waitFor(() => expect(input('body')).toHaveValue('개인 긴 입력'));
  expect(update).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledWith(
      record.id,
      3,
      expect.objectContaining({ title: record.title, body: '개인 긴 입력' }),
    );
  });
});

it('ignores a conflict reload that finishes after the editing account changes', async () => {
  vi.spyOn(api, 'updateRecord').mockRejectedValue(new ApiError(409, 'conflict', 'changed'));
  let finish: ((value: RecordView[]) => void) | undefined;
  const pending = new Promise<RecordView[]>((resolve) => {
    finish = resolve;
  });
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
  await fireEvent.input(input('title'), { target: { value: 'user-a edit' } });
  vi.mocked(api.itemRecords).mockReturnValue(pending);
  await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
  await waitFor(() => {
    expect(api.itemRecords).toHaveBeenCalledTimes(2);
  });
  me.userId = 'user-b';
  await waitFor(() => expect(input('title')).toHaveValue(''));
  if (finish === undefined) throw new Error('missing conflict response');
  finish([{ ...record, version: 4, title: 'old account response' }]);
  await pending;
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  expect(screen.queryByText('old account response')).not.toBeInTheDocument();
  expect(input('title')).toHaveValue('');
});
