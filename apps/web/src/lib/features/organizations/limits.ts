import type { OrganizationLimits } from '@fvoci/api-client';

export const LIMIT_KEYS = [
  'max_file_bytes',
  'max_storage_bytes',
  'max_concurrent_uploads',
  'activity_retention_days',
  'audit_retention_days',
] as const;
export type LimitKey = (typeof LIMIT_KEYS)[number];
export interface LimitInput {
  unset: boolean;
  amount: string;
  unit: 'MB' | 'GB';
}
export type LimitInputs = Record<LimitKey, LimitInput>;

export function isByteLimit(key: LimitKey): boolean {
  return key === 'max_file_bytes' || key === 'max_storage_bytes';
}
export function isRetention(key: LimitKey): boolean {
  return key === 'activity_retention_days' || key === 'audit_retention_days';
}
export const bytesPerUnit = (unit: 'MB' | 'GB'): number => (unit === 'GB' ? 1024 ** 3 : 1024 ** 2);

function input(value: number | null | undefined, key: LimitKey): LimitInput {
  const unit =
    value !== undefined && value !== null && value >= 1024 ** 3 && value % 1024 ** 3 === 0
      ? 'GB'
      : 'MB';
  return {
    unset: value === undefined || value === null,
    amount:
      value === undefined || value === null
        ? ''
        : String(isByteLimit(key) ? value / bytesPerUnit(unit) : value),
    unit,
  };
}
export function limitInputs(limits: OrganizationLimits): LimitInputs {
  return {
    max_file_bytes: input(limits.max_file_bytes, 'max_file_bytes'),
    max_storage_bytes: input(limits.max_storage_bytes, 'max_storage_bytes'),
    max_concurrent_uploads: input(limits.max_concurrent_uploads, 'max_concurrent_uploads'),
    activity_retention_days: input(limits.activity_retention_days, 'activity_retention_days'),
    audit_retention_days: input(limits.audit_retention_days, 'audit_retention_days'),
  };
}
export function limitValue(key: LimitKey, value: LimitInput): number | null | undefined {
  if (value.unset) return null;
  // Small existing byte values can render in exponent notation when expressed in MB.
  if (!(isByteLimit(key) ? /^\d+(?:\.\d+)?(?:e[+-]?\d+)?$/i : /^\d+$/).test(value.amount.trim()))
    return undefined;
  const parsed = Number(value.amount) * (isByteLimit(key) ? bytesPerUnit(value.unit) : 1);
  if (
    !Number.isSafeInteger(parsed) ||
    parsed < (key === 'max_storage_bytes' ? 0 : 1) ||
    (isRetention(key) && parsed > 3650) ||
    (key === 'max_concurrent_uploads' && parsed > 2147483647)
  )
    return undefined;
  return parsed;
}
