import type { components } from '@fvoci/api-client';

import { expectOk } from '$lib/shared/api';

type SetLastOrganization = components['schemas']['SetLastOrganization'];

// #178·D-105: 마지막 조직 선택은 표시 설정과 독립된 명령이다.
export const organizationsApi = {
  setLastOrganization: (body: SetLastOrganization) =>
    expectOk('PUT', '/api/me/last-organization', () => undefined, body),
};
