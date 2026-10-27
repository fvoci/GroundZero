import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, RecordKindView, RecordView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { formDraftTabKey, writeFormDraftAt } from '$lib/features/common/form-draft';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
import { emptyForm } from '$lib/features/records/records';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
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
  realtime_enabled: false,
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
    const create = vi.spyOn(api, 'createRecord').mockReturnValue(new Promise(() => undefined));
    render(RecordsPanel, { item, kind: 'project', canWrite: true });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.records_new_kind({ label: '위험' }) }),
    );
    const title = screen.getByLabelText<HTMLInputElement>(m.records_title());
    await fireEvent.input(title, { target: { value: '새 위험' } });
    await fireEvent.click(
      screen.getByRole('button', { name: m.records_add_kind({ label: '위험' }) }),
    );
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });

    expect(title).toBeDisabled();
    expect(screen.getByLabelText(m.records_body())).toBeDisabled();
    expectOfferLocked();
  });

  it('locks the editor and the offer while a record edit is saved', async () => {
    recordServer([record]);
    otherTabRecord('r1');
    const update = vi.spyOn(api, 'updateRecord').mockReturnValue(new Promise(() => undefined));
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
    const update = vi.spyOn(api, 'updateModule').mockReturnValue(new Promise(() => undefined));
    render(ModuleScreen, { workspace, moduleId: mod.id });
    const name = await screen.findByLabelText<HTMLInputElement>(m.modules_name());
    await fireEvent.input(name, { target: { value: '새 이름' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledOnce();
    });

    expect(name).toBeDisabled();
    expectOfferLocked();
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
    const create = vi
      .spyOn(api, 'createItemDocument')
      .mockReturnValue(new Promise(() => undefined));
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
  });
});
