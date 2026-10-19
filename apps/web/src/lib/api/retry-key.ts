// 만들기 요청의 재시도 키. 응답을 잃고 같은 내용으로 다시 보내면 같은 request_id(서버가 처음 것을 돌려줘 중복이
// 생기지 않음), 내용을 고쳐 보내면 새 request_id(서버가 처음 것을 돌려주면 고친 내용이 조용히 사라지므로).
// 성공하면 `done()`으로 비운다. 문서 저장의 `attempt`(내용 스냅샷 키)와 같은 규칙이다.
import { newRequestId } from '$lib/api/client';
import { canonicalJson } from '$lib/features/documents/drafts';

export class RetryKey {
  #pending: { id: string; snapshot: string } | undefined;

  /** 이 내용으로 보낼 request_id. 지난번과 내용이 같으면 같은 id다. */
  for(payload: unknown): string {
    const snapshot = canonicalJson(payload);
    if (this.#pending?.snapshot !== snapshot) this.#pending = { id: newRequestId(), snapshot };
    return this.#pending.id;
  }

  /** 성공했거나 대상이 바뀌었다. 다음 요청은 새 id다. */
  done(): void {
    this.#pending = undefined;
  }
}
