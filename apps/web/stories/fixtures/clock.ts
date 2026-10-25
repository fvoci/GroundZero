// 화면이 "지금"(new Date()·Date.now())을 직접 읽는 story에서 지금을 합성 데이터의 기준일로 옮긴다. 시계는 그 시각부터
// 그대로 흐르므로(멈추지 않음) 지연·타이머 동작은 실제와 같다. 보는 날짜와 상관없이 오늘 표시·지난 마감이 같게 보인다.
import { TODAY } from './synthetic';

/** 합성 데이터의 "오늘" 한낮(Asia/Seoul로도 같은 날). */
const NOW = Date.parse(`${TODAY}T03:00:00Z`);

/** story의 `beforeEach`에서 부르고 돌려준 정리 함수를 그대로 돌려준다. */
export function fixClock(): () => void {
  const RealDate = globalThis.Date;
  const realNowDescriptor = Object.getOwnPropertyDescriptor(RealDate, 'now');
  const wallClock = (): number => performance.timeOrigin + performance.now();
  const offset = NOW - wallClock();
  const now = (): number => wallClock() + offset;
  RealDate.now = now;
  // 인자 없는 생성·호출만 옮기고, 날짜를 주고 만드는 것은 그대로 둔다.
  globalThis.Date = new Proxy(RealDate, {
    construct: (target, args: ConstructorParameters<DateConstructor> | []) =>
      args.length === 0 ? new target(now()) : new target(...args),
    apply: () => new RealDate(now()).toString(),
  });
  return () => {
    globalThis.Date = RealDate;
    if (realNowDescriptor !== undefined) Object.defineProperty(RealDate, 'now', realNowDescriptor);
  };
}
