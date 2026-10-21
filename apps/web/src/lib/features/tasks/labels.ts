// 업무 상태·우선순위의 표시 이름. 값은 읽을 때 지금 언어로 만든다(언어는 실행 중에 바뀔 수 있다, D-81).
import type { LinkKind, TaskInput, TaskPriority, TaskStatus } from '$lib/api/client';
import { formatDay, formatNumber } from '$lib/display';
import { m } from '$lib/paraglide/messages.js';

export const STATUS_LABEL: Record<TaskStatus, string> = {
  get todo() {
    return m.tasks_status_todo();
  },
  get in_progress() {
    return m.tasks_status_in_progress();
  },
  get done() {
    return m.tasks_status_done();
  },
};

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

/** 충돌 확인에 보이는 칸 하나: 폼의 칸 이름, 서버의 지금 값, 내 값(읽을 수 있는 글로). */
export interface ClashRow {
  key: keyof TaskInput;
  label: string;
  server: string;
  mine: string;
}

// 폼에 놓인 순서(충돌 확인도 이 순서로 보인다).
const FIELD_ORDER: readonly (keyof TaskInput)[] = [
  'title',
  'description',
  'status',
  'priority',
  'start_date',
  'due_date',
  'estimate_minutes',
  'tags',
  'assignee_ids',
  'milestone',
];

/** 폼의 칸 이름(폼의 이름표와 같은 글). */
function fieldLabel(key: keyof TaskInput): string {
  switch (key) {
    case 'title':
      return m.tasks_title();
    case 'description':
      return m.tasks_description();
    case 'status':
      return m.tasks_status();
    case 'priority':
      return m.tasks_priority();
    case 'start_date':
      return m.tasks_start_date();
    case 'due_date':
      return m.tasks_due_date();
    case 'estimate_minutes':
      return m.tasks_estimate();
    case 'tags':
      return m.tasks_tags();
    case 'assignee_ids':
      return m.tasks_assignees();
    case 'milestone':
      return m.tasks_milestone();
  }
}

// 긴 글(설명 등)은 줄바꿈을 한 칸으로 모으고 앞부분만 보인다.
const SHORT = 80;
function shorten(text: string): string {
  const flat = text.replace(/\s+/g, ' ').trim();
  if (flat === '') return m.tasks_clash_empty();
  return flat.length > SHORT ? `${flat.slice(0, SHORT)}…` : flat;
}

/** 칸 값 하나를 읽을 수 있는 글로. 담당자는 `names`(사용자 id → 이름)에서 이름을 찾는다. */
function fieldText(
  key: keyof TaskInput,
  input: TaskInput,
  names: ReadonlyMap<string, string>,
): string {
  switch (key) {
    case 'title':
    case 'description':
      return shorten(input[key]);
    case 'status':
      return STATUS_LABEL[input.status];
    case 'priority':
      return PRIORITY_LABEL[input.priority];
    case 'start_date':
    case 'due_date': {
      const day = input[key];
      return day === null || day === '' ? m.tasks_clash_empty() : formatDay(day);
    }
    case 'estimate_minutes':
      return input.estimate_minutes === null
        ? m.tasks_clash_empty()
        : formatNumber(input.estimate_minutes);
    case 'tags':
      return shorten(input.tags.join(', '));
    case 'assignee_ids':
      return input.assignee_ids.length === 0
        ? m.tasks_clash_empty()
        : input.assignee_ids
            .map((id) => names.get(id) ?? m.tasks_clash_unknown_person())
            .join(', ');
    case 'milestone':
      return input.milestone ? m.tasks_clash_yes() : m.tasks_clash_no();
  }
}

/** 양쪽이 서로 다르게 바꾼 칸(`keys`)을 폼 순서로, 서버의 지금 값(`server`)과 내 값(`mine`)과 함께. */
export function clashRows(
  keys: readonly (keyof TaskInput)[],
  server: TaskInput,
  mine: TaskInput,
  names: ReadonlyMap<string, string>,
): ClashRow[] {
  return FIELD_ORDER.filter((key) => keys.includes(key)).map((key) => ({
    key,
    label: fieldLabel(key),
    server: fieldText(key, server, names),
    mine: fieldText(key, mine, names),
  }));
}
