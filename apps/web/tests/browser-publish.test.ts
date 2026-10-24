// 브라우저 시험의 실패 증거 공개 검사(e2e/publish.ts). 합성 민감 값으로 공개·거절을 확인한다.
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

import JSZip from 'jszip';

import { MARKER, publish } from '../e2e/publish';

const SENTINEL = 'fvoci-sentinel-0123456789abcdef';
// URL 예약 문자가 있어 인코딩하면 원문이 남지 않는 값. 인코딩된 형태만으로 걸러야 한다.
const RESERVED = 'fvoci/reserved?&=#0123456789';

async function trace(entries: Record<string, string>): Promise<Uint8Array> {
  const zip = new JSZip();
  for (const [name, text] of Object.entries(entries)) zip.file(name, text);
  return zip.generateAsync({ type: 'uint8array' });
}

// 이 시험이 만든 임시 폴더만 지운다.
const created: string[] = [];
afterEach(() => {
  for (const dir of created.splice(0)) rmSync(dir, { recursive: true, force: true });
});

function setup(): { raw: string; out: string } {
  const dir = mkdtempSync(path.join(tmpdir(), 'fvoci-publish-'));
  created.push(dir);
  const raw = path.join(dir, 'raw');
  mkdirSync(path.join(raw, 'traces'), { recursive: true });
  mkdirSync(path.join(raw, 'test-results', 'a'), { recursive: true });
  const out = path.join(dir, 'artifacts');
  // 지난 실행의 표시가 남아 있어도 이번 결과로 올라가면 안 된다.
  mkdirSync(out);
  writeFileSync(path.join(out, MARKER), 'stale\n');
  return { raw, out };
}

describe('browser evidence publishing', () => {
  it('drops network records from traces, checks the rest, and writes the marker last', async () => {
    const { raw, out } = setup();
    writeFileSync(
      path.join(raw, 'traces', 't.zip'),
      await trace({
        'trace.trace': '{"type":"action"}',
        'trace.network': `{"headers":[{"name":"cookie","value":"x=${SENTINEL}"}]}`,
      }),
    );
    writeFileSync(path.join(raw, 'test-results', 'a', 'shot.png'), 'png');
    writeFileSync(path.join(raw, 'test-results', 'a', 'trace-raw.zip'), SENTINEL);
    writeFileSync(path.join(raw, 'sensitive.txt'), `${SENTINEL}\n`);
    writeFileSync(path.join(raw, 'server.log'), '{"event":"server.listening"}\n');

    const result = await publish(raw, out, [SENTINEL]);

    expect(result.refused).toEqual([]);
    expect(result.published.sort()).toEqual(
      [
        path.join('test-results', 'a', 'shot.png'),
        path.join('traces', 't.zip'),
        'server.log',
      ].sort(),
    );
    const published = await JSZip.loadAsync(readFileSync(path.join(out, 'traces', 't.zip')));
    expect(Object.keys(published.files)).toEqual(['trace.trace']);
    expect(existsSync(path.join(out, 'sensitive.txt'))).toBe(false);
    expect(readFileSync(path.join(out, MARKER), 'utf8')).toContain('3 files checked');
  });

  it.each([
    ['a sensitive value in a trace action', { 'trace.trace': `{"params":"${SENTINEL}"}` }],
    ['an encoded sensitive value', { 'trace.trace': `?next=${encodeURIComponent(RESERVED)}` }],
    ['an auth cookie name', { 'resources/a.html': 'Set-Cookie: fvoci_session=1' }],
  ])('refuses to publish anything when a trace has %s', async (_, entries) => {
    const { raw, out } = setup();
    writeFileSync(path.join(raw, 'traces', 't.zip'), await trace(entries));
    writeFileSync(path.join(raw, 'server.log'), 'ok\n');

    const result = await publish(raw, out, [SENTINEL, RESERVED]);

    expect(result.published).toEqual([]);
    expect(result.refused.map((r) => r.file)).toEqual([path.join('traces', 't.zip')]);
    const reported = JSON.stringify(result.refused);
    for (const value of [SENTINEL, RESERVED, encodeURIComponent(RESERVED)]) {
      expect(reported).not.toContain(value);
    }
    expect(existsSync(out)).toBe(false);
  });

  it('refuses a server log with a sensitive value', async () => {
    const { raw, out } = setup();
    writeFileSync(path.join(raw, 'server.log'), `token ${SENTINEL}\n`);

    const result = await publish(raw, out, [SENTINEL]);

    expect(result.refused).toEqual([{ file: 'server.log', reason: 'sensitive value #1' }]);
    expect(existsSync(path.join(out, MARKER))).toBe(false);
  });
});
