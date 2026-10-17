import { afterEach, describe, expect, it, vi } from 'vitest';

import { isHwp, renderHwp } from '$lib/features/files/hwp';

/** 답하지 않는(멈춘) 작업자. 끝냈는지만 본다. */
class SilentWorker {
  static last: SilentWorker | undefined;
  terminated = false;
  posted: unknown[] = [];
  private listeners = new Map<string, ((event: MessageEvent<unknown>) => void)[]>();

  constructor() {
    SilentWorker.last = this;
  }

  addEventListener(type: string, listener: (event: MessageEvent<unknown>) => void): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  postMessage(message: unknown): void {
    this.posted.push(message);
  }

  terminate(): void {
    this.terminated = true;
  }

  emit(data: unknown): void {
    for (const listener of this.listeners.get('message') ?? []) {
      listener(new MessageEvent('message', { data }));
    }
  }
}

afterEach(() => {
  vi.unstubAllGlobals();
  SilentWorker.last = undefined;
});

describe('HWP preview worker driver (D-76)', () => {
  it('recognises HWP and HWPX files', () => {
    expect(isHwp('', '보고서.HWP')).toBe(true);
    expect(isHwp('application/haansofthwpx', 'a')).toBe(true);
    expect(isHwp('application/pdf', 'a.pdf')).toBe(false);
  });

  it('stops a worker that takes too long', async () => {
    vi.stubGlobal('Worker', SilentWorker);
    const run = renderHwp('/rhwp/abc/', new ArrayBuffer(8), () => undefined, 30);
    await expect(run.result).resolves.toEqual({ kind: 'timeout' });
    expect(SilentWorker.last?.terminated).toBe(true);
  });

  it('cancels on request and ignores late replies', async () => {
    vi.stubGlobal('Worker', SilentWorker);
    const pages: number[] = [];
    const run = renderHwp('/rhwp/abc/', new ArrayBuffer(8), (i) => pages.push(i), 10_000);
    const worker = SilentWorker.last;
    worker?.emit({ type: 'page', index: 0, svg: '<svg/>' });
    run.cancel();
    worker?.emit({ type: 'page', index: 1, svg: '<svg/>' });
    await expect(run.result).resolves.toEqual({ kind: 'cancelled' });
    expect(pages).toEqual([0]);
    expect(worker?.terminated).toBe(true);
  });

  it('reports a damaged file as failed and ignores malformed messages', async () => {
    vi.stubGlobal('Worker', SilentWorker);
    const run = renderHwp('/rhwp/abc/', new ArrayBuffer(8), () => undefined, 10_000);
    const worker = SilentWorker.last;
    worker?.emit({ type: 'page', index: 'x' });
    worker?.emit({ type: 'failed' });
    await expect(run.result).resolves.toEqual({ kind: 'failed' });
    expect(worker?.terminated).toBe(true);
  });
});
