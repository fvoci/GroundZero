// Storybook과 개발 seed가 함께 쓰는 합성 데이터(deploy/postgres/dev-ui-data.json)의 약속. 사람 이름은 개발
// Keycloak 테스트 사용자와 같아야 하고(로그인하면 이 이름이 보인다), 문서 부모·댓글 작성자는 데이터 안에 있어야 한다.
import { describe, expect, it } from 'vitest';

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
    for (const key of [...authors, ...assignees]) expect(people.has(key)).toBe(true);
  });
});
