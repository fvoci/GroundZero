// 겹치는 다시 읽기(목록 갱신 등)의 공통 규칙: 마지막 요청의 응답만 적용하고, 먼저 부른 쪽도 그 마지막 요청이 끝난 뒤에
// 돌아간다. 먼저 부른 쪽은 "갱신이 끝났다"고 믿고 새 목록을 전제로 다음 일을 하기 때문이다(초대 수락 뒤 조직 선택 등).
export interface Latest {
  /** `isCurrent`가 false면 이 요청의 응답은 적용하지 않는다. 더 새 요청의 실패도 그대로 전한다. */
  run(task: (isCurrent: () => boolean) => Promise<void>): Promise<void>;
}

export function createLatest(): Latest {
  let seq = 0;
  let latest: Promise<void> = Promise.resolve();
  return {
    async run(task) {
      const mine = ++seq;
      const own = task(() => mine === seq);
      latest = own;
      let awaited = own;
      await awaited;
      // 기다리는 동안 더 새 요청이 시작됐으면 그 요청(들)까지 기다린다.
      while (latest !== awaited) {
        awaited = latest;
        await awaited;
      }
    },
  };
}
