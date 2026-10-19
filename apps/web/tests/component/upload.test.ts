import { afterEach, describe, expect, it, vi } from 'vitest';

import type { UploadView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
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
    await expect(uploadFile(first, { fileId: 'f1' })).rejects.toThrow('stop');

    create.mockResolvedValue(view([1]));
    sent.length = 0;
    const changed = new File(['CCCCBBBB'], 'a.bin', { lastModified: 1000 });
    await uploadFile(changed, { fileId: 'f1' });
    expect(put).toHaveBeenCalled();
    expect(sent).toEqual([
      [1, 'CCCC'],
      [2, 'BBBB'],
    ]);
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
    await expect(uploadFile(file, { fileId: 'f1' })).rejects.toThrow('stop');

    create.mockResolvedValue(view([1]));
    sent.length = 0;
    await uploadFile(file, { fileId: 'f1' });
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
