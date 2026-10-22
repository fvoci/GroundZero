// 리비전을 남긴 사람 표시(D-95). 실시간 편집에서 고친 사람이 쓸 수 없어 다른 사람(옮긴 사람·실시간을 끈 관리자·연결된
// 다른 작성자)이 대신 저장한 리비전은 실제로 고친 사람과 대신 저장한 사람을 함께 보인다. 대신 저장한 사람이 내용을 쓴
// 것처럼 보이지 않게 한다.
import type { RevisionSummaryView } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';

/** 대신 저장한 리비전이면 그 표시, 보통 리비전이면 `undefined`. 이름을 볼 수 없는 사람은 "알 수 없음"이다. */
export function standInLabel(rev: RevisionSummaryView): string | undefined {
  const names = rev.edited_by_names;
  if (names === null) return undefined;
  const author = rev.author_name ?? m.documents_unknown_author();
  if (names.length === 0) return m.documents_saved_for_unknown({ author });
  const editors = names.map((name) => name ?? m.documents_unknown_author()).join(', ');
  return m.documents_saved_for_editors({ editors, author });
}
