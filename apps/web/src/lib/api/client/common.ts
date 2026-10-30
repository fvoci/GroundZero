export function visibility(value: string): 'workspace' | 'private' {
  if (value === 'workspace' || value === 'private') return value;
  throw new TypeError('unknown visibility');
}

export const none = (): undefined => undefined;
