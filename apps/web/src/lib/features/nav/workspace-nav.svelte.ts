// 사이드바와 워크스페이스 화면이 같이 쓰는 현재 워크스페이스의 모듈·위키 문서 목록.
// 같은 목록을 두 곳에서 따로 받지 않도록 한 번 받아 나눠 쓴다. 서버가 권한으로 거른 목록만 담는다.
import type { Module } from '@fvoci/api-client';
import { untrack } from 'svelte';

import type { SummaryView } from '$lib/api/client';

import { api } from '$lib/api/client';

/** 이보다 오래된 목록은 화면을 옮길 때 뒤에서 다시 받는다(다른 화면에서 바뀐 제목·이동 반영). */
const STALE_MS = 15_000;

export type Load<T> =
  { state: 'loading' } | { state: 'ready'; list: T[] } | { state: 'error'; error: unknown };

class WorkspaceNav {
  workspaceId = $state<string | undefined>(undefined);
  modules = $state<Load<Module>>({ state: 'loading' });
  docs = $state<Load<SummaryView>>({ state: 'loading' });
  /** 항목 화면이 알려 주는 항목과 그 모듈(경로에는 항목 id만 있다). 경로의 항목과 같을 때만 쓴다. */
  currentItem = $state<{ itemId: string; moduleId: string } | undefined>(undefined);
  #loadedAt = 0;
  #moduleSeq = 0;
  #docSeq = 0;

  /** 워크스페이스가 바뀌었으면 처음부터, 같으면 오래됐을 때만 다시 받는다. */
  show(workspaceId: string): void {
    // 효과 안에서 불려도 이 저장소의 상태를 의존으로 잡지 않는다(서로 다시 부르는 고리 방지).
    untrack(() => {
      this.#show(workspaceId);
    });
  }

  #show(workspaceId: string): void {
    if (this.workspaceId !== workspaceId) {
      this.workspaceId = workspaceId;
      this.modules = { state: 'loading' };
      this.docs = { state: 'loading' };
      this.currentItem = undefined;
    } else if (Date.now() - this.#loadedAt <= STALE_MS) {
      return;
    }
    this.#loadedAt = Date.now();
    this.refreshModules();
    this.refreshDocs();
  }

  refreshModules(): void {
    const workspaceId = this.workspaceId;
    if (workspaceId === undefined) return;
    const mine = ++this.#moduleSeq;
    const current = (): boolean => mine === this.#moduleSeq && this.workspaceId === workspaceId;
    api.modules(workspaceId).then(
      (list) => {
        if (current()) this.modules = { state: 'ready', list };
      },
      (error: unknown) => {
        if (current()) this.modules = { state: 'error', error };
      },
    );
  }

  refreshDocs(): void {
    const workspaceId = this.workspaceId;
    if (workspaceId === undefined) return;
    const mine = ++this.#docSeq;
    const current = (): boolean => mine === this.#docSeq && this.workspaceId === workspaceId;
    api.wikiDocuments(workspaceId).then(
      (list) => {
        if (current()) this.docs = { state: 'ready', list };
      },
      (error: unknown) => {
        if (current()) this.docs = { state: 'error', error };
      },
    );
  }

  /** 화면이 이미 받은 새 목록을 넣는다(옮기기 직후 등). 진행 중인 이전 요청의 응답은 버린다. */
  setDocs(workspaceId: string, list: SummaryView[]): void {
    if (this.workspaceId !== workspaceId) return;
    this.#docSeq += 1;
    this.docs = { state: 'ready', list };
  }
}

export const workspaceNav = new WorkspaceNav();
