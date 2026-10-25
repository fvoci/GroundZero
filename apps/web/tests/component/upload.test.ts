import { afterEach, describe, expect, it, vi } from 'vitest';

import type { UploadView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import { scopedKey } from '$lib/storage';
import { formatBytes, missingParts, partRange, uploadFile } from '$lib/features/files/upload';

describe('chunked upload helpers', () => {
  it('splits a file into server-sized parts', () => {
    expect(partRange(1, 5, 12)).toEqual([0, 5]);
    expect(partRange(3, 5, 12)).toEqual([10, 12]);
  });

  it('resumes with only the parts the server has not received', () => {
    expect(missingParts(4, [2, 4])).toEqual([1, 3]);
    expect(missingParts(1, [1])).toEqual([]);
  });

  it('formats sizes for people', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(1536)).toBe('1.5 KB');
    expect(formatBytes(5 * 1024 * 1024 + 10)).toBe('5.0 MB');
  });
});

// 감사 1번: 이어 올리기는 이름·크기·수정 시각이 같은 파일을 같은 업로드로 본다. 그 사이 내용이 바뀌었으면 서버가
// 이미 받은 조각도 지금 파일과 같은지 확인해야 한다(아니면 옛 조각과 새 조각이 섞인 파일이 만들어진다).
describe('resuming an upload', () => {
  it('never reuses another account’s resume request for the same file', async () => {
    const create = vi
      .spyOn(api, 'createUpload')
      .mockRejectedValue(new ApiError(400, 'bad_request', 'stop'));
    const file = new File(['same'], 'same.bin', { lastModified: 1 });
    await expect(uploadFile(file, { expectedUser: 'u1', fileId: 'f1' })).rejects.toThrow('stop');
    await expect(uploadFile(file, { expectedUser: 'u2', fileId: 'f1' })).rejects.toThrow('stop');
    await expect(uploadFile(file, { expectedUser: 'u1', fileId: 'f1' })).rejects.toThrow('stop');
    expect(create.mock.calls[0]?.[0].requestId).toBe(create.mock.calls[2]?.[0].requestId);
    expect(create.mock.calls[1]?.[0].requestId).not.toBe(create.mock.calls[0]?.[0].requestId);
    expect(localStorage.getItem(scopedKey('uploads', 'u1', undefined))).not.toBeNull();
    expect(localStorage.getItem(scopedKey('uploads', 'u2', undefined))).not.toBeNull();
    expect(localStorage.getItem('fvoci.uploads')).toBeNull();
  });

  it('does not poll or retry completion after account_changed', async () => {
    vi.spyOn(api, 'createUpload').mockResolvedValue({ ...view([]), state: 'completing' });
    vi.spyOn(api, 'completeUpload').mockRejectedValue(
      new ApiError(409, 'account_changed', 'changed'),
    );
    const poll = vi.spyOn(api, 'upload');
    await expect(
      uploadFile(new File(['same'], 'same.bin'), { expectedUser: 'u1', fileId: 'f1' }),
    ).rejects.toThrow('changed');
    expect(poll).not.toHaveBeenCalled();
  });
  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  function view(received: number[]): UploadView {
    return {
      id: 'up-1',
      state: 'uploading',
      name: 'a.bin',
      size: 8,
      chunk_bytes: 4,
      parts_total: 2,
      received,
      expires_at: new Date(Date.now() + 60_000).toISOString(),
      file_id: null,
      version: null,
      failure: null,
    };
  }

  it('resends a received part whose bytes changed since it was sent', async () => {
    const sent: [number, string][] = [];
    const stop = oneShot();
    const put = vi
      .spyOn(api, 'putUploadPart')
      .mockImplementation(async (_id: string, n: number, piece: Blob) => {
        sent.push([n, await piece.text()]);
        if (n === 2 && stop.once()) throw new ApiError(400, 'bad', 'stop');
      });
    vi.spyOn(api, 'completeUpload').mockResolvedValue({ ...view([1, 2]), state: 'done' });
    const create = vi.spyOn(api, 'createUpload').mockResolvedValue(view([]));
    const first = new File(['AAAABBBB'], 'a.bin', { lastModified: 1000 });
    await expect(uploadFile(first, { expectedUser: 'u1', fileId: 'f1' })).rejects.toThrow('stop');

    create.mockResolvedValue(view([1]));
    sent.length = 0;
    const changed = new File(['CCCCBBBB'], 'a.bin', { lastModified: 1000 });
    await uploadFile(changed, { expectedUser: 'u1', fileId: 'f1' });
    expect(put).toHaveBeenCalled();
    expect(sent).toEqual([
      [1, 'CCCC'],
      [2, 'BBBB'],
    ]);
  });

  // 두 조각을 다 받은 뒤 첫 조각만 바뀌어 다시 보내면 진행률은 받은 바이트의 합이다(첫 조각의 끝 위치 4/8이 아니라 8/8).
  it('reports cumulative progress after resending a changed part', async () => {
    vi.spyOn(api, 'putUploadPart').mockResolvedValue(undefined);
    const complete = vi
      .spyOn(api, 'completeUpload')
      .mockRejectedValueOnce(new ApiError(400, 'bad', 'stop'));
    const create = vi.spyOn(api, 'createUpload').mockResolvedValue(view([]));
    await expect(
      uploadFile(new File(['AAAABBBB'], 'a.bin', { lastModified: 2000 }), {
        expectedUser: 'u1',
        fileId: 'f2',
      }),
    ).rejects.toThrow('stop');

    create.mockResolvedValue(view([1, 2]));
    complete.mockResolvedValue({ ...view([1, 2]), state: 'done' });
    const seen: number[] = [];
    await uploadFile(new File(['CCCCBBBB'], 'a.bin', { lastModified: 2000 }), {
      expectedUser: 'u1',
      fileId: 'f2',
      onProgress: (p) => seen.push(p.sent),
    });
    expect(seen.at(0)).toBe(4);
    expect(seen.at(-1)).toBe(8);
  });

  // 검토(2차) 6번: 그만둔 업로드의 조각 해시가 저장소에 계속 쌓이지 않는다(기한이 지난 업로드는 이어 올릴 수 없다).
  it('drops part hashes of uploads past their expiry', async () => {
    window.localStorage.setItem(
      scopedKey('upload-parts', 'u1', undefined),
      JSON.stringify({
        'abandoned-1': { expires: '2026-01-01T00:00:00Z', hashes: { '1': 'aa' } },
        'abandoned-2': { '1': 'bb' },
      }),
    );
    vi.spyOn(api, 'putUploadPart').mockResolvedValue(undefined);
    vi.spyOn(api, 'createUpload').mockResolvedValue(view([]));
    // 마치기 직전(이 업로드의 해시는 아직 있을 때)의 저장 내용을 본다.
    let kept = '';
    vi.spyOn(api, 'completeUpload').mockImplementation(() => {
      kept = window.localStorage.getItem(scopedKey('upload-parts', 'u1', undefined)) ?? '';
      return Promise.resolve({ ...view([1, 2]), state: 'done' });
    });
    await uploadFile(new File(['AAAABBBB'], 'a.bin', { lastModified: 1 }), {
      expectedUser: 'u1',
      fileId: 'f2',
    });
    expect(kept).toContain('up-1');
    expect(kept).not.toContain('abandoned');
  });

  it('skips a received part that still matches', async () => {
    const sent: number[] = [];
    const stop = oneShot();
    vi.spyOn(api, 'putUploadPart').mockImplementation((_id: string, n: number) => {
      sent.push(n);
      return n === 2 && stop.once()
        ? Promise.reject(new ApiError(400, 'bad', 'stop'))
        : Promise.resolve();
    });
    vi.spyOn(api, 'completeUpload').mockResolvedValue({ ...view([1, 2]), state: 'done' });
    const create = vi.spyOn(api, 'createUpload').mockResolvedValue(view([]));
    const file = new File(['AAAABBBB'], 'a.bin', { lastModified: 1000 });
    await expect(uploadFile(file, { expectedUser: 'u1', fileId: 'f1' })).rejects.toThrow('stop');

    create.mockResolvedValue(view([1]));
    sent.length = 0;
    await uploadFile(file, { expectedUser: 'u1', fileId: 'f1' });
    expect(sent).toEqual([2]);
  });
});

/** 처음 한 번만 참(첫 시도의 두 번째 조각에서 끊는다). */
function oneShot(): { once: () => boolean } {
  let fired = false;
  return {
    once: () => {
      if (fired) return false;
      fired = true;
      return true;
    },
  };
}
