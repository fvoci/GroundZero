import type { StatusCategory, TaskStatusDefinition } from '@fvoci/api-client';
export type Access = 'owner' | 'write' | 'read';
export type Json = unknown;

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function str(o: Record<string, unknown>, key: string): string {
  const v = o[key];
  if (typeof v !== 'string') throw new TypeError(`expected string at ${key}`);
  return v;
}

export function int(o: Record<string, unknown>, key: string): number {
  const v = o[key];
  if (typeof v !== 'number' || !Number.isSafeInteger(v))
    throw new TypeError(`expected integer at ${key}`);
  return v;
}

export function bool(o: Record<string, unknown>, key: string): boolean {
  const v = o[key];
  if (typeof v !== 'boolean') throw new TypeError(`expected boolean at ${key}`);
  return v;
}

export function rec(value: unknown, what: string): Record<string, unknown> {
  if (!isRecord(value)) throw new TypeError(`expected object for ${what}`);
  return value;
}

export function parseList<T>(value: Json, item: (v: Json) => T): T[] {
  if (!Array.isArray(value)) throw new TypeError('expected array');
  return value.map(item);
}

export function optStr(o: Record<string, unknown>, key: string): string | null {
  const v = o[key];
  if (v === null || v === undefined) return null;
  if (typeof v !== 'string') throw new TypeError(`expected string or null at ${key}`);
  return v;
}

export function strings(value: Json): string[] {
  return parseList(value, (s) => {
    if (typeof s !== 'string') throw new TypeError('expected string');
    return s;
  });
}

export function optInt(o: Record<string, unknown>, key: string): number | null {
  const v = o[key];
  if (v === null || v === undefined) return null;
  if (typeof v !== 'number' || !Number.isSafeInteger(v))
    throw new TypeError(`expected integer at ${key}`);
  return v;
}

export function access(value: string): Access {
  if (value === 'owner' || value === 'write' || value === 'read') return value;
  throw new TypeError('unknown access');
}

export function statusCategory(value: string): StatusCategory {
  if (value === 'todo' || value === 'doing' || value === 'done') return value;
  throw new TypeError(`unknown status category ${value}`);
}

export function parseTaskStatusDefinition(value: Json): TaskStatusDefinition {
  const o = rec(value, 'task status');
  const position = o.position;
  if (typeof position !== 'number' || !Number.isFinite(position))
    throw new TypeError('expected number at position');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    color: str(o, 'color'),
    position,
    category: statusCategory(str(o, 'category')),
    task_count: int(o, 'task_count'),
  };
}

export function none(): void {
  /* HTTP status is sufficient. */
}
