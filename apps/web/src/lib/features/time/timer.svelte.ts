// 진행 중인 타이머(사용자당 하나, D-32). 화면 어디서나 같은 상태를 보도록 하나만 둔다.
import type { TimeEntryView, TimeKind, TimeTargetKind } from '$lib/api/client';
import { api, ApiError, newRequestId } from '$lib/api/client';
import { errorText } from '$lib/api/errors';
import { m } from '$lib/paraglide/messages.js';

import { liveSeconds } from './format';

class TimerStore {
  entry = $state<TimeEntryView | null>(null);
  error = $state<string | undefined>(undefined);
  #fetchedAt = $state(Date.now());
  #now = $state(Date.now());
  #tick: ReturnType<typeof setInterval> | undefined;
  // 늦게 온 옛 응답이 그 뒤의 멈춤·시작을 되돌리지 않게 한다. 쓰기(시작·멈춤 등)는 그 뒤에 시작한 쓰기가 없을 때
  // 반영하고, 읽기는 시작할 때 진행 중인 쓰기가 없었고 그 뒤 시작한 요청도 없을 때만 반영한다(쓰기 도중의 읽기는
  // 쓰기 전 상태일 수 있다).
  #seq = 0;
  #writeSeq = 0;
  #writing = 0;

  #beginWrite(): number {
    this.#seq += 1;
    this.#writing += 1;
    this.#writeSeq += 1;
    return this.#writeSeq;
  }

  /** 지금까지의 길이(초). */
  seconds = $derived(this.entry === null ? 0 : liveSeconds(this.entry, this.#fetchedAt, this.#now));

  #set(entry: TimeEntryView | null): void {
    this.entry = entry;
    this.#fetchedAt = Date.now();
    this.#now = this.#fetchedAt;
    const running = entry !== null && entry.ended_at === null && entry.paused_at === null;
    if (running && this.#tick === undefined) {
      this.#tick = setInterval(() => {
        this.#now = Date.now();
      }, 1000);
    } else if (!running && this.#tick !== undefined) {
      clearInterval(this.#tick);
      this.#tick = undefined;
    }
  }

  /** 로그인·다시 접속할 때 서버에서 이어 받는다. */
  async load(): Promise<void> {
    const mine = ++this.#seq;
    const duringWrite = this.#writing > 0;
    try {
      const entry = await api.currentTimer();
      if (duringWrite || mine !== this.#seq) return;
      this.#set(entry);
      this.error = undefined;
    } catch (e: unknown) {
      this.error = errorText(e, m.time_timer_load_failed());
    }
  }

  /** 시작. 이미 진행 중이면 409이므로 먼저 끝내야 한다(호출하는 쪽이 사용자에게 묻는다). */
  async start(
    kind: TimeKind,
    target: { kind: TimeTargetKind; id: string } | null = null,
    memo = '',
  ): Promise<void> {
    const mine = this.#beginWrite();
    let entry: TimeEntryView;
    try {
      entry = await api.startTimer(newRequestId(), kind, memo, target);
    } catch (e: unknown) {
      this.#writing -= 1;
      if (e instanceof ApiError && e.status === 409) await this.load();
      throw e;
    }
    this.#writing -= 1;
    if (mine === this.#writeSeq) this.#set(entry);
  }

  async act(action: 'pause' | 'resume' | 'stop'): Promise<TimeEntryView | undefined> {
    if (this.entry === null) return undefined;
    const mine = this.#beginWrite();
    let next: TimeEntryView;
    try {
      next = await api.timerAction(this.entry.id, action);
    } catch (e: unknown) {
      this.#writing -= 1;
      // 다른 탭·기기에서 이미 바뀌었다. 서버 상태로 맞춘 뒤 알린다.
      if (e instanceof ApiError && (e.status === 404 || e.status === 409)) await this.load();
      throw e;
    }
    this.#writing -= 1;
    if (mine === this.#writeSeq) this.#set(action === 'stop' ? null : next);
    return next;
  }

  /** 탭을 다시 볼 때 다른 탭·기기의 변경을 따라잡는다. */
  watchVisibility(): () => void {
    const onVisible = (): void => {
      if (document.visibilityState === 'visible') void this.load();
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      document.removeEventListener('visibilitychange', onVisible);
    };
  }
}

export const timer = new TimerStore();
