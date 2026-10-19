// 홈 빠른 추가에서 실제로 보낼 값. 재시도 키도 이 값으로 만든다(보내지 않는 칸이 키를 바꾸거나, 보낼 값이 바뀌었는데
// 키가 같아 옛 결과가 돌아오지 않게).

export interface QuickInput {
  what: 'memo' | 'task' | 'event';
  title: string;
  item: string;
  date: string;
}

export type QuickSend =
  | { what: 'memo'; title: string }
  | { what: 'task'; item: string; title: string }
  | { what: 'event'; item: string; title: string; date: string };

/** 보낼 값. 메모는 대상·날짜를 보내지 않고, 날짜를 비운 일정은 오늘로 보낸다. 대상이 필요한데 없으면 undefined. */
export function quickSend(input: QuickInput, today: string): QuickSend | undefined {
  const title = input.title.trim();
  if (input.what === 'memo') return { what: 'memo', title };
  if (input.item === '') return undefined;
  if (input.what === 'task') return { what: 'task', item: input.item, title };
  return {
    what: 'event',
    item: input.item,
    title,
    date: input.date === '' ? today : input.date,
  };
}
