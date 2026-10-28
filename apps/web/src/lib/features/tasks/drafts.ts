/** 업무 폼은 사용자·조직·탭 키를 쓴다. 저장·열거·보류 구현은 공통 폼과 CSV가 공유한다(D-121·#154). */
import * as forms from '$lib/features/common/form-draft';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';

export { removeFormDraftAt } from '$lib/features/common/form-draft';
export type { StoredFormDraft } from '$lib/features/common/form-draft';
export const formDraftTabKey = (name: string, user = me.userId) =>
  forms.formDraftTabKey(name, user, org.currentId);
export const clearFormDraft = (name: string) => {
  forms.clearFormDraft(name, org.currentId);
};
export const storeFormDraft = (
  name: string,
  value: Record<string, unknown> & { savedAt: string },
) => forms.storeFormDraft(name, value, org.currentId);
export const listFormDrafts = <T extends { savedAt: string }>(
  name: string,
  parse: (value: unknown) => T | undefined,
  user = me.userId,
) => forms.listFormDrafts(name, parse, user, org.currentId);
export const holdFormDraft = <T extends { savedAt: string }>(
  name: string,
  entry: forms.StoredFormDraft<T>,
  user = me.userId,
) => forms.holdFormDraft(name, entry, user, org.currentId);
