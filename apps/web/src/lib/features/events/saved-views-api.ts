import { expectOk, fail, request } from '$lib/shared/api';
import { isRecord, parseList, type Json } from '$lib/shared/api/parse';
import { parseSavedView, type SavedViewView } from './saved-views-model';

function versionConflict(status: number, json: Json): unknown {
  if (status !== 409 || !isRecord(json) || json.current === undefined) return undefined;
  return json.current;
}

const none = (): undefined => undefined;

export const savedViewsApi = {
  /** 이 항목의 내 저장한 보기(D-31). config는 화면이 해석한다. */
  savedViews: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/views`, (v) =>
      parseList(v, parseSavedView),
    ),
  /** 새 보기. 같은 이름이 이미 있으면(다른 탭이 먼저 만듦) 만들지 않고 그 보기를 돌려준다(D-103). */
  async createView(
    itemId: string,
    requestId: string,
    name: string,
    config: Record<string, unknown>,
  ): Promise<{ kind: 'saved'; view: SavedViewView } | { kind: 'exists'; current: SavedViewView }> {
    const { status, json } = await request(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/views`,
      {
        request_id: requestId,
        name,
        config: { ...config },
      },
    );
    const current = versionConflict(status, json);
    if (current !== undefined) return { kind: 'exists', current: parseSavedView(current) };
    if (status !== 201) fail(status, json);
    return { kind: 'saved', view: parseSavedView(json) };
  },
  /** 기존 보기 고치기. 읽은 판이 지났으면(다른 탭이 저장함) 바꾸지 않고 최신 보기를 돌려준다(D-103). */
  async updateView(
    viewId: string,
    requestId: string,
    baseVersion: number,
    config: Record<string, unknown>,
  ): Promise<
    { kind: 'saved'; view: SavedViewView } | { kind: 'conflict'; current: SavedViewView }
  > {
    const { status, json } = await request('PUT', `/api/views/${encodeURIComponent(viewId)}`, {
      request_id: requestId,
      base_version: baseVersion,
      config: { ...config },
    });
    const current = versionConflict(status, json);
    if (current !== undefined) return { kind: 'conflict', current: parseSavedView(current) };
    if (status !== 200) fail(status, json);
    return { kind: 'saved', view: parseSavedView(json) };
  },
  deleteView: (id: string) => expectOk('DELETE', `/api/views/${encodeURIComponent(id)}`, none),
};
