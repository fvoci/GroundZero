// HWP·HWPX 미리보기 작업자(D-76). rhwp WASM을 화면 밖 스레드에서 불러 문서를 해석하고 쪽마다 SVG를 만든다.
// 큰 파일·손상 파일로 해석이 오래 걸리거나 멈춰도 화면은 멈추지 않고, 화면 쪽이 시간 한도나 취소로 이 작업자를
// 끝낸다. WASM은 처음 미리볼 때만 받는다(입력 해시 폴더라 브라우저 캐시에 오래 남는다).
import type { HwpReply, HwpRequest } from './hwp';

interface HwpDoc {
  pageCount(): number;
  renderPageSvg(index: number): string;
  free(): void;
}

interface RhwpModule {
  default: (input: { module_or_path: string }) => Promise<unknown>;
  HwpDocument: new (bytes: Uint8Array) => HwpDoc;
}

function isModule(m: unknown): m is RhwpModule {
  return (
    typeof m === 'object' &&
    m !== null &&
    'default' in m &&
    typeof m.default === 'function' &&
    'HwpDocument' in m &&
    typeof m.HwpDocument === 'function'
  );
}

function isRequest(data: unknown): data is HwpRequest {
  return (
    typeof data === 'object' &&
    data !== null &&
    'base' in data &&
    typeof data.base === 'string' &&
    'bytes' in data &&
    data.bytes instanceof ArrayBuffer &&
    'maxPages' in data &&
    typeof data.maxPages === 'number'
  );
}

function reply(message: HwpReply): void {
  self.postMessage(message);
}

let loaded: Promise<RhwpModule> | undefined;

function load(base: string): Promise<RhwpModule> {
  loaded ??= import(/* @vite-ignore */ `${base}rhwp.js`).then(async (m: unknown) => {
    if (!isModule(m)) throw new Error('unexpected rhwp module');
    await m.default({ module_or_path: `${base}rhwp_bg.wasm` });
    return m;
  });
  return loaded;
}

self.addEventListener('message', (event: MessageEvent<unknown>) => {
  const request = event.data;
  if (!isRequest(request)) return;
  void (async () => {
    let doc: HwpDoc | undefined;
    try {
      const rhwp = await load(request.base);
      doc = new rhwp.HwpDocument(new Uint8Array(request.bytes));
      const total = doc.pageCount();
      const pages = Math.min(total, request.maxPages);
      for (let index = 0; index < pages; index += 1) {
        reply({ type: 'page', index, svg: doc.renderPageSvg(index) });
      }
      reply({ type: 'done', rendered: pages, total });
    } catch {
      // 손상·지원하지 않는 파일. 내용은 알리지 않고 실패만 알린다.
      reply({ type: 'failed' });
    } finally {
      doc?.free();
    }
  })();
});
