import { EXPECTED_USER, request, expectOk, fail, ApiError, NetworkError } from '$lib/shared/api';
import { none, parseList, rec, str, type Json } from '$lib/shared/api/parse';
import { accountSwitching } from '$lib/features/feedback/feedback.svelte';
import { untrack } from 'svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { parseFile, parseUpload, parseVersion, type UploadView, type FileTarget } from './model';
export type { FileView, FileVersionView, UploadView, FileTarget } from './model';

const parseBatch = (v: Json): string => str(rec(v, 'trashed'), 'batch_id');
async function withExpected<T>(
  method: string,
  path: string,
  parse: (v: Json) => T,
  body: Json,
  expectedUser: string | undefined,
): Promise<T> {
  const { status, json } = await request(
    method,
    path,
    body,
    expectedUser === undefined ? {} : { [EXPECTED_USER]: expectedUser },
  );
  if (status < 200 || status >= 300) fail(status, json);
  return parse(json);
}
/** 파일 내용 주소(같은 원점, 쿠키로 권한 확인). */
export function fileContentUrl(id: string, version?: number, inline = false): string {
  const params = new URLSearchParams();
  if (version !== undefined) params.set('version', String(version));
  if (inline) params.set('inline', 'true');
  const query = params.toString();
  return `/api/files/${encodeURIComponent(id)}/content${query === '' ? '' : `?${query}`}`;
}

/** 업로드 조각 보내기(날것 본문 + SHA-256 머리글). */
async function putPart(
  upload: string,
  number: number,
  body: Blob,
  sha256: string,
  expectedUser = me.userId,
): Promise<void> {
  // 일반 JSON 요청 밖 업로드 재시도도 계정 전환 시 멈춘다.
  if (untrack(accountSwitching))
    throw new ApiError(409, 'account_changed', m.common_error_account_changed());
  let response: Response;
  try {
    response = await fetch(`/api/uploads/${encodeURIComponent(upload)}/parts/${String(number)}`, {
      method: 'PUT',
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/octet-stream',
        'x-content-sha256': sha256,
        [EXPECTED_USER]: expectedUser,
      },
      body,
    });
  } catch (error: unknown) {
    throw new NetworkError(error instanceof Error ? error.message : 'network error');
  }
  if (response.status !== 204) {
    const text = await response.text();
    let json: Json;
    try {
      json = text === '' ? null : JSON.parse(text);
    } catch {
      json = null;
    }
    fail(response.status, json);
  }
}

export const filesApi = {
  documentFiles: (id: string) =>
    expectOk('GET', `/api/documents/${encodeURIComponent(id)}/files`, (v) =>
      parseList(v, parseFile),
    ),
  itemFiles: (id: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(id)}/files`, (v) => parseList(v, parseFile)),
  file: (id: string, signal?: AbortSignal) =>
    expectOk('GET', `/api/files/${encodeURIComponent(id)}`, parseFile, undefined, signal),
  fileVersions: (id: string) =>
    expectOk('GET', `/api/files/${encodeURIComponent(id)}/versions`, (v) =>
      parseList(v, parseVersion),
    ),
  renameFile: (id: string, name: string, expectedUser?: string) =>
    withExpected(
      'PATCH',
      `/api/files/${encodeURIComponent(id)}`,
      parseFile,
      { name },
      expectedUser,
    ),
  deleteFileVersion: (id: string, version: number) =>
    expectOk('DELETE', `/api/files/${encodeURIComponent(id)}/versions/${String(version)}`, none),
  trashFile: (id: string) => expectOk('DELETE', `/api/files/${encodeURIComponent(id)}`, parseBatch),
  /**
   * 업로드 시작. 같은 requestId면 같은 업로드(이어 올리기). `expectedUser`는 올리기를 시작한 화면의 사용자 id(서버가
   * 세션과 다르면 409 `account_changed`, 만료된 업로드를 스스로 다시 시작할 때 새 계정의 업로드를 만들지 않게).
   */
  async createUpload(input: {
    expectedUser: string;
    requestId: string;
    target?: FileTarget;
    fileId?: string;
    name: string;
    note: string;
    contentType: string;
    size: number;
  }): Promise<UploadView> {
    const { status, json } = await request(
      'POST',
      '/api/uploads',
      {
        request_id: input.requestId,
        target: input.target ?? null,
        file_id: input.fileId ?? null,
        name: input.name,
        note: input.note,
        content_type: input.contentType,
        size: input.size,
      },
      { [EXPECTED_USER]: input.expectedUser },
    );
    if (status < 200 || status >= 300) fail(status, json);
    return parseUpload(json);
  },
  upload: (id: string) => expectOk('GET', `/api/uploads/${encodeURIComponent(id)}`, parseUpload),
  putUploadPart: putPart,
  completeUpload: (id: string, expectedUser?: string) =>
    withExpected(
      'POST',
      `/api/uploads/${encodeURIComponent(id)}/complete`,
      parseUpload,
      undefined,
      expectedUser,
    ),
  abortUpload: (id: string, expectedUser?: string) =>
    withExpected('DELETE', `/api/uploads/${encodeURIComponent(id)}`, none, undefined, expectedUser),
};
