// 조각 업로드(D-06). 파일을 서버가 정한 조각 크기로 나눠 보내고, 끊기면 같은 요청 id로 다시 시작해 받은 조각을
// 건너뛴다(새로 고침 뒤에도 같은 파일을 다시 고르면 이어진다). 조각마다 SHA-256을 함께 보내 서버가 확인한다.
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
    return up;
  }
  if (up.state !== 'uploading') {
    // 기한이 지났거나 그만둔 업로드면 새로 시작한다.
    forget(key);
    up = await start();
  }
  const total = up.parts_total;
  let sent = up.received.length;
  opts.onProgress?.({ sent: Math.min(file.size, sent * up.chunk_bytes), total: file.size });
  for (const n of missingParts(total, up.received)) {
    const [from, to] = partRange(n, up.chunk_bytes, file.size);
    const piece = file.slice(from, to);
    const hash = await sha256Hex(piece);
    const id = up.id;
    await withRetry(() => api.putUploadPart(id, n, piece, hash), opts.signal);
    sent += 1;
    opts.onProgress?.({ sent: Math.min(file.size, to), total: file.size });
  }
  const done = await api.completeUpload(up.id);
  forget(key);
  return done;
}
