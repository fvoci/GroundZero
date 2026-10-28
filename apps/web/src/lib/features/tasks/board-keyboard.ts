import type { BoardAction } from './board';
export function handleCardKey(
  event: KeyboardEvent,
  id: string,
  canWrite: boolean,
  move: (id: string, action: BoardAction) => Promise<void>,
): void {
  if (!event.altKey || !canWrite) return;
  const map: Partial<Record<string, BoardAction>> = {
    ArrowUp: 'up',
    ArrowDown: 'down',
    ArrowLeft: 'left',
    ArrowRight: 'right',
  };
  const action = map[event.key];
  if (action === undefined) return;
  event.preventDefault();
  void move(id, action);
}
