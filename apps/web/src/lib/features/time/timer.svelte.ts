import type { TimeEntryView, TimeKind, TimeTargetKind } from '$lib/features/time/model';
import { timeData } from '$lib/features/time/data';
// 진행 중인 타이머(사용자당 하나, D-32). 화면 어디서나 같은 상태를 보도록 하나만 둔다.

import { ApiError, newRequestId } from '$lib/api/client';
import { errorText } from '$lib/api/errors';
import { m } from '$lib/paraglide/messages.js';
import { accountSwitching, notifyDone } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';

import { liveSeconds } from './format';

class TimerStore {
  entry = $state<TimeEntryView | null>(null);
  error = $state<string | undefined>(undefined);
  restartPending = $state(false);
  #restartChoice: ((mode: 'continue' | 'new' | undefined) => void) | undefined;

  /** 취소·다른 쓰기·계정 전환도 대기 중인 선택을 끝내 원래 호출을 풀어 준다. */
  chooseRestart(mode: 'continue' | 'new' | undefined): void {
    const resolve = this.#restartChoice;
    this.#restartChoice = undefined;
    this.restartPending = false;
    resolve?.(mode);
  }

  #askRestart(): Promise<'continue' | 'new' | undefined> {
    this.restartPending = true;
    return new Promise((resolve) => {
      this.#restartChoice = resolve;
    });
  }
  #fetchedAt = $state(Date.now());
  #now = $state(Date.now());
  #tick: ReturnType<typeof setInterval> | undefined;
  // 늦게 온 옛 응답이 그 뒤의 멈춤·시작을 되돌리지 않게 한다. 쓰기(시작·멈춤 등)는 그 뒤에 시작한 쓰기가 없을 때
  // 반영하고, 읽기는 시작할 때 진행 중인 쓰기가 없었고 그 뒤 시작한 요청도 없을 때만 반영한다(쓰기 도중의 읽기는
  // 쓰기 전 상태일 수 있다).
  #seq = 0;
  #writeSeq = 0;
  #writing = 0;
  // 겹친 쓰기 가운데 결과를 버렸거나 서버 상태와 달라 실패한 것이 있었다. 마지막 쓰기가 끝날 때 서버 상태로 맞춘다.
  #resync = false;

  #beginWrite(): number {
    this.chooseRestart(undefined);
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
      const entry = await timeData.currentTimer();
      if (duringWrite || mine !== this.#seq) return;
      this.#set(entry);
      this.error = undefined;
    } catch (e: unknown) {
      // 옛 읽기의 실패는 지금 상태와 맞지 않으므로 알리지 않는다(성공과 같은 순서 검사).
      if (duringWrite || mine !== this.#seq) return;
      this.error = errorText(e, m.time_timer_load_failed());
    }
  }

  /** 서버 상태가 이 화면과 달라 생긴 실패(이미 진행 중·이미 바뀜). 네트워크·입력 오류는 다시 읽지 않는다. */
  static #stale(e: unknown): boolean {
    return e instanceof ApiError && (e.status === 404 || e.status === 409);
  }

  /**
   * 쓰기 하나가 끝났다. 서버 상태와 달라 실패했거나(`applied` 거짓) 뒤에 시작한 쓰기 때문에 결과를 버렸으면, 진행 중인
   * 쓰기가 다 끝난 뒤 서버 상태로 맞춘다(먼저 보낸 시작이 성공하고 뒤의 시작이 409로 실패하면, 앞의 성공을 버린 채 화면이
   * 비어 있었다). 버린 결과는 마지막 쓰기까지 기억한다(마지막 쓰기가 네트워크 오류로 끝나도 다시 읽는다).
   */
  async #finishWrite(applied: boolean): Promise<void> {
    this.#writing -= 1;
    if (!applied) this.#resync = true;
    if (this.#writing === 0 && this.#resync) {
      this.#resync = false;
      await this.load();
    }
  }

  /** D-117: 서버가 이전 기록 저장과 새 시작을 한 트랜잭션에서 처리한다. */
  async start(
    kind: TimeKind,
    target: { kind: TimeTargetKind; id: string } | null = null,
    memo = '',
  ): Promise<void> {
    const mine = this.#beginWrite();
    const userId = me.userId;
    const requestId = newRequestId();
    const current = (): boolean =>
      mine === this.#writeSeq && me.userId === userId && !accountSwitching();
    let entry: TimeEntryView;
    try {
      let started;
      try {
        started = await timeData.startTimer(requestId, kind, memo, target);
      } catch (error: unknown) {
        if (!(error instanceof ApiError) || error.status !== 409 || error.code !== 'timer_running')
          throw error;
        if (!current()) {
          await this.#finishWrite(true);
          return;
        }
        const mode = await this.#askRestart();
        if (mode === undefined || !current()) {
          await this.#finishWrite(true);
          return;
        }
        started = await timeData.startTimer(requestId, kind, memo, target, mode);
      }
      entry = started;
      if (current()) {
        notifyDone(
          started.stopped_entry_id != null ? m.time_previous_saved() : m.time_timer_started(),
        );
      }
    } catch (e: unknown) {
      await this.#finishWrite(!TimerStore.#stale(e));
      throw e;
    }
    const latest = current();
    if (latest) this.#set(entry);
    await this.#finishWrite(latest);
  }

  async act(action: 'pause' | 'resume' | 'stop'): Promise<TimeEntryView | undefined> {
    if (this.entry === null) return undefined;
    const mine = this.#beginWrite();
    let next: TimeEntryView;
    try {
      next = await timeData.timerAction(this.entry.id, action);
    } catch (e: unknown) {
      // 다른 탭·기기에서 이미 바뀌었다(404·409). 서버 상태로 맞춘 뒤 알린다.
      await this.#finishWrite(!TimerStore.#stale(e));
      throw e;
    }
    const latest = mine === this.#writeSeq;
    if (latest) this.#set(action === 'stop' ? null : next);
    await this.#finishWrite(latest);
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
