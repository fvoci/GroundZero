import type { components } from '@fvoci/api-client';
import { int, rec, str, type Json } from '$lib/shared/api/parse';

export type SavedViewView = Omit<components['schemas']['SavedView'], 'config'> & {
  config: Record<string, unknown>;
};

export function parseSavedView(value: Json): SavedViewView {
  const o = rec(value, 'saved view');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    config: rec(o.config, 'view config'),
    version: int(o, 'version'),
  };
}
