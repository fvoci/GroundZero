// 초안 복사본도 본문 PUT 없이 실시간 방을 통해 저장한다(D-27·D-99).
import { getSchema } from '@tiptap/core';
import { prosemirrorJSONToYXmlFragment } from '@tiptap/y-tiptap';
import * as Y from 'yjs';

import type { DocNode } from '$lib/api/client';
import { schemaExtensions } from './editor-schema';
import { closeLive, colorFor, openLive, stopFor } from './live';

/** 새 개인 노트가 초안 본문을 DB에 저장했다고 확인한 뒤에만 원래 초안을 해제한다. */
export async function saveLiveCopy(id: string, body: DocNode, expectedUser: string): Promise<void> {
  const session = openLive(id, { name: '', color: colorFor(expectedUser) }, true, expectedUser);
  let cleanup = (): void => undefined;
  try {
    await new Promise<void>((resolve, reject) => {
      let applied = false;
      const timer = setTimeout(() => {
        reject(new Error('live copy was not confirmed'));
      }, 20_000);
      const synced = (ready: boolean): void => {
        if (!ready || applied) return;
        try {
          prosemirrorJSONToYXmlFragment(
            getSchema(schemaExtensions({ collaborative: true })),
            body,
            session.document.getXmlFragment('default'),
          );
          applied = true;
        } catch (error: unknown) {
          reject(error instanceof Error ? error : new Error('invalid draft content'));
        }
      };
      const persisted = (snapshot: Y.Snapshot): void => {
        if (applied && Y.snapshotContainsUpdate(snapshot, Y.encodeStateAsUpdate(session.document)))
          resolve();
      };
      const closed = (event: CloseEvent | null): void => {
        if (event !== null && stopFor(event.code) !== undefined)
          reject(new Error('live copy was refused'));
      };
      cleanup = () => {
        clearTimeout(timer);
        session.provider.off('sync', synced);
        session.provider.off('connection-close', closed);
        session.persisted.off(persisted);
      };
      session.provider.on('sync', synced);
      session.provider.on('connection-close', closed);
      session.persisted.on(persisted);
      synced(session.provider.synced);
    });
  } finally {
    cleanup();
    closeLive(session);
  }
}
