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

/** 옮기기 또는 그 뒤의 목록 다시 읽기가 실패했다. `moved`: 서버가 옮기기를 받았는지, `reloaded`: 목록을 다시 읽었는지. */
export class TreeMoveError extends Error {
  constructor(
    readonly reason: unknown,
    readonly moved: boolean,
    readonly reloaded: boolean,
  ) {
    super('document tree move');
  }
}

/**
 * 옮기기를 보낸 뒤 결과와 상관없이 목록을 다시 읽는다(권한 철회·동시 변경 뒤 최신 목록). 둘 중 하나라도 실패하면
 * 어느 쪽이 실패했는지 담아 던진다: 다시 읽기의 실패가 옮기기의 실패를 가리거나, 옮기기가 됐는데 실패처럼 보이지 않게.
 */
export async function moveThenReload(
  move: () => Promise<unknown>,
  reload: () => Promise<void>,
): Promise<void> {
  let failure: unknown;
  let moved = true;
  try {
    await move();
  } catch (e: unknown) {
    moved = false;
    failure = e;
  }
  let reloaded = true;
  try {
    await reload();
  } catch (e: unknown) {
    reloaded = false;
    if (moved) failure = e;
  }
  if (moved && reloaded) return;
  throw new TreeMoveError(failure, moved, reloaded);
}
