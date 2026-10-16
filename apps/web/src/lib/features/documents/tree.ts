// 문서 트리 계산 (D-27). 서버 목록 순서를 지키며 부모별로 묶고, 펼친 항목만 깊이 우선으로 편다.
// 부모가 목록에 없으면(볼 수 없는 부모) 최상위로 둔다. 순서 바꾸기는 서버 API의 (부모, 앞 형제)로 바꾼다.

export interface TreeDoc {
  id: string;
  parent_id: string | null;
}

export interface TreeRow<T extends TreeDoc> {
  doc: T;
  depth: number;
  hasChildren: boolean;
  /** 같은 부모 아래 형제 수와 그 안의 순서(1부터). aria-setsize·aria-posinset에 쓴다. */
  setSize: number;
  position: number;
}

/** 부모 id(최상위는 null) → 자식 목록. 목록에 없는 부모는 최상위로 본다. */
export function childrenMap<T extends TreeDoc>(docs: readonly T[]): Map<string | null, T[]> {
  const ids = new Set(docs.map((d) => d.id));
  const map = new Map<string | null, T[]>();
  for (const doc of docs) {
    const parent = doc.parent_id !== null && ids.has(doc.parent_id) ? doc.parent_id : null;
    const list = map.get(parent) ?? [];
    list.push(doc);
    map.set(parent, list);
  }
  return map;
}

export function visibleRows<T extends TreeDoc>(
  docs: readonly T[],
  collapsed: ReadonlySet<string>,
): TreeRow<T>[] {
  const map = childrenMap(docs);
  const rows: TreeRow<T>[] = [];
  const walk = (parent: string | null, depth: number): void => {
    const list = map.get(parent) ?? [];
    list.forEach((doc, index) => {
      const hasChildren = (map.get(doc.id)?.length ?? 0) > 0;
      rows.push({ doc, depth, hasChildren, setSize: list.length, position: index + 1 });
      if (hasChildren && !collapsed.has(doc.id)) walk(doc.id, depth + 1);
    });
  };
  walk(null, 1);
  return rows;
}

/** 서버에 보낼 이동: 새 부모와, 그 아래에서 바로 뒤에 올 형제(없으면 맨 끝). */
export interface TreeMove {
  parentId: string | null;
  beforeId: string | null;
}

export type TreeAction = 'up' | 'down' | 'indent' | 'outdent';

/** 동작에 맞는 이동. 할 수 없으면(맨 위에서 위로 등) undefined. */
export function planMove(
  docs: readonly TreeDoc[],
  id: string,
  action: TreeAction,
): TreeMove | undefined {
  const map = childrenMap(docs);
  const parentOf = (docId: string): string | null => {
    for (const [parent, list] of map) if (list.some((d) => d.id === docId)) return parent;
    return null;
  };
  const parent = parentOf(id);
  const siblings = map.get(parent) ?? [];
  const index = siblings.findIndex((d) => d.id === id);
  if (index < 0) return undefined;
  switch (action) {
    case 'up': {
      const previous = siblings[index - 1];
      return previous === undefined ? undefined : { parentId: parent, beforeId: previous.id };
    }
    case 'down': {
      if (index >= siblings.length - 1) return undefined;
      return { parentId: parent, beforeId: siblings[index + 2]?.id ?? null };
    }
    case 'indent': {
      const previous = siblings[index - 1];
      return previous === undefined ? undefined : { parentId: previous.id, beforeId: null };
    }
    case 'outdent': {
      if (parent === null) return undefined;
      const grandparent = parentOf(parent);
      const uncles = map.get(grandparent) ?? [];
      const at = uncles.findIndex((d) => d.id === parent);
      return { parentId: grandparent, beforeId: uncles[at + 1]?.id ?? null };
    }
  }
}
