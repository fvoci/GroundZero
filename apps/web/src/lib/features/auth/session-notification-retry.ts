import { SESSION_CHECK_MAX_ATTEMPTS } from './session-check';

export interface SessionNotificationRetry {
  /** False means the caller's lifetime ended; notification churn never becomes an error. */
  wait(signal: AbortSignal): Promise<boolean>;
}

/** Notification churn has its own pacing, independent of the safety failure budget. */
export function createSessionNotificationRetry(): SessionNotificationRetry {
  let immediate = SESSION_CHECK_MAX_ATTEMPTS - 1;
  let delay = 250;
  return {
    async wait(signal) {
      if (signal.aborted) return false;
      if (immediate > 0) {
        immediate -= 1;
        return true;
      }
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
