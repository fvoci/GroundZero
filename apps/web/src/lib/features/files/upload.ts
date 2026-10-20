// 조각 업로드(D-06). 파일을 서버가 정한 조각 크기로 나눠 보내고, 끊기면 같은 요청 id로 다시 시작해 받은 조각을
// 건너뛴다(새로 고침 뒤에도 같은 파일을 다시 고르면 이어진다). 받은 조각은 이 브라우저가 보낸 해시와 지금 내용이
// 같을 때만 건너뛴다. 조각마다 SHA-256을 함께 보내 서버가 확인한다.
import type { FileTarget, UploadView } from '$lib/api/client';
import { api, ApiError, NetworkError } from '$lib/api/client';

export interface UploadProgress {
  sent: number;
  total: number;
}

const STORE = 'fvoci.uploads';

/** n번째 조각(1부터)의 [시작, 끝) 바이트. */
export function partRange(n: number, chunk: number, size: number): [number, number] {
  const start = (n - 1) * chunk;
  return [start, Math.min(size, start + chunk)];
}

/** 아직 보내지 않은 조각 번호. */
export function missingParts(total: number, received: readonly number[]): number[] {
  const have = new Set(received);
  const out: number[] = [];
  for (let n = 1; n <= total; n += 1) if (!have.has(n)) out.push(n);
  return out;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${String(bytes)} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit] ?? 'TB'}`;
}

async function sha256Hex(blob: Blob): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', await blob.arrayBuffer());
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
}

function readStore(): Record<string, string> {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(STORE) ?? '{}');
    if (typeof raw !== 'object' || raw === null) return {};
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(raw)) if (typeof v === 'string') out[k] = v;
    return out;
  } catch {
    return {};
  }
}

function writeStore(next: Record<string, string>): void {
  try {
    localStorage.setItem(STORE, JSON.stringify(next));
  } catch {
    // 저장할 수 없으면 이어 올리기만 못 한다.
  }
}

function remember(key: string): string {
  const store = readStore();
  const existing = store[key];
  if (existing !== undefined) return existing;
  const id = crypto.randomUUID();
  writeStore({ ...store, [key]: id });
  return id;
}

function forget(key: string): void {
  const store = readStore();
  if (key in store) {
    writeStore(Object.fromEntries(Object.entries(store).filter(([k]) => k !== key)));
  }
}

// 업로드마다 서버에 보낸 조각의 SHA-256(조각 번호 → 해시)과 그 업로드의 기한. 이어 올릴 때 서버가 받은 조각이 지금
// 파일의 그 부분과 같은지 확인한다. 이름·크기·수정 시각이 같아도 내용이 다를 수 있다. 기한이 지난 업로드는 이어 올릴
// 수 없으므로 그 해시는 다음에 적을 때 버린다(그만둔 업로드가 저장소에 쌓이지 않게).
const PARTS = 'fvoci.upload-parts';

interface SentParts {
  expires: string;
  hashes: Record<string, string>;
}

function readParts(): Record<string, SentParts> {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(PARTS) ?? '{}');
    if (typeof raw !== 'object' || raw === null) return {};
    const out: Record<string, SentParts> = {};
    const entries = (o: object): [string, unknown][] => Object.entries(o);
    for (const [id, entry] of entries(raw)) {
      if (typeof entry !== 'object' || entry === null) continue;
      if (!('expires' in entry) || typeof entry.expires !== 'string') continue;
      if (!('hashes' in entry) || typeof entry.hashes !== 'object' || entry.hashes === null)
        continue;
      const hashes: Record<string, string> = {};
      for (const [n, h] of entries(entry.hashes)) if (typeof h === 'string') hashes[n] = h;
      out[id] = { expires: entry.expires, hashes };
    }
    return out;
  } catch {
    return {};
  }
}

function writeParts(next: Record<string, SentParts>): void {
  try {
    localStorage.setItem(PARTS, JSON.stringify(next));
  } catch {
    // 적지 못한 조각은 이어 올릴 때 다시 보낸다.
  }
}

function recordPart(up: UploadView, n: number, hash: string): void {
  const now = Date.now();
  const live = Object.entries(readParts()).filter(([, e]) => Date.parse(e.expires) > now);
  const all = Object.fromEntries(live);
  writeParts({
    ...all,
    [up.id]: { expires: up.expires_at, hashes: { ...all[up.id]?.hashes, [String(n)]: hash } },
  });
}

function forgetParts(uploadId: string): void {
  const all = readParts();
  if (uploadId in all) {
    writeParts(Object.fromEntries(Object.entries(all).filter(([k]) => k !== uploadId)));
  }
}

const wait = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** 끊긴 연결·서버 일시 오류는 몇 번 다시 보낸다. */
async function withRetry(send: () => Promise<void>, signal?: AbortSignal): Promise<void> {
  for (let attempt = 0; ; attempt += 1) {
    signal?.throwIfAborted();
    try {
      await send();
      return;
    } catch (e: unknown) {
      const retriable = e instanceof NetworkError || (e instanceof ApiError && e.status >= 500);
      if (!retriable || attempt >= 3) throw e;
      await wait(1000 * (attempt + 1));
    }
  }
}

/** 파일 하나를 올린다. `fileId`면 그 파일의 새 버전, 아니면 `target`에 새 파일. */
export async function uploadFile(
  file: File,
  opts: {
    target?: FileTarget;
    fileId?: string;
    note?: string;
    onProgress?: (p: UploadProgress) => void;
    signal?: AbortSignal;
  },
): Promise<UploadView> {
  const where =
    opts.fileId ?? (opts.target === undefined ? '' : `${opts.target.kind}:${opts.target.id}`);
  const key = `${where}|${file.name}|${String(file.size)}|${String(file.lastModified)}`;
  const start = (): Promise<UploadView> =>
    api.createUpload({
      requestId: remember(key),
      ...(opts.target === undefined ? {} : { target: opts.target }),
      ...(opts.fileId === undefined ? {} : { fileId: opts.fileId }),
      name: file.name,
      note: opts.note ?? '',
      contentType: file.type,
      size: file.size,
    });
  let up = await start();
  if (up.state === 'done') {
    forget(key);
    forgetParts(up.id);
    return up;
  }
  if (up.state === 'completing') {
    // 앞선 마치기가 끊겼다: 다시 올리지 않고 결과를 기다린다.
    const done = await finish(up.id);
    forget(key);
    forgetParts(up.id);
    return done;
  }
  if (up.state !== 'uploading' || Date.parse(up.expires_at) <= Date.now()) {
    // 기한이 지났거나 그만둔 업로드면 새로 시작한다.
    forget(key);
    forgetParts(up.id);
    up = await start();
  }
  const id = up.id;
  try {
    // 저장소가 조각을 합치지 못하면 서버가 받은 조각을 비우고 다시 받는다(한 번 더 보낸다).
    for (let round = 0; ; round += 1) {
      await sendParts(file, up, opts);
      try {
        const done = await finish(id);
        forget(key);
        forgetParts(id);
        return done;
      } catch (e: unknown) {
        if (!(e instanceof ApiError) || e.status !== 409 || round >= 1) throw e;
        up = await api.upload(id);
        if (up.state !== 'uploading') throw e;
      }
    }
  } catch (e: unknown) {
    if (opts.signal?.aborted === true) {
      forget(key);
      forgetParts(id);
      await api.abortUpload(id).catch(() => undefined);
    }
    throw e;
  }
}

async function sendParts(
  file: File,
  up: UploadView,
  opts: { onProgress?: (p: UploadProgress) => void; signal?: AbortSignal },
): Promise<void> {
  // 서버가 받은 조각은 이 브라우저가 보낸 해시와 지금 파일의 그 부분이 같을 때만 건너뛴다. 나머지는 다시 보낸다
  // (같은 번호를 다시 보내면 서버가 바꾼다).
  const known = readParts()[up.id]?.hashes ?? {};
  const trusted: number[] = [];
  for (const n of up.received) {
    opts.signal?.throwIfAborted();
    const want = known[String(n)];
    if (want === undefined) continue;
    const [from, to] = partRange(n, up.chunk_bytes, file.size);
    if ((await sha256Hex(file.slice(from, to))) === want) trusted.push(n);
  }
  // 진행률은 받은 바이트의 합이다(다시 보낸 조각의 끝 위치로 보면 앞 조각만 바뀐 이어 올리기가 끝나도 중간에 머문다).
  const length = (n: number): number => {
    const [from, to] = partRange(n, up.chunk_bytes, file.size);
    return to - from;
  };
  let sent = trusted.reduce((sum, n) => sum + length(n), 0);
  opts.onProgress?.({ sent, total: file.size });
  for (const n of missingParts(up.parts_total, trusted)) {
    opts.signal?.throwIfAborted();
    const [from, to] = partRange(n, up.chunk_bytes, file.size);
    const piece = file.slice(from, to);
    const hash = await sha256Hex(piece);
    await withRetry(() => api.putUploadPart(up.id, n, piece, hash), opts.signal);
    recordPart(up, n, hash);
    sent += to - from;
    opts.onProgress?.({ sent: Math.min(file.size, sent), total: file.size });
  }
}

/** 마치기. 다른 요청이 마치는 중이면(409) 잠시 기다렸다가 다시 묻는다. */
async function finish(id: string): Promise<UploadView> {
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await api.completeUpload(id);
    } catch (e: unknown) {
      if (e instanceof ApiError && e.status === 409 && attempt < 20) {
        const now = await api.upload(id);
        if (now.state === 'done') return now;
        if (now.state === 'completing') {
          await wait(3000);
          continue;
        }
      }
      throw e;
    }
  }
}
