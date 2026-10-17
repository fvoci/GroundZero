// 워크스페이스 구분 표시 (D-17). 표시 이름은 중복될 수 있으므로 내부 UUID에서 자동 배지 색을 정하고,
// 내가 속한 워크스페이스 중 이름이 같은 것이 있으면 UUID 끝 네 글자를 보조 정보로 붙인다.
// 새 전역 고유 이름이나 키를 요구하지 않는다. 링크·권한은 계속 내부 ID로 처리한다.

/** Tailwind가 찾을 수 있게 클래스를 글자 그대로 둔다. */
const BADGE_COLORS = [
  'bg-sky-500',
  'bg-emerald-500',
  'bg-amber-500',
  'bg-rose-500',
  'bg-violet-500',
  'bg-teal-500',
  'bg-orange-500',
  'bg-fuchsia-500',
] as const;

/** UUID에서 정한 배지 색(같은 워크스페이스는 늘 같은 색). */
export function badgeColor(id: string): string {
  let hash = 0;
  for (const ch of id) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return BADGE_COLORS[hash % BADGE_COLORS.length] ?? 'bg-sky-500';
}

/** UUID 끝 네 글자. UUIDv7 앞부분은 생성 시각이라 비슷하므로 무작위인 끝부분을 쓴다. */
export function shortId(id: string): string {
  return id.replace(/-/g, '').slice(-4);
}

export interface NamedWorkspace {
  id: string;
  name: string;
}

/** 이름 옆에 붙일 보조 정보. 같은 이름의 다른 워크스페이스가 없으면 없다. */
export function workspaceHint(ws: NamedWorkspace, all: readonly NamedWorkspace[]): string | null {
  const duplicated = all.some((other) => other.id !== ws.id && other.name === ws.name);
  return duplicated ? `#${shortId(ws.id)}` : null;
}

/** 목록·후보에 쓸 이름. 예: "연구실 #a3f2". */
export function workspaceLabel(ws: NamedWorkspace, all: readonly NamedWorkspace[]): string {
  const hint = workspaceHint(ws, all);
  return hint === null ? ws.name : `${ws.name} ${hint}`;
}
