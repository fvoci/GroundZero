// 업무 상태·우선순위의 표시 이름. 값은 읽을 때 지금 언어로 만든다(언어는 실행 중에 바뀔 수 있다, D-81).
import type { AssigneeView, LinkKind, TaskInput, TaskPriority } from '$lib/api/client';
import { formatDay } from '$lib/display';
import { shortId } from '$lib/features/workspaces/badge';
import { m } from '$lib/paraglide/messages.js';

import type { MergeUnit } from './form';
import { MERGE_UNITS, scheduleOf, trimLikeServer } from './form';

export const PRIORITY_LABEL: Record<TaskPriority, string> = {
  get none() {
    return m.tasks_priority_none();
  },
  get low() {
    return m.tasks_priority_low();
  },
  get medium() {
    return m.tasks_priority_medium();
  },
  get high() {
    return m.tasks_priority_high();
  },
  get urgent() {
    return m.tasks_priority_urgent();
  },
};

/** 카드의 우선순위 칩 색. 글자(이름)가 늘 함께 있어 색만으로 뜻을 전하지 않는다. */
export const PRIORITY_TONE: Record<TaskPriority, string> = {
  none: '',
  low: 'bg-muted text-muted-foreground',
  medium: 'bg-muted text-foreground',
  high: 'bg-amber-500/15 text-amber-800 dark:text-amber-200',
  urgent: 'bg-destructive/10 text-destructive',
};

/** 선행 관계 종류의 이름(뜻과 약어, 예: "종료 후 시작 (FS)", D-91). */
export const LINK_KIND_LABEL: Record<LinkKind, string> = {
  get FS() {
    return m.tasks_link_kind_fs();
  },
  get SS() {
    return m.tasks_link_kind_ss();
  },
  get FF() {
    return m.tasks_link_kind_ff();
  },
  get SF() {
    return m.tasks_link_kind_sf();
  },
};

/** 선행 관계 종류가 후속 업무에 요구하는 것을 풀어 쓴 문장. */
export const LINK_KIND_MEANING: Record<LinkKind, string> = {
  get FS() {
    return m.tasks_link_meaning_fs();
  },
  get SS() {
    return m.tasks_link_meaning_ss();
  },
  get FF() {
    return m.tasks_link_meaning_ff();
  },
  get SF() {
    return m.tasks_link_meaning_sf();
  },
};

/** 관계 한 줄: 종류 이름과, 지연일이 있으면 "N일 뒤"(예: "시작 후 시작 (SS), 2일 뒤"). */
export function linkText(link: { kind: LinkKind; lag_days: number }): string {
  const kind = LINK_KIND_LABEL[link.kind];
  return link.lag_days > 0
    ? m.tasks_link_text({ kind, lag: m.tasks_link_lag({ days: link.lag_days }) })
    : kind;
}

/** 업무 이름(참조 번호와 제목). 미리보기·알림이 같은 표기를 쓴다. */
export const taskName = (t: { reference: string | null; title: string }): string =>
  `${t.reference ?? ''} ${t.title}`.trim();

/** 날짜가 없어 옮기지 않는 업무 알림(없으면 빈 글). */
export function undatedText(
  undated: readonly { reference: string | null; title: string }[],
): string {
  return undated.length > 0
    ? m.tasks_shift_undated({ tasks: undated.map(taskName).join(', ') })
    : '';
}

/** 충돌 확인에 보이는 단위 하나: 폼의 칸 이름, 서버의 지금 값, 내 값(읽을 수 있는 글로). */
export interface ClashRow {
  key: MergeUnit;
  label: string;
  server: string;
  mine: string;
  /** 서로 다른 값이 화면에서는 똑같아 보인다(확인이 그렇다고 알린다). */
  looksSame: boolean;
}

/** 폼의 칸 이름(폼의 이름표와 같은 글). 일정은 시작일·마감일·마일스톤 묶음의 이름이다. */
export function fieldLabel(unit: MergeUnit): string {
  switch (unit) {
    case 'title':
      return m.tasks_title();
    case 'description':
      return m.tasks_description();
    case 'status':
      return m.tasks_status();
    case 'priority':
      return m.tasks_priority();
    case 'schedule':
      return m.tasks_clash_schedule();
    case 'estimate_minutes':
      return m.tasks_estimate();
    case 'tags':
      return m.tasks_tags();
    case 'assignee_ids':
      return m.tasks_assignees();
  }
}

// 글은 줄이거나 공백을 모으지 않고 서버가 저장하는 그대로(앞뒤 공백만 뗀다) 보인다. 긴 값이나 줄바꿈·공백만 다른
// 값도 서로 다르게 보여야 무엇을 덮어쓰는지 알 수 있다(확인 화면이 줄바꿈을 지켜 보이고 길면 스크롤한다).
function fullText(text: string): string {
  const stored = trimLikeServer(text);
  return stored === '' ? m.tasks_clash_empty() : stored;
}

const dayText = (day: string | null): string =>
  day === null ? m.tasks_clash_empty() : formatDay(day);

/**
 * 사람마다 보일 이름(사용자 id → 이름). 표시 이름은 겹칠 수 있으므로, 이름이 같은 다른 사람이 있을 때만 워크스페이스
 * 배지(D-17)처럼 id 끝 네 글자를 붙인다(예: "Alex #a1a1"). 이메일 등 멤버 화면이 보이지 않는 정보는 쓰지 않는다. 같은
 * 사람이 여러 번 들어와도(지금 판·서버 판·지정할 수 있는 사람) 한 사람으로 센다.
 */
export function assigneeLabels(people: readonly AssigneeView[]): Map<string, string> {
  const names = new Map(people.map((p) => [p.user_id, p.display_name]));
  const count = new Map<string, number>();
  for (const name of names.values()) count.set(name, (count.get(name) ?? 0) + 1);
  return new Map(
    [...names].map(([id, name]) => [
      id,
      (count.get(name) ?? 0) > 1 ? `${name} #${shortId(id)}` : name,
    ]),
  );
}

/** 단위 값 하나를 읽을 수 있는 글로. 담당자는 `names`(사용자 id → 보일 이름, `assigneeLabels`)에서 찾는다. */
export function fieldText(
  unit: MergeUnit,
  input: TaskInput,
  names: ReadonlyMap<string, string>,
): string {
  switch (unit) {
    case 'title':
    case 'description':
      return fullText(input[unit]);
    case 'status':
      return input.status;
    case 'priority':
      return PRIORITY_LABEL[input.priority];
    case 'schedule': {
      // 저장할 때 보내는 일정 그대로(마일스톤이면 시작일 없음) 묶음 전체를 한 줄로 보인다.
      const s = scheduleOf(input);
      return s.milestone
        ? m.tasks_clash_schedule_milestone({ due: dayText(s.due_date) })
        : m.tasks_clash_schedule_dates({ start: dayText(s.start_date), due: dayText(s.due_date) });
    }
    case 'estimate_minutes':
      // 폼의 예상 시간 칸과 같은 글(`String`)이라 확인의 값을 칸에서 그대로 찾을 수 있다.
      return input.estimate_minutes === null
        ? m.tasks_clash_empty()
        : String(input.estimate_minutes);
    case 'tags':
      return input.tags.length === 0 ? m.tasks_clash_empty() : input.tags.join(', ');
    case 'assignee_ids':
      // 담당자는 순서 없는 모음이라 이름순으로 보인다(양쪽을 같은 순서로 견주어 볼 수 있게). 이름은 `assigneeLabels`로
      // 만든 것이라 같은 이름의 두 사람이 구분되고, 모르는 사람도 서로 구분되게 id 끝 네 글자를 붙인다.
      return input.assignee_ids.length === 0
        ? m.tasks_clash_empty()
        : input.assignee_ids
            .map((id) => names.get(id) ?? `${m.tasks_clash_unknown_person()} #${shortId(id)}`)
            .sort((a, b) => a.localeCompare(b))
            .join(', ');
  }
}

/** 양쪽이 서로 다르게 바꾼 단위(`keys`)를 폼 순서로, 서버의 지금 값(`server`)과 내 값(`mine`)과 함께. */
export function clashRows(
  keys: readonly MergeUnit[],
  server: TaskInput,
  mine: TaskInput,
  names: ReadonlyMap<string, string>,
): ClashRow[] {
  return MERGE_UNITS.filter((unit) => keys.includes(unit)).map((unit) => {
    const row = {
      key: unit,
      label: fieldLabel(unit),
      server: fieldText(unit, server, names),
      mine: fieldText(unit, mine, names),
    };
    return { ...row, looksSame: seen(row.server) === seen(row.mine) };
  });
}

// 화면에 보이는 모양: 줄 끝 공백(줄바꿈을 지켜 보여도 보이지 않는다)과 보이지 않는 서식 글자(U+FEFF, U+200B 등)를 뺀다.
// 글이 실제로 "(비어 있음)"이거나 태그 하나에 쉼표가 들어 있어 표시가 같아지는 경우도 이 비교로 잡힌다.
function seen(text: string): string {
  return text.replace(/\p{Cf}/gu, '').replace(/[^\S\n]+$/gm, '');
}
