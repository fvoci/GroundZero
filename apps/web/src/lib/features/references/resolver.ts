// 참조 표시 이름 해석. 본문에는 종류와 id만 있으므로 화면에 보일 때 서버에 묻는다(D-18).
// 같은 틱에 요청된 참조를 한 번에 묻고, 페이지가 살아 있는 동안 결과를 기억한다.
import type { ReferenceKind, ResolvedReferenceView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';

interface Pending {
  kind: ReferenceKind;
  id: string;
  resolve: (r: ResolvedReferenceView) => void;
}

const cache = new Map<string, Promise<ResolvedReferenceView>>();
let queue: Pending[] = [];
let scheduled = false;

/** 서버가 답하지 못할 때(로그인 전 등)도 존재 여부를 드러내지 않는 같은 모양. */
export function hiddenReference(kind: ReferenceKind, id: string): ResolvedReferenceView {
  return {
    kind,
    id,
    visible: false,
    title: null,
    reference: null,
    workspace_id: null,
    item_id: null,
  };
}

/** 한 번에 묻는 최대 개수. 서버 상한(문서당 참조 수)보다 작게 둔다. */
const BATCH = 100;

async function flush(): Promise<void> {
  scheduled = false;
  const batch = queue;
  queue = [];
  for (let start = 0; start < batch.length; start += BATCH) {
    const part = batch.slice(start, start + BATCH);
    let results: ResolvedReferenceView[] = [];
    try {
      results = await api.resolveReferences(part.map(({ kind, id }) => ({ kind, id })));
    } catch {
      results = [];
    }
    part.forEach((p, i) => {
      p.resolve(results[i] ?? hiddenReference(p.kind, p.id));
    });
  }
}

export function resolveReference(kind: ReferenceKind, id: string): Promise<ResolvedReferenceView> {
  const key = `${kind}:${id}`;
  const known = cache.get(key);
  if (known !== undefined) return known;
  const promise = new Promise<ResolvedReferenceView>((resolve) => {
    queue.push({ kind, id, resolve });
  });
  cache.set(key, promise);
  if (!scheduled) {
    scheduled = true;
    queueMicrotask(() => void flush());
  }
  return promise;
}

/** 이름·키가 바뀐 뒤 다시 묻도록 기억을 비운다. */
export function forgetReferences(): void {
  cache.clear();
}

/**
 * 화면·내보내기 파일에 보일 글자. 볼 수 없는 대상은 제목 없이 같은 문구(화면 언어, D-81)로 보인다.
 */
export function referenceLabel(r: ResolvedReferenceView): string {
  if (!r.visible || r.title === null) return m.references_hidden();
  if (r.kind === 'user') return `@${r.title}`;
  return r.reference === null ? r.title : `${r.reference} ${r.title}`;
}
