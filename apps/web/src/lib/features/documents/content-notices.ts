// 문서 내용 규칙(paste-guard)에 걸린 것을 사용자에게 알리는 문구. 원인과 고칠 방법을 함께 적는다.
import { m } from '$lib/paraglide/messages.js';

import type { ContentAdjustments, ContentIssues } from './paste-guard';

const example = (list: readonly string[]): string => {
  const first = list[0] ?? '';
  return first.length > 60 ? `${first.slice(0, 57)}…` : first;
};

/** 붙여넣기·끌어 놓기에서 바꾼 것. 바꾼 것이 없으면 빈 글. */
export function describeAdjustments(stats: ContentAdjustments): string {
  const parts: string[] = [];
  if (stats.imagesLinked > 0)
    parts.push(m.documents_paste_images_linked({ count: stats.imagesLinked }));
  if (stats.imagesRemoved > 0)
    parts.push(m.documents_paste_images_removed({ count: stats.imagesRemoved }));
  if (stats.linksUnlinked > 0)
    parts.push(m.documents_paste_links_unlinked({ count: stats.linksUnlinked }));
  return parts.join(' ');
}

/** 저장 전 확인에 걸린 것(저장하지 않는 이유와 고칠 방법). 걸린 것이 없으면 undefined. */
export function describeSaveBlock(issues: ContentIssues): string | undefined {
  if (issues.images.length > 0)
    return m.documents_save_blocked_images({
      count: issues.images.length,
      example: example(issues.images),
    });
  if (issues.links.length > 0)
    return m.documents_save_blocked_links({
      count: issues.links.length,
      example: example(issues.links),
    });
  return undefined;
}

/** Markdown 원문에서 저장할 수 없는 이미지. 없으면 undefined. */
export function describeMarkdownIssues(issues: ContentIssues): string | undefined {
  if (issues.images.length > 0)
    return m.documents_markdown_blocked_images({
      count: issues.images.length,
      example: example(issues.images),
    });
  if (issues.links.length > 0)
    return m.documents_markdown_blocked_links({
      count: issues.links.length,
      example: example(issues.links),
    });
  return undefined;
}
