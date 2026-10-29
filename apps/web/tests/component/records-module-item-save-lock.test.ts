import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView, ItemView, RecordKindView, RecordView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { formDraftTabKey, writeFormDraftAt } from '$lib/features/common/form-draft';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
import { emptyForm } from '$lib/features/records/records';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { itemDocumentPath, router } from '$lib/router.svelte';
import { ACCESS_DOCUMENT } from '../../stories/fixtures/access';
import { openItemGroup, resetRoute } from './fixtures/item-groups';

// R5 2차 delta 2묶음 훑기(코디네이터 2026-10-27)의 같은 부류: 저장을 기다리는 동안 입력·되살리기 제안이 열려 있어
// 스냅숏이 바뀌면 `complete()`/`isCurrent()`가 거짓이 되어 성공을 버린다. 기록 만들기는 다시 내면 새 요청 id로 두 번째
// 기록이 생기고, 고치기는 기준 판을 못 옮겨 다음 저장이 자기 저장과 409로 부딪친다. 모듈 설정은 성공 알림이 숨는다.
// 항목 문서 템플릿은 CreateForm 밖에서 `draftContext`를 고른다(NotesScreen·WorkspaceScreen과 같은 잠금).

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
  resetRoute();
  me.userId = '';
});

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
};

const mod: Module = {
  access: 'owner',
  id: 'm1',
  workspace_id: 'ws-1',
  kind: 'project',
  name: '개발',
  key: 'DEV',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
};

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'DEV-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'owner',
  archived: false,
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

/** 같은 사용자의 다른 탭이 남긴 초안(이 탭에는 되살리기 제안으로 보인다). */
function otherTabDraft(name: string, input: unknown): void {
  const own = formDraftTabKey(name, 'user-a');
  if (own === undefined) throw new Error('draft key');
  writeFormDraftAt(own.replace(/:tab:.*$/, ':tab:other-tab'), {
    savedAt: '2026-10-27T00:00:00.000Z',
    input,
  });
}

function expectOfferLocked(): void {
  expect(screen.getByRole('button', { name: m.common_draft_restore() })).toBeDisabled();
  expect(screen.getByRole('button', { name: m.common_draft_discard() })).toBeDisabled();
}

// FC-13(D4-F-1과 같은 부류): 붙잡은 요청을 끝내 풀지 않으면 잠금 해제와 성공 반영을 보지 않는다. 요청을 성공으로 풀어
// 한 번만 보냈는지, 결과가 반영됐는지, 다른 탭 제안과 입력이 다시 쓸 수 있는지 본다.
function held<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: ((value: T) => void) | undefined;
  const promise = new Promise<T>((ok) => {
    resolve = ok;
  });
  if (resolve === undefined) throw new Error('promise');
  return { promise, resolve };
}

/** 저장이 끝난 뒤 다른 탭 제안과 대표 입력이 다시 쓸 수 있는지. */
async function expectReleased(...inputs: (() => HTMLElement)[]): Promise<void> {
  await waitFor(() => {
    expect(screen.getByRole('button', { name: m.common_draft_restore() })).not.toBeDisabled();
    expect(screen.getByRole('button', { name: m.common_draft_discard() })).not.toBeDisabled();
    for (const input of inputs) expect(input()).not.toBeDisabled();
  });
}

describe('records panel while a record is saved', () => {
  const risk: RecordKindView = {
    kind: 'risk',
    label: '위험',
    module_kind: 'project',
    date_label: null,
    fields: [{ key: 'plan', label: '대응', type: 'long_text', choices: [] }],
    file_roles: [],
  };
  const record: RecordView = {
    id: 'r1',
    kind: 'risk',
    title: '일정 지연',
    occurred_on: null,
    body: '',
    fields: {},
    files: [],
    created_by: 'user-a',
    created_by_name: null,
    updated_at: '2026-10-19T00:00:00Z',
    version: 3,
    can_edit: true,
    can_delete: true,
  };

  function recordServer(records: RecordView[]): void {
    vi.spyOn(api, 'recordKinds').mockResolvedValue([risk]);
    vi.spyOn(api, 'itemRecords').mockResolvedValue(records);
    vi.spyOn(api, 'itemFiles').mockResolvedValue([]);
    vi.spyOn(api, 'members').mockResolvedValue({
      members: [],
      my_role: 'member',
      can_manage_members: false,
      i_am_owner: false,
      pending_owner: null,
    });
  }

  function otherTabRecord(editing: string): void {
    otherTabDraft(`record:${item.id}:risk:${editing}`, {
      form: { ...emptyForm(), title: '다른 탭 기록' },
      baseline: JSON.stringify(emptyForm()),
      version: 3,
      pending: null,
    });
  }

  it('locks the editor and the offer while a new record is created', async () => {
    recordServer([]);
    otherTabRecord('new');
    const request = held<string>();
    const create = vi.spyOn(api, 'createRecord').mockReturnValue(request.promise);
    const done = vi.spyOn(feedback, 'notifyDone');
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.records_new_kind({ label: '위험' }) }),
    );
    const title = screen.getByLabelText<HTMLInputElement>(m.records_title());
    await fireEvent.input(title, { target: { value: '추가한 위험' } });
    await fireEvent.click(
      screen.getByRole('button', { name: m.records_add_kind({ label: '위험' }) }),
    );
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });

    expect(title).toBeDisabled();
    expect(screen.getByLabelText(m.records_body())).toBeDisabled();
    expectOfferLocked();

    vi.mocked(api.itemRecords).mockResolvedValue([
      { ...record, id: 'r-new', title: '추가한 위험' },
    ]);
    request.resolve('r-new');
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(m.records_added({ label: '위험' }));
    });
    expect(create).toHaveBeenCalledOnce();
    expect(await screen.findByText('추가한 위험')).toBeInTheDocument();
    expect(screen.queryByLabelText(m.records_title())).toBeNull();
    // 다른 탭 제안은 그 편집기를 열 때 보인다. 다시 열어 잠금이 풀렸는지 본다.
    await fireEvent.click(
      screen.getByRole('button', { name: m.records_new_kind({ label: '위험' }) }),
    );
    await expectReleased(() => screen.getByLabelText(m.records_title()));
  });

  it('locks the editor and the offer while a record edit is saved', async () => {
    recordServer([record]);
    otherTabRecord('r1');
    const request = held<number>();
    const update = vi.spyOn(api, 'updateRecord').mockReturnValue(request.promise);
    const done = vi.spyOn(feedback, 'notifyDone');
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await screen.findByText('일정 지연');
    await fireEvent.click(screen.getByRole('button', { name: m.records_edit() }));
    const title = screen.getByLabelText<HTMLInputElement>(m.records_title());
    await fireEvent.input(title, { target: { value: '일정 지연(고침)' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledOnce();
    });

    expect(title).toBeDisabled();
    expectOfferLocked();

    // 저장한 판(4)을 다시 읽어 다음 고치기의 기준 판으로 쓴다.
    vi.mocked(api.itemRecords).mockResolvedValue([
      { ...record, title: '일정 지연(고침)', version: 4 },
    ]);
    request.resolve(4);
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(m.records_updated({ label: '위험' }));
    });
    expect(update).toHaveBeenCalledOnce();
    expect(update.mock.calls[0]?.[1]).toBe(3);
    expect(await screen.findByText('일정 지연(고침)')).toBeInTheDocument();
    expect(screen.queryByLabelText(m.records_title())).toBeNull();
    // 다시 열면 다른 탭 제안과 입력이 풀려 있고, 다음 저장은 옮긴 기준 판(4)으로 나간다.
    update.mockResolvedValue(5);
    await fireEvent.click(screen.getByRole('button', { name: m.records_edit() }));
    await expectReleased(() => screen.getByLabelText(m.records_title()));
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[1]).toBe(4);
  });
});

describe('module settings while they are saved', () => {
  it('locks the settings offer and inputs while the settings are saved', async () => {
    otherTabDraft(`module-settings:${mod.id}`, {
      name: '다른 탭 이름',
      key: 'DEV',
      icon: '',
      description: '',
    });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(mod);
    vi.spyOn(api, 'items').mockResolvedValue([]);
    const request = held<Module>();
    const update = vi.spyOn(api, 'updateModule').mockReturnValue(request.promise);
    const done = vi.spyOn(feedback, 'notifyDone');
    render(ModuleScreen, { workspace, moduleId: mod.id });
    const name = await screen.findByLabelText<HTMLInputElement>(m.modules_name());
    await fireEvent.input(name, { target: { value: '새 이름' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledOnce();
    });

    expect(name).toBeDisabled();
    expectOfferLocked();

    request.resolve({ ...mod, name: '새 이름' });
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(m.common_saved(), 'module-settings');
    });
    expect(update).toHaveBeenCalledOnce();
    expect(update).toHaveBeenCalledWith(mod.id, '새 이름', mod.key);
    await expectReleased(() => screen.getByLabelText(m.modules_name()));
  });
});

describe('item document template while the document is created', () => {
  it('locks the template picker while the item document is created', async () => {
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'item').mockResolvedValue(item);
    vi.spyOn(api, 'module').mockResolvedValue(mod);
    vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
    vi.spyOn(api, 'documentTemplates').mockResolvedValue([
      { id: 't-1', title: '회의록 양식', reference: 'WIKI-1', personal: false },
    ]);
    const request = held<DocumentView>();
    const create = vi.spyOn(api, 'createItemDocument').mockReturnValue(request.promise);
    render(ItemScreen, { workspace, itemId: item.id, workspaces: [workspace] });
    await openItemGroup('문서');
    const picker = await screen.findByLabelText(m.documents_template_label());
    await fireEvent.input(screen.getByLabelText(m.modules_new_document_title()), {
      target: { value: '새 문서' },
    });
    await fireEvent.click(screen.getByRole('button', { name: m.modules_create_document() }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });

    expect(picker).toBeDisabled();

    // 만든 문서로 옮겨 간다. 경로 이동은 붙잡아 이 화면에서 잠금이 풀리는지 본다.
    const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
    request.resolve({ ...ACCESS_DOCUMENT, id: 'd-new', title: '새 문서' });
    await waitFor(() => {
      expect(navigate).toHaveBeenCalledWith(itemDocumentPath(workspace.id, item.id, 'd-new'));
    });
    expect(create).toHaveBeenCalledOnce();
    expect(create.mock.calls[0]?.[2]).toBe('새 문서');
    await waitFor(() => {
      expect(screen.getByLabelText(m.documents_template_label())).not.toBeDisabled();
    });
  });
});
