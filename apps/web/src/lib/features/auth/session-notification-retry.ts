import { SESSION_CHECK_MAX_ATTEMPTS } from './session-check';

export interface SessionNotificationRetry {
  /** beforePause runs only before a timed retry; false means the caller's lifetime ended. */
  wait(signal: AbortSignal, beforePause?: () => void): Promise<boolean>;
}

/** Notification churn has its own pacing, independent of the safety failure budget. */
export function createSessionNotificationRetry(): SessionNotificationRetry {
  let immediate = SESSION_CHECK_MAX_ATTEMPTS - 1;
  let delay = 250;
  return {
    async wait(signal, beforePause) {
      const isAborted = (): boolean => signal.aborted;
      if (isAborted()) return false;
      if (immediate > 0) {
        immediate -= 1;
        return true;
      }
      beforePause?.();
      if (isAborted()) return false;
      const duration = delay;
      delay = Math.min(delay * 2, 2_000);
      return new Promise<boolean>((resolve) => {
        const finish = (proceed: boolean): void => {
          clearTimeout(timer);
          signal.removeEventListener('abort', abort);
          resolve(proceed);
        };
        const abort = (): void => {
          finish(false);
        };
        const timer = setTimeout(() => {
          finish(true);
        }, duration);
        signal.addEventListener('abort', abort, { once: true });
      });
    },
  };
}
