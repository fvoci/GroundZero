import type { OrganizationLimits } from '@fvoci/api-client';

import { m } from '$lib/paraglide/messages.js';

export const LIMIT_FIELDS = [
  { key: 'max_file_bytes', bytes: true, min: 1, max: Number.MAX_SAFE_INTEGER },
  { key: 'max_storage_bytes', bytes: true, min: 0, max: Number.MAX_SAFE_INTEGER },
  { key: 'max_concurrent_uploads', bytes: false, min: 1, max: 2_147_483_647 },
  { key: 'activity_retention_days', bytes: false, min: 1, max: 3650 },
  { key: 'audit_retention_days', bytes: false, min: 1, max: 3650 },
] as const;

export type LimitKey = (typeof LIMIT_FIELDS)[number]['key'];
export interface LimitInput {
  mode: 'unset' | 'set';
  value: string;
  unit: 'MB' | 'GB';
}
export type LimitInputs = Record<LimitKey, LimitInput>;

export function limitLabel(key: LimitKey): string {
  switch (key) {
    case 'max_file_bytes':
      return m.organizations_max_file_bytes();
    case 'max_storage_bytes':
      return m.organizations_workspace_storage();
    case 'max_concurrent_uploads':
      return m.organizations_max_concurrent_uploads();
    case 'activity_retention_days':
      return m.organizations_activity_retention_days();
    case 'audit_retention_days':
      return m.organizations_audit_retention_days();
  }
}

/** Match the existing file-size display: MB = 1024² bytes, GB = 1024³ bytes. */
function byteUnit(value: number): 'MB' | 'GB' {
  return value >= 1024 ** 3 && value % 1024 ** 3 === 0 ? 'GB' : 'MB';
}

export function limitInputs(limits: OrganizationLimits): LimitInputs {
  const input = (key: LimitKey, bytes: boolean): LimitInput => {
    const value = limits[key] ?? null;
    const unit = bytes && value !== null ? byteUnit(value) : 'MB';
    return {
      mode: value === null ? 'unset' : 'set',
      value: value === null ? '' : String(bytes ? value / 1024 ** (unit === 'GB' ? 3 : 2) : value),
      unit,
    };
  };
  return {
    max_file_bytes: input('max_file_bytes', true),
    max_storage_bytes: input('max_storage_bytes', true),
    max_concurrent_uploads: input('max_concurrent_uploads', false),
    activity_retention_days: input('activity_retention_days', false),
    audit_retention_days: input('audit_retention_days', false),
  };
}

export function limitValue(input: LimitInput, bytes: boolean): number | null {
  if (input.mode === 'unset') return null;
  if (!/^\d+(?:\.\d+)?$/.test(input.value.trim())) return NaN;
  const value = Number(input.value);
  return bytes ? value * 1024 ** (input.unit === 'GB' ? 3 : 2) : value;
}

export function limitErrors(
  inputs: LimitInputs,
  organization: OrganizationLimits,
  submitted: boolean,
): Partial<Record<LimitKey, string>> {
  const errors: Partial<Record<LimitKey, string>> = {};
  for (const field of LIMIT_FIELDS) {
    const input = inputs[field.key];
    const value = limitValue(input, field.bytes);
    const cap = organization[field.key];
    if (value === null || (!submitted && input.value.trim() === '')) continue;
    if (!Number.isSafeInteger(value) || value < field.min || value > field.max) {
      errors[field.key] = field.bytes
        ? m.organizations_limit_bytes_error()
        : m.organizations_limit_integer_error({ min: field.min, max: field.max });
    } else if (
      (field.key === 'activity_retention_days' || field.key === 'audit_retention_days') &&
      cap != null &&
      value > cap
    ) {
      errors[field.key] = m.organizations_limit_exceeds_organization();
    }
  }
  return errors;
}

export function limitsPatch(inputs: LimitInputs, saved: OrganizationLimits): OrganizationLimits {
  const patch: OrganizationLimits = {};
  for (const { key, bytes } of LIMIT_FIELDS) {
    const value = limitValue(inputs[key], bytes);
    if (value !== (saved[key] ?? null)) patch[key] = value;
  }
  return patch;
}

export function organizationCap(key: LimitKey, limits: OrganizationLimits): string {
  const value = limits[key];
  if (value == null) return m.organizations_limit_unset();
  if (key === 'max_file_bytes' || key === 'max_storage_bytes') {
    const unit = byteUnit(value);
    return `${String(value / 1024 ** (unit === 'GB' ? 3 : 2))} ${unit}`;
  }
  return String(value);
}
