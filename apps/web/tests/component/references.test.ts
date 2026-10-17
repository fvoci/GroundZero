import { describe, expect, it } from 'vitest';

import { parseResolved } from '$lib/api/client';
import { plainText } from '$lib/features/documents/drafts';
import { referenceHref } from '$lib/features/references/context';
import { hiddenReference, referenceLabel } from '$lib/features/references/resolver';
import { parseRoute } from '$lib/router.svelte';

const ws = '0198f1f2-0000-7000-8000-000000000002';
const item = '0198f1f2-0000-7000-8000-000000000004';
const doc = '0198f1f2-0000-7000-8000-000000000005';

describe('reference display', () => {
  it('labels visible targets and hides the rest the same way', () => {
    const seen = parseResolved({
      kind: 'item',
      id: item,
      visible: true,
      title: '배양 조건 비교',
      reference: 'EXP-1',
      workspace_id: ws,
      item_id: null,
    });
    expect(referenceLabel(seen)).toBe('EXP-1 배양 조건 비교');
    expect(referenceLabel(hiddenReference('item', item))).toBe('볼 수 없는 대상');
    expect(referenceLabel(hiddenReference('document', doc))).toBe('볼 수 없는 대상');
    expect(referenceLabel({ ...hiddenReference('user', item), visible: true, title: '영환' })).toBe(
      '@영환',
    );
  });

  it('links only to places I can open', () => {
    const member = (id: string): boolean => id === ws;
    const target = {
      ...hiddenReference('document', doc),
      visible: true,
      title: 't',
      workspace_id: ws,
    };
    expect(referenceHref({ ...target, item_id: item }, member)).toBe(`/w/${ws}/i/${item}/d/${doc}`);
    expect(referenceHref(target, () => false)).toBe(`/d/${doc}`);
    expect(referenceHref(hiddenReference('document', doc), member)).toBeNull();
    expect(referenceHref({ ...hiddenReference('user', doc), visible: true }, member)).toBeNull();
  });

  it('keeps references visible in the conflict comparison text', () => {
    const body = {
      type: 'doc',
      content: [
        {
          type: 'paragraph',
          content: [
            { type: 'text', text: '참고 ' },
            { type: 'reference', attrs: { kind: 'item', id: item } },
          ],
        },
      ],
    };
    expect(plainText(body)).toBe(`참고 @[item:${item}]`);
  });

  it('routes the notifications page', () => {
    expect(parseRoute('/notifications')).toEqual({ name: 'notifications' });
  });
});
