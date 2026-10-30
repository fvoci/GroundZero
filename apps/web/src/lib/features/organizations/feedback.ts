import { errorText } from '$lib/api/errors';
import { ApiError } from '$lib/shared/api';
import { m } from '$lib/paraglide/messages.js';
import { getLocale } from '$lib/paraglide/runtime.js';

export function organizationUnavailable(error: unknown): boolean {
  return error instanceof ApiError && (error.status === 403 || error.status === 404);
}

// D-81·D-82: keep conflicts beside the action and show server diagnostics only in English.
export function organizationError(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.status === 409 && error.message === 'the organization needs at least one admin')
      return m.organizations_last_admin();
    if (
      error.status === 409 &&
      error.code === 'invalid_state' &&
      error.message === 'already a member'
    )
      return m.organizations_already_member();
    if (
      getLocale() === 'en' &&
      (error.status === 400 || (error.status === 409 && error.code !== 'account_changed'))
    )
      return error.message;
  }
  return errorText(error, m.organizations_action_failed());
}
