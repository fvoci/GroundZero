// HWP·HWPX 미리보기(D-76). rhwp WASM을 Web Worker에서 돌려 쪽마다 SVG를 받고, 화면은 SVG를 그림(<img>)으로만
// 보인다(SVG 안 스크립트·바깥 요청이 돌지 않는다). 파일 크기·처리 시간 한도와 취소가 있어 큰 파일·손상 파일이 화면을
// 멈추지 않는다. WASM 위치는 빌드가 쓴 `/rhwp/manifest.json`(입력 해시 폴더)에서 읽는다.

export interface HwpRequest {
  base: string;
  bytes: ArrayBuffer;
  maxPages: number;
}

export type HwpReply =
  | { type: 'page'; index: number; svg: string }
  | { type: 'done'; rendered: number; total: number }
  | { type: 'failed' };

/** 미리볼 수 있는 최대 크기. 더 크면 내려받아 연다. */
export const HWP_PREVIEW_MAX_BYTES = 20 * 1024 * 1024;
/** 처음 그리는 최대 쪽 수. */
export const HWP_MAX_PAGES = 50;
/** WASM을 받고 해석해 쪽을 다 그리기까지의 시간 한도. 넘으면 작업자를 끝낸다. */
export const HWP_TIME_LIMIT_MS = 30_000;

export function isHwp(contentType: string, name: string): boolean {
  const type = contentType.split(';')[0]?.trim().toLowerCase() ?? '';
  const ext = name.toLowerCase().split('.').pop() ?? '';
  return (
    ext === 'hwp' ||
    ext === 'hwpx' ||
    [
      'application/x-hwp',
      'application/haansofthwp',
      'application/vnd.hancom.hwp',
      'application/hwp+zip',
      'application/haansofthwpx',
      'application/vnd.hancom.hwpx',
    ].includes(type)
  );
}

/** 빌드가 둔 WASM 폴더(`/rhwp/<해시>/`). 없으면(설치되지 않음) undefined. */
export async function rhwpBase(): Promise<string | undefined> {
  const response = await fetch('/rhwp/manifest.json', { cache: 'no-cache' });
  if (!response.ok) return undefined;
  const manifest: unknown = await response.json();
  if (
    typeof manifest !== 'object' ||
    manifest === null ||
    !('dir' in manifest) ||
    typeof manifest.dir !== 'string' ||
    !/^[0-9a-f]{16}$/.test(manifest.dir)
  ) {
    return undefined;
  }
  return `/rhwp/${manifest.dir}/`;
}

function isReply(data: unknown): data is HwpReply {
  if (typeof data !== 'object' || data === null || !('type' in data)) return false;
  if (data.type === 'failed') return true;
  if (data.type === 'page')
    return (
      'index' in data &&
      typeof data.index === 'number' &&
      'svg' in data &&
      typeof data.svg === 'string'
    );
  if (data.type === 'done')
    return (
      'rendered' in data &&
      typeof data.rendered === 'number' &&
      'total' in data &&
      typeof data.total === 'number'
    );
  return false;
}

export type HwpOutcome =
  | { kind: 'done'; rendered: number; total: number }
  | { kind: 'failed' }
  | { kind: 'timeout' }
  | { kind: 'cancelled' };

export interface HwpRun {
  /** 끝나면(성공·실패·시간 초과·취소) 풀린다. */
  result: Promise<HwpOutcome>;
  cancel: () => void;
}

/** 작업자에서 쪽마다 SVG를 만든다. `onPage`는 그린 순서대로 불린다. */
export function renderHwp(
  base: string,
  bytes: ArrayBuffer,
  onPage: (index: number, svg: string) => void,
  limitMs = HWP_TIME_LIMIT_MS,
): HwpRun {
  const worker = new Worker(new URL('./hwp-worker.ts', import.meta.url), { type: 'module' });
  let settle: (outcome: HwpOutcome) => void = () => undefined;
  const result = new Promise<HwpOutcome>((resolve) => {
    settle = resolve;
  });
  let finished = false;
  const finish = (outcome: HwpOutcome): void => {
    if (finished) return;
    finished = true;
    clearTimeout(timer);
    worker.terminate();
    settle(outcome);
  };
  const timer = setTimeout(() => {
    finish({ kind: 'timeout' });
  }, limitMs);
  worker.addEventListener('message', (event: MessageEvent<unknown>) => {
    const data = event.data;
    if (finished || !isReply(data)) return;
    if (data.type === 'page') onPage(data.index, data.svg);
    else if (data.type === 'done')
      finish({ kind: 'done', rendered: data.rendered, total: data.total });
    else finish({ kind: 'failed' });
  });
  worker.addEventListener('error', () => {
    finish({ kind: 'failed' });
  });
  const request: HwpRequest = { base, bytes, maxPages: HWP_MAX_PAGES };
  worker.postMessage(request, [bytes]);
  return {
    result,
    cancel: () => {
      finish({ kind: 'cancelled' });
    },
  };
}
