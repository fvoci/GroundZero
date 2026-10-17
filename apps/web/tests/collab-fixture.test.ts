// 실시간 공동 편집(D-70)의 문서 형식 기준. 브라우저(@tiptap/y-tiptap)와 서버(yrs, crates/server/src/collab/convert.rs)가
// 같은 Yjs 표현을 쓰는지 contracts/collab의 고정 파일로 확인한다. 스키마가 바뀌면
// `FVOCI_UPDATE_COLLAB_FIXTURES=1 pnpm --filter @fvoci/web test`로 다시 만든 뒤 서버 테스트도 다시 돌린다.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { getSchema } from '@tiptap/core';
import { prosemirrorJSONToYXmlFragment, yXmlFragmentToProsemirrorJSON } from '@tiptap/y-tiptap';
import * as Y from 'yjs';

import { schemaExtensions } from '$lib/features/documents/editor-schema';

const dir = path.resolve(import.meta.dirname, '../../../contracts/collab');
const read = (name: string): string => readFileSync(path.join(dir, name), 'utf8');
const json = (value: unknown): string => `${JSON.stringify(value, null, 2)}\n`;
const update = process.env.FVOCI_UPDATE_COLLAB_FIXTURES === '1';

function fragmentJson(bytes: Uint8Array): unknown {
  const doc = new Y.Doc();
  Y.applyUpdate(doc, bytes);
  return yXmlFragmentToProsemirrorJSON(doc.getXmlFragment('default'));
}

describe('collaboration fixtures', () => {
  const schema = getSchema(schemaExtensions());
  const input: unknown = JSON.parse(read('input.json'));
  const stored: unknown = schema.nodeFromJSON(input).toJSON();
  const doc = new Y.Doc();
  doc.clientID = 1;
  prosemirrorJSONToYXmlFragment(schema, stored, doc.getXmlFragment('default'));
  const bytes = Y.encodeStateAsUpdate(doc);
  const expected = yXmlFragmentToProsemirrorJSON(doc.getXmlFragment('default'));

  if (update) {
    writeFileSync(path.join(dir, 'stored.json'), json(stored));
    writeFileSync(path.join(dir, 'from-js.b64'), `${Buffer.from(bytes).toString('base64')}\n`);
    writeFileSync(path.join(dir, 'expected.json'), json(expected));
  }

  it('stores what the editor stores', () => {
    expect(JSON.parse(read('stored.json'))).toEqual(stored);
  });

  it('encodes the same Yjs update', () => {
    expect(read('from-js.b64').trim()).toBe(Buffer.from(bytes).toString('base64'));
    expect(JSON.parse(read('expected.json'))).toEqual(expected);
  });

  it('reads what the server encodes', () => {
    const file = path.join(dir, 'from-rust.b64');
    expect(existsSync(file)).toBe(true);
    const fromRust = new Uint8Array(Buffer.from(read('from-rust.b64').trim(), 'base64'));
    expect(fragmentJson(fromRust)).toEqual(expected);
    // 서버가 만든 문서를 편집기 스키마로 다시 읽어도 같은 문서다.
    const reread: unknown = schema.nodeFromJSON(fragmentJson(fromRust)).toJSON();
    expect(reread).toEqual(stored);
  });
});
