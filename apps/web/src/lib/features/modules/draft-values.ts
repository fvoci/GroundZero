/** Validate stored input without treating browser storage as trusted application state. */
export function object(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function strings(value: unknown): value is Record<string, string> {
  return object(value) && Object.values(value).every((entry) => typeof entry === 'string');
}

export function stringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((entry: unknown) => typeof entry === 'string');
}

export function retry(value: unknown): value is { requestId: string; signature: string } | null {
  return (
    value === null ||
    (object(value) && typeof value.requestId === 'string' && typeof value.signature === 'string')
  );
}
