import { expectOk } from '$lib/shared/api';
import { int, none, parseList, rec, str } from '$lib/shared/api/parse';
import { parseRecord, parseRecordKind, type RecordInput, type ZoteroLibraryInput } from './model';

export const recordsApi = {
  /** Zotero 컬렉션(D-34). 키는 이 요청에만 쓰고 서버가 저장하지 않는다. */
  zoteroCollections: (library: ZoteroLibraryInput) =>
    expectOk(
      'POST',
      '/api/zotero/collections',
      (v) =>
        parseList(v, (c) => {
          const o = rec(c, 'zotero collection');
          return { key: str(o, 'key'), name: str(o, 'name') };
        }),
      { ...library },
    ),
  /** Zotero 문헌을 연구 항목의 문헌 기록으로 가져온다(같은 문헌은 서지만 새로). */
  zoteroImport: (itemId: string, library: ZoteroLibraryInput, collectionKey: string | null) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/zotero/import`,
      (v) => {
        const o = rec(v, 'zotero import');
        return {
          created: int(o, 'created'),
          updated: int(o, 'updated'),
          skipped: int(o, 'skipped'),
        };
      },
      { ...library, collection_key: collectionKey },
    ),
  recordKinds: () => expectOk('GET', '/api/record-kinds', (v) => parseList(v, parseRecordKind)),
  itemRecords: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/records`, (v) =>
      parseList(v, parseRecord),
    ),
  createRecord: (itemId: string, requestId: string, kind: string, input: RecordInput) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/records`,
      (v) => str(rec(v, 'created record'), 'id'),
      {
        request_id: requestId,
        kind,
        ...input,
        fields: { ...input.fields },
        files: [...input.files],
      },
    ),
  updateRecord: (id: string, version: number, input: RecordInput) =>
    expectOk(
      'PUT',
      `/api/records/${encodeURIComponent(id)}`,
      (v) => int(rec(v, 'record version'), 'version'),
      { version, ...input, fields: { ...input.fields }, files: [...input.files] },
    ),
  deleteRecord: (id: string) => expectOk('DELETE', `/api/records/${encodeURIComponent(id)}`, none),
};
