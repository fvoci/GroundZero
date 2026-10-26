import { waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { readLocal, scopedKey, writeLocal } from '$lib/storage';

import { TEST_ORGANIZATION } from './fixtures/organization';

const other = { ...TEST_ORGANIZATION, id: 'org-2', name: '다른 조직' };

beforeEach(() => {
  org.reset();
  me.userId = 'user-a';
  window.localStorage.clear();
});

afterEach(() => {
  org.reset();
  me.userId = '';
  vi.restoreAllMocks();
  window.localStorage.clear();
});

describe('organization selection follows the account setting', () => {
  it('automatically selects the only organization without choosing the first of several', () => {
    org.load('user-a', [TEST_ORGANIZATION]);
    expect(org.currentId).toBe(TEST_ORGANIZATION.id);
    org.reset();
    org.load('user-a', [TEST_ORGANIZATION, other]);
    expect(org.currentId).toBeUndefined();
  });

  it('uses the server choice over a different browser choice', () => {
    writeLocal(scopedKey('organization', 'user-a', undefined), TEST_ORGANIZATION.id);
    org.load('user-a', [TEST_ORGANIZATION, other], other.id);
    expect(org.currentId).toBe(other.id);
    expect(readLocal(scopedKey('organization', 'user-a', undefined))).toBe(other.id);
  });

  it('does not revive a browser choice when the server explicitly cleared it', () => {
    writeLocal(scopedKey('organization', 'user-a', undefined), TEST_ORGANIZATION.id);
    org.load('user-a', [TEST_ORGANIZATION, other], null);
    expect(org.currentId).toBeUndefined();
  });

  it('ignores a remembered organization that is no longer in the membership list', () => {
    org.load('user-a', [TEST_ORGANIZATION, other], 'former-org');
    expect(org.currentId).toBeUndefined();
    org.load('user-a', [other], 'former-org');
    expect(org.currentId).toBe(other.id);
  });

  it('serializes writes so a slow earlier choice cannot overwrite the latest choice', async () => {
    let finishFirst: (() => void) | undefined;
    const save = vi
      .spyOn(api, 'setDisplaySettings')
      .mockImplementationOnce(
        () =>
          new Promise<undefined>((resolve) => {
            finishFirst = () => {
              resolve(undefined);
            };
          }),
      )
      .mockResolvedValue(undefined);
    org.load('user-a', [TEST_ORGANIZATION, other], null);
    org.select(TEST_ORGANIZATION.id);
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(1);
    });
    org.select(other.id);
    expect(org.currentId).toBe(other.id);
    expect(save).toHaveBeenCalledTimes(1);
    finishFirst?.();
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(2);
    });
    expect(save.mock.calls.map(([value]) => value)).toEqual([
      { last_organization_id: TEST_ORGANIZATION.id },
      { last_organization_id: other.id },
    ]);
  });

  it('does not send a queued old-account selection after the account changes', async () => {
    let finishFirst: (() => void) | undefined;
    const save = vi
      .spyOn(api, 'setDisplaySettings')
      .mockImplementationOnce(
        () =>
          new Promise<undefined>((resolve) => {
            finishFirst = () => {
              resolve(undefined);
            };
          }),
      )
      .mockResolvedValue(undefined);
    org.load('user-a', [TEST_ORGANIZATION, other], null);
    org.select(TEST_ORGANIZATION.id);
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(1);
    });
    org.select(other.id);
    org.reset();
    me.userId = 'user-b';
    org.load('user-b', [TEST_ORGANIZATION, other], null);
    finishFirst?.();
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
    expect(save).toHaveBeenCalledTimes(1);
    expect(org.currentId).toBeUndefined();
  });
});
