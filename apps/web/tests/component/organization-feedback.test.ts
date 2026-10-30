import { beforeEach, expect, it } from 'vitest';

import { organizationError } from '$lib/features/organizations/feedback';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { ApiError } from '$lib/shared/api';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
});

it.each([
  [
    m.organizations_removal_workspaces,
    'leave 1 workspace',
    'leave 2 workspaces',
    'leave 0 workspaces',
  ],
  [m.organizations_removal_shares, 'revoke 1 share', 'revoke 2 shares', 'revoke 0 shares'],
  [
    m.organizations_removal_workspaces_end,
    'leave 1 workspace',
    'leave 2 workspaces',
    'leave 0 workspaces',
  ],
  [m.organizations_removal_shares_end, 'revoke 1 share', 'revoke 2 shares', 'revoke 0 shares'],
  [
    m.organizations_removal_assignments,
    'release 1 assignment',
    'release 2 assignments',
    'release 0 assignments',
  ],
  [m.organizations_member_count, '1 member', '2 members', '0 members'],
] as const)('inflects the English organization count: %s', (message, one, other, zero) => {
  expect(message({ count: 1 }, { locale: 'en' })).toBe(one);
  expect(message({ count: 2 }, { locale: 'en' })).toBe(other);
  expect(message({ count: 0 }, { locale: 'en' })).toBe(zero);
});

it.each([
  [
    'ko',
    "관리 탭을 더 볼 수 없습니다. '관리자로 참여'로 받은 공간 역할은 참여 전으로 돌아갑니다.",
    '제안을 보냈습니다. 받는 사람이 수락하면 이전됩니다.',
  ],
  [
    'en',
    "You will lose access to management tabs. Workspace roles received through 'Join as administrator' will revert to their previous roles.",
    'Offer sent. Ownership transfers when the recipient accepts.',
  ],
] as const)(
  'keeps user guidance without internal decision references in %s',
  (locale, demote, offer) => {
    expect(m.organizations_demote_body({}, { locale })).toBe(demote);
    expect(m.organizations_ownership_sent({}, { locale })).toBe(offer);
  },
);

it.each([
  [400, 'bad_request', '입력한 값을 확인해 주세요.'],
  [409, 'invalid_state', '지금 상태에서는 할 수 없습니다. 새로고침한 뒤 다시 시도해 주세요.'],
] as const)(
  'localizes organization %s errors while retaining English diagnostics in English',
  async (status, code, reason) => {
    const error = new ApiError(status, code, 'English server diagnostic');
    expect(organizationError(error)).toBe(`작업을 마치지 못했습니다. ${reason}`);
    await setLocale('en', { reload: false });
    expect(organizationError(error)).toBe('English server diagnostic');
  },
);

it('retains the last administrator and account-change guidance', () => {
  expect(
    organizationError(
      new ApiError(409, 'invalid_state', 'the organization needs at least one admin'),
    ),
  ).toBe('마지막 관리자는 바꿀 수 없습니다');
  expect(organizationError(new ApiError(409, 'account_changed', 'account changed'))).toBe(
    '작업을 마치지 못했습니다. 이 창을 연 계정이 아닌 다른 계정으로 로그인돼 있어 서버가 받지 않았습니다. 지금 계정으로 다시 읽습니다.',
  );
});
