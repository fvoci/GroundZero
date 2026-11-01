/** Server permissions can change while the same component and its sibling inputs stay mounted. */
export function permissionState<T>(initial: T): T {
  const state = $state(initial);
  return state;
}
