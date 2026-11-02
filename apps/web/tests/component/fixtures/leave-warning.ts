/** Whether closing the window right now would ask about unsaved input (D-27). */
export function unloadBlocked(): boolean {
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  return unload.defaultPrevented;
}
