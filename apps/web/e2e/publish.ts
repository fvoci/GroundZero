// 브라우저 시험의 실패 증거를 공개 폴더로 옮긴다(`cargo xtask browser`가 시험 뒤에 부른다).
//
// 원시 출력(FVOCI_BROWSER_RAW)에는 세션 쿠키·인증 흐름이 남을 수 있으므로 그대로 올리지 않는다. 정해진 종류만 옮긴다.
// - 실패 화면(test-results/**/*.png)과 실패 맥락(test-results/**/*.md)
// - trace(traces/*.zip). 요청·응답 머리글과 쿠키가 담기는 네트워크 기록(*.network)은 뺀다.
// - 서버 로그(server.log). 서버는 머리글·본문·쿼리를 로그에 쓰지 않는다(crates/server/src/logging.rs).
// 옮길 파일은 모두 압축을 푼 내용까지 테스트 계정 비밀번호, 시험 중 받은 쿠키 값(sensitive.txt), 인증 쿠키·머리글
// 이름이 있는지 검사한다. 하나라도 걸리면 공개 폴더를 지우고 실패한다. 모두 통과하면 마지막에 SAFE_TO_UPLOAD를 쓴다.
// CI는 그 표시가 있을 때만 공개 폴더를 올린다.
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import JSZip from 'jszip';

export const MARKER = 'SAFE_TO_UPLOAD';

/** 값을 몰라도 들어 있으면 안 되는 인증 쿠키·머리글 이름(대소문자 무시). */
const FORBIDDEN_NAMES = [
  'set-cookie',
  'authorization:',
  'fvoci_session',
  'fvoci_login',
  'keycloak_identity',
  'keycloak_session',
  'auth_session_id',
  'kc_restart',
  'id_token_hint',
  'access_token',
  'refresh_token',
  'code_verifier',
];

/** trace에서 빼는 항목: 요청·응답 머리글(쿠키)과 본문을 담는 네트워크 기록. */
const DROPPED_TRACE_ENTRY = /(^|\/)[^/]*\.network$/;

export interface Refusal {
  file: string;
  reason: string;
}

export interface PublishResult {
  published: string[];
  refused: Refusal[];
}

function walk(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((e) => e.isFile())
    .map((e) => path.join(e.parentPath, e.name));
}

/** 걸린 값의 종류만 돌려준다(값 자체는 출력하지 않는다). */
export function findLeak(bytes: Uint8Array, secrets: readonly string[]): string | undefined {
  const text = Buffer.from(bytes).toString('latin1');
  const lower = text.toLowerCase();
  for (const name of FORBIDDEN_NAMES) {
    if (lower.includes(name)) return `auth name ${name}`;
  }
  const utf8 = Buffer.from(bytes);
  for (const [i, secret] of secrets.entries()) {
    for (const form of new Set([secret, encodeURIComponent(secret)])) {
      if (utf8.includes(Buffer.from(form, 'utf8'))) return `sensitive value #${String(i + 1)}`;
    }
  }
  return undefined;
}

/** 네트워크 기록을 뺀 trace를 만들고, 남은 항목을 모두 검사한다. */
async function sanitizeTrace(
  bytes: Uint8Array,
  secrets: readonly string[],
): Promise<{ zip: Uint8Array } | { reason: string }> {
  const source = await JSZip.loadAsync(bytes);
  const target = new JSZip();
  for (const entry of Object.values(source.files)) {
    if (entry.dir || DROPPED_TRACE_ENTRY.test(entry.name)) continue;
    const content = await entry.async('uint8array');
    const leak = findLeak(content, secrets);
    if (leak !== undefined) return { reason: `${entry.name}: ${leak}` };
    target.file(entry.name, content);
  }
  return { zip: await target.generateAsync({ type: 'uint8array', compression: 'DEFLATE' }) };
}

/** 테스트 계정 비밀번호(deploy/keycloak/realm-test.json)와 시험 중 기록한 민감 값. */
export function loadSecrets(raw: string, realmFile: string): string[] {
  const secrets = new Set<string>();
  const realm: unknown = JSON.parse(readFileSync(realmFile, 'utf8'));
  const collect = (v: unknown): void => {
    if (Array.isArray(v)) v.forEach(collect);
    else if (typeof v === 'object' && v !== null) {
      const rec = Object.fromEntries(Object.entries(v));
      if (rec.type === 'password' && typeof rec.value === 'string') secrets.add(rec.value);
      Object.values(rec).forEach(collect);
    }
  };
  collect(realm);
  const recorded = path.join(raw, 'sensitive.txt');
  if (existsSync(recorded)) {
    for (const line of readFileSync(recorded, 'utf8').split('\n')) {
      if (line.length >= 8) secrets.add(line);
    }
  }
  return [...secrets];
}

/** 원시 출력에서 공개 폴더로 옮긴다. 하나라도 거절되면 공개 폴더를 남기지 않는다. */
export async function publish(
  raw: string,
  out: string,
  secrets: readonly string[],
): Promise<PublishResult> {
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  const published: string[] = [];
  const refused: Refusal[] = [];
  const put = (name: string, bytes: Uint8Array): void => {
    const dest = path.join(out, name);
    mkdirSync(path.dirname(dest), { recursive: true });
    writeFileSync(dest, bytes);
    published.push(name);
  };

  const results = path.join(raw, 'test-results');
  for (const file of walk(results)) {
    if (!/\.(png|md)$/.test(file)) continue;
    const name = path.join('test-results', path.relative(results, file));
    const bytes = readFileSync(file);
    const leak = findLeak(bytes, secrets);
    if (leak === undefined) put(name, bytes);
    else refused.push({ file: name, reason: leak });
  }
  const traces = path.join(raw, 'traces');
  for (const file of walk(traces)) {
    if (!file.endsWith('.zip')) continue;
    const name = path.join('traces', path.relative(traces, file));
    const result = await sanitizeTrace(readFileSync(file), secrets);
    if ('zip' in result) put(name, result.zip);
    else refused.push({ file: name, reason: result.reason });
  }
  const log = path.join(raw, 'server.log');
  if (existsSync(log)) {
    const bytes = readFileSync(log);
    const leak = findLeak(bytes, secrets);
    if (leak === undefined) put('server.log', bytes);
    else refused.push({ file: 'server.log', reason: leak });
  }

  if (refused.length > 0) {
    rmSync(out, { recursive: true, force: true });
    return { published: [], refused };
  }
  const manifest = published
    .sort()
    .map((name) => {
      const sha = createHash('sha256')
        .update(readFileSync(path.join(out, name)))
        .digest('hex');
      return `${sha}  ${name}`;
    })
    .join('\n');
  // 표시는 마지막에 쓴다. 중간에 멈추면 표시가 없어 올라가지 않는다.
  writeFileSync(path.join(out, MARKER), `${manifest}\n${String(published.length)} files checked\n`);
  return { published, refused };
}

async function main(): Promise<number> {
  const [raw, out] = process.argv.slice(2);
  if (raw === undefined || out === undefined) {
    console.error('usage: node e2e/publish.ts <raw dir> <artifacts dir>');
    return 2;
  }
  const realm = path.resolve(import.meta.dirname, '../../../deploy/keycloak/realm-test.json');
  const { published, refused } = await publish(raw, out, loadSecrets(raw, realm));
  if (refused.length > 0) {
    for (const r of refused) console.error(`refused ${r.file}: ${r.reason}`);
    console.error(`published nothing; ${out} removed`);
    return 1;
  }
  console.log(`published ${String(published.length)} files to ${out} (${MARKER} written)`);
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exitCode = await main();
}
