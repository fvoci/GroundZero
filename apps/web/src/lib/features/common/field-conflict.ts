export type FieldSide = 'mine' | 'server';
export interface ConflictField<T> {
  key: string;
  label: string;
  /** A group (such as a period) is compared and copied as one value. */
  same(a: T, b: T): boolean;
  text(value: T): string;
  copy(target: T, source: T): void;
}
export interface FieldDifference<T> {
  field: ConflictField<T>;
  kind: 'both' | 'mine' | 'server';
}

export function fieldDifferences<T>(
  fields: readonly ConflictField<T>[],
  base: T,
  mine: T,
  server: T,
): FieldDifference<T>[] {
  const rows = fields
    .filter((f) => !f.same(mine, server))
    .map((field): FieldDifference<T> => ({
      field,
      kind: field.same(base, mine) ? 'server' : field.same(base, server) ? 'mine' : 'both',
    }));
  return [...rows.filter((r) => r.kind === 'both'), ...rows.filter((r) => r.kind !== 'both')];
}

export function initialFieldChoices<T>(
  rows: readonly FieldDifference<T>[],
): Record<string, FieldSide> {
  return Object.fromEntries(
    rows.flatMap((r) => (r.kind === 'both' ? [] : [[r.field.key, r.kind]])),
  );
}

export function selectedFields<T>(
  fields: readonly ConflictField<T>[],
  mine: T,
  server: T,
  choices: Readonly<Record<string, FieldSide>>,
): T {
  const result = structuredClone(server);
  for (const field of fields) if (choices[field.key] === 'mine') field.copy(result, mine);
  return result;
}
