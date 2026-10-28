import { m } from '$lib/paraglide/messages.js';
import { ApiError } from '$lib/shared/api';

/** #189·CR205-1: 이메일 검증은 칸 오류이며 바이트 상한은 서버가 준다. */
export function invitationEmailError(error: unknown): string | undefined {
  if (!(error instanceof ApiError) || error.status !== 400 || error.field !== 'email')
    return undefined;
  if (error.reason === 'invalid_format') return m.members_email_invalid_format();
  if (error.reason === 'too_long' && error.limit !== undefined)
    return m.members_email_too_long({ limit: error.limit });
  return undefined;
}
