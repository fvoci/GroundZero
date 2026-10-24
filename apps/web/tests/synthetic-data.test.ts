// Storybook과 개발 seed가 함께 쓰는 합성 데이터(deploy/postgres/dev-ui-data.json)의 약속. 사람 이름은 개발
// Keycloak 테스트 사용자와 같아야 하고(로그인하면 이 이름이 보인다), 문서 부모·댓글 작성자는 데이터 안에 있어야 한다.
import { describe, expect, it } from 'vitest';

import { ITEM_STATUSES, MODULE_KINDS } from '$lib/api/client';
import { normalizeKey } from '$lib/features/modules/kinds';

import data from '../../../deploy/postgres/dev-ui-data.json';
import realm from '../../../deploy/keycloak/realm-test.json';

describe('shared synthetic data', () => {
  it('names people like the dev Keycloak test users', () => {
    const realmNames = realm.users.map((u) => `${u.firstName} ${u.lastName}`);
    expect(data.people.map((p) => p.name)).toEqual(realmNames);
  });

  it('refers only to earlier documents and known people', () => {
    const seen = new Set<string>();
    for (const doc of data.documents) {
      if (doc.parent !== null) expect(seen.has(doc.parent)).toBe(true);
      seen.add(doc.key);
    }
    expect(seen.has(data.comments.document)).toBe(true);
    const people = new Set(data.people.map((p) => p.key));
    const authors = data.comments.threads.flatMap((t) => [
      t.author,
      ...t.replies.map((r) => r.author),
    ]);
    const assignees = data.tasks.flatMap((t) => t.assignees);
    const items = [...data.project_items, ...data.modules.flatMap((mod) => mod.items)];
    const members = items.flatMap((it) => it.members);
    for (const key of [...authors, ...assignees, ...members]) expect(people.has(key)).toBe(true);
  });

  // seed는 app_create_module·app_set_item_profile로 넣는다. 서버가 거절할 값이면 `cargo xtask dev`가 멈춘다.
  it('has one module per kind with keys and items the server accepts', () => {
    const kinds = new Set(['project', ...data.modules.map((mod) => mod.kind)]);
    expect([...kinds].sort()).toEqual([...MODULE_KINDS].sort());
    const keys = [data.module.key, ...data.modules.map((mod) => mod.key)];
    for (const key of keys) expect(normalizeKey(key)).toBe(key);
    expect(new Set(keys).size).toBe(keys.length);
    for (const it of [...data.project_items, ...data.modules.flatMap((mod) => mod.items)]) {
      expect(ITEM_STATUSES).toContain(it.status);
      // 날짜 없는 항목도 있다(JSON 추론 타입은 항목마다 달라 넓혀 받는다).
      const { start, end }: { start: number | null; end: number | null } = it;
      if (start !== null && end !== null) expect(start).toBeLessThanOrEqual(end);
    }
  });

  it('names some modules with the hard-to-display samples', () => {
    const names = data.modules.map((mod) => mod.name);
    for (const sample of [data.samples.long_ko, data.samples.no_space, data.samples.emoji])
      expect(names).toContain(sample);
  });
});
