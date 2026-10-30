import { expectOk } from '$lib/shared/api';
import { parseList } from '$lib/shared/api/parse';
import {
  parseCreatedOrganization,
  parseServerAdmin,
  parseServerOrganization,
  type CreateOrganization,
} from './model';

export const serverAdminApi = {
  organizations: () =>
    expectOk('GET', '/api/server/organizations', (v) => parseList(v, parseServerOrganization)),
  admins: () => expectOk('GET', '/api/server/admins', (v) => parseList(v, parseServerAdmin)),
  createOrganization: (body: CreateOrganization) =>
    expectOk('POST', '/api/server/organizations', parseCreatedOrganization, body),
};
