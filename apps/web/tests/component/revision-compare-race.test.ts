import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView, RevisionView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import RevisionHistory from '$lib/features/documents/RevisionHistory.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

function paragraph(text: string): DocumentView['body'] {
  return { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] };
}

const TEXT: Record<number, string> = {
  2: '둘째 문장',
  3: '셋째 문장',
  9: '아홉째 문장',
  10: '열째 문장',
};

function revisionAt(revision: number): RevisionView {
  return {
    revision,
    title: '문서',
    author_name: '나',
    created_at: '2026-10-19T00:00:00Z',
    restored_from: null,
    body: paragraph(TEXT[revision] ?? ''),
    schema_version: 1,
  };
}

function documentAt(revision: number): DocumentView {
  return {
    id: 'd1',
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: 'WIKI-1',
    title: '문서',
    body: paragraph('지금 문장'),
    schema_version: 1,
    revision,
    updated_at: '2026-10-19T00:00:00Z',
    access: 'owner',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: true,
    can_manage: true,
    can_delete: true,
    is_template: false,
    realtime: false,
    can_export: true,
  };
}

interface Waiting {
  resolve: (value: RevisionView) => void;
  reject: (error: unknown) => void;
}

/** 리비전마다 손으로 끝내는(또는 실패시키는) 응답. */
function pendingRevisions(): {
  resolve: (revision: number) => Promise<void>;
  reject: (revision: number) => Promise<void>;
} {
  const waiting = new Map<number, Waiting>();
  vi.spyOn(api, 'revision').mockImplementation(
    (_id: string, revision: number) =>
      new Promise<RevisionView>((resolve, reject) => {
        waiting.set(revision, { resolve, reject });
      }),
  );
  const take = (revision: number): Waiting => {
    const found = waiting.get(revision);
    if (found === undefined) throw new Error(`r${String(revision)} was not requested`);
    waiting.delete(revision);
    return found;
  };
  const settle = async (): Promise<void> => {
    await tick();
    await new Promise((r) => setTimeout(r, 0));
  };
  return {
    resolve: async (revision: number) => {
      take(revision).resolve(revisionAt(revision));
      await settle();
    },
    reject: async (revision: number) => {
      take(revision).reject(new NetworkError('offline'));
      await settle();
    },
  };
}

function hint(): string {
  return screen.getByText(/: 이 리비전에서 바뀐 것$/).textContent;
}

async function openHistory(): Promise<void> {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'revisions').mockResolvedValue([
    revisionAt(10),
    revisionAt(9),
    revisionAt(3),
    revisionAt(2),
  ]);
  render(RevisionHistory, { props: { doc: documentAt(11), onrestored: () => undefined } });
  await fireEvent.click(screen.getByRole('button', { name: '기록' }));
  await screen.findByRole('option', { name: /^r3 / });
}

// 감사 W12: "직전과 비교"는 고른 리비전과 그 바로 앞 리비전의 차이만 보인다. 앞서 고른 리비전의 직전 리비전(또는 늦게
// 도착한 앞선 응답)으로 다른 쌍을 비교하면서 "r9 → r10"이라고 이름 붙이지 않는다.
describe('compare with previous', () => {
  it('does not diff against the earlier selection’s previous revision while the new one loads', async () => {
    const revisions = pendingRevisions();
    await openHistory();
    await fireEvent.click(screen.getByRole('option', { name: /^r3 / }));
    await revisions.resolve(3);
    await revisions.resolve(2);
    await fireEvent.click(screen.getByRole('button', { name: '직전과 비교' }));
    expect(hint()).toBe('r2 → r3: 이 리비전에서 바뀐 것');

    await fireEvent.click(screen.getByRole('option', { name: /^r10 / }));
    await revisions.resolve(10);
    // r9는 아직 오지 않았다: r2 → r10을 "r9 → r10"으로 보이지 않는다.
    expect(hint()).toBe('r9 → r10: 이 리비전에서 바뀐 것');
    expect(screen.getByTestId('revision-diff').textContent).not.toContain('둘째 문장');
    expect(screen.getByTestId('revision-diff').textContent).toContain('불러오는 중…');

    await revisions.resolve(9);
    await waitFor(() => {
      expect(screen.getByTestId('revision-diff').textContent).toContain('아홉째 문장');
    });
    expect(screen.getByTestId('revision-diff').textContent).toContain('열째 문장');
  });

  it('keeps only the last click when the earlier responses arrive later', async () => {
    const revisions = pendingRevisions();
    await openHistory();
    await fireEvent.click(screen.getByRole('option', { name: /^r3 / }));
    await fireEvent.click(screen.getByRole('option', { name: /^r10 / }));
    await revisions.resolve(10);
    await revisions.resolve(9);
    await fireEvent.click(screen.getByRole('button', { name: '직전과 비교' }));
    // 먼저 누른 r3의 응답이 늦게 온다. 버려야 할 응답이므로 그 직전 리비전(r2)은 묻지 않아도 된다.
    await revisions.resolve(3);
    await revisions.resolve(2).catch(() => undefined);

    expect(screen.getByRole('option', { name: /^r10 / })).toHaveAttribute('aria-selected', 'true');
    expect(hint()).toBe('r9 → r10: 이 리비전에서 바뀐 것');
    const diff = screen.getByTestId('revision-diff').textContent;
    expect(diff).toContain('아홉째 문장');
    expect(diff).toContain('열째 문장');
    expect(diff).not.toContain('둘째 문장');
  });

  it('stops saying loading when the previous revision fails to load', async () => {
    const revisions = pendingRevisions();
    await openHistory();
    await fireEvent.click(screen.getByRole('option', { name: /^r10 / }));
    await revisions.resolve(10);
    await fireEvent.click(screen.getByRole('button', { name: '직전과 비교' }));
    expect(screen.getByTestId('revision-diff').textContent).toContain('불러오는 중…');

    // 직전 리비전(r9)을 불러오지 못했다: 끝없이 "불러오는 중…"이라고 하지 않고, 차이가 없다고도 하지 않는다.
    await revisions.reject(9);
    expect(screen.getByRole('alert').textContent).toContain('리비전을 불러오지 못했습니다.');
    const diff = screen.getByTestId('revision-diff').textContent;
    expect(diff).not.toContain('불러오는 중…');
    expect(diff).not.toContain('차이가 없습니다.');
    expect(diff).not.toContain('열째 문장');
  });
});
