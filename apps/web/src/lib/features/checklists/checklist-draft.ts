import type { ParsedChecklistItem } from './checklist-item';

export interface ChecklistDraft {
  draft: string;
  draftRequest: { text: string; requestId: string } | null;
  renaming: { id: string; text: string; original: string } | null;
  pasteOpen: boolean;
  pasted: string;
  preview: ParsedChecklistItem[];
  previewIds: Record<string, string[]>;
}

const record = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === 'object' && !Array.isArray(v);

export function parseChecklistDraft(v: unknown): ChecklistDraft | undefined {
  if (
    !record(v) ||
    typeof v.draft !== 'string' ||
    typeof v.pasted !== 'string' ||
    typeof v.pasteOpen !== 'boolean' ||
    !Array.isArray(v.preview) ||
    !record(v.previewIds)
  )
    return undefined;
  const draftRequest = v.draftRequest;
  if (
    draftRequest !== null &&
    (!record(draftRequest) ||
      typeof draftRequest.text !== 'string' ||
      typeof draftRequest.requestId !== 'string')
  )
    return undefined;
  const renaming = v.renaming;
  if (
    renaming !== null &&
    (!record(renaming) ||
      typeof renaming.id !== 'string' ||
      typeof renaming.text !== 'string' ||
      typeof renaming.original !== 'string')
  )
    return undefined;
  const preview: ParsedChecklistItem[] = [];
  for (const row of v.preview) {
    if (!record(row) || typeof row.text !== 'string' || typeof row.done !== 'boolean')
      return undefined;
    preview.push({ text: row.text, done: row.done });
  }
  const previewIds: Record<string, string[]> = {};
  for (const [key, ids] of Object.entries(v.previewIds)) {
    if (!Array.isArray(ids) || !ids.every((id): id is string => typeof id === 'string'))
      return undefined;
    Object.defineProperty(previewIds, key, {
      value: ids,
      enumerable: true,
      writable: true,
      configurable: true,
    });
  }
  return {
    draft: v.draft,
    draftRequest:
      draftRequest === null
        ? null
        : { text: String(draftRequest.text), requestId: String(draftRequest.requestId) },
    renaming:
      renaming === null
        ? null
        : {
            id: String(renaming.id),
            text: String(renaming.text),
            original: String(renaming.original),
          },
    pasteOpen: v.pasteOpen,
    pasted: v.pasted,
    preview,
    previewIds,
  };
}
