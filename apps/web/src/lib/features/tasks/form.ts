// 업무 편집 폼 값. 체크리스트·반복·선행 업무를 바꾸면 서버의 새 판으로 다시 채우는데, 그때 사용자가 고쳤지만
// 아직 저장하지 않은 칸은 지킨다(전에는 폼 전체를 덮어 고친 제목·설명 등이 사라졌다).
import type { TaskDetailView, TaskInput } from '$lib/api/client';
import { canonicalJson } from '$lib/features/documents/drafts';

export function formOf(task: TaskDetailView): TaskInput {
  return {
    title: task.title,
    description: task.description,
    status: task.status_name ?? task.status,
    priority: task.priority,
    start_date: task.start_date,
    due_date: task.due_date,
    estimate_minutes: task.estimate_minutes,
    tags: task.tags,
    assignee_ids: task.assignees.map((a) => a.user_id),
    milestone: task.milestone,
  };
}

/** 업무의 일정: 시작일·마감일·마일스톤 여부. 병합과 충돌 확인에서 한 묶음으로 다룬다(`mergeForm`). */
export type TaskSchedule = Pick<TaskInput, 'start_date' | 'due_date' | 'milestone'>;

/**
 * 저장할 때 보내는 일정: 마일스톤에는 시작일이 없고(서버 규칙), 비운 날짜 칸은 없음이다. 폼을 입력으로 바꿀 때와 병합이
 * 같은 함수를 써서, 병합이 고른 일정과 보내는 일정이 어긋나지 않게 한다.
 */
export function scheduleOf(input: TaskSchedule): TaskSchedule {
  return {
    start_date: input.milestone || input.start_date === '' ? null : input.start_date,
    due_date: input.due_date === '' ? null : input.due_date,
    milestone: input.milestone,
  };
}

/**
 * 병합에서 견주고 충돌을 묻는 단위. 일정(시작일·마감일·마일스톤)은 하나다: 칸마다 따로 고르면 아무도 고르지 않은
 * 조합(서버의 시작일과 내 마감일이 뒤바뀌어 저장이 거절되거나, 내 시작일과 서버의 마감일이 한 줄로 보이는 것)이 생긴다.
 * D-94가 일정(이벤트)의 날짜·시각·시간대·종일 여부를 한 묶음으로 처리하라고 한 것을 업무 일정에도 같은 뜻으로 적용한
 * 구현 해석이다.
 */
export type MergeUnit = Exclude<keyof TaskInput, keyof TaskSchedule> | 'schedule';

/**
 * 폼에 놓인 순서(충돌 확인도 이 순서로 보인다): 제목, 상태, 우선순위, 시작일·마감일, 설명, 예상 시간, 태그, 마일스톤,
 * 담당자. 일정 묶음은 그 첫 칸(시작일) 자리에 한 줄로 둔다(마일스톤 칸은 폼에서 태그 뒤에 있지만 이 묶음에 든다).
 */
export const MERGE_UNITS: readonly MergeUnit[] = [
  'title',
  'status',
  'priority',
  'schedule',
  'description',
  'estimate_minutes',
  'tags',
  'assignee_ids',
];

/**
 * 서버처럼 앞뒤 공백을 뗀다. 서버(Rust `str::trim`)는 유니코드 White_Space만 떼므로, JS `trim`과 달리 U+FEFF는 남기고
 * U+0085는 뗀다. 고친 칸 판정과 충돌 확인의 글이 서버에 저장될 값과 어긋나지 않게 한다.
 */
export function trimLikeServer(text: string): string {
  return text.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '');
}

/**
 * 견주는 값: 서버에 저장될 모양. 담당자는 순서 없는 모음이라(서버는 이름순으로 돌려주고 화면은 고른 순서로 덧붙인다)
 * 정렬해 견주고, 일정은 보내는 모양(`scheduleOf`)으로 견준다. 태그는 서버가 받은 순서를 그대로 지키므로 순서까지
 * 견준다. 제목·설명은 서버처럼 앞뒤 공백을 뗀다: 앞뒤 공백만 친 칸은 고친 칸이 아니고(2차 검증), 서버 값과 앞뒤 공백만
 * 다른 내 값은 겹친 것이 아니다(묻는 화면에서 두 값이 똑같아 보인다).
 */
function stored(unit: MergeUnit, input: TaskInput): unknown {
  switch (unit) {
    case 'schedule':
      return scheduleOf(input);
    case 'assignee_ids':
      return [...new Set(input.assignee_ids)].sort();
    case 'title':
    case 'description':
      return trimLikeServer(input[unit]);
    case 'status':
    case 'priority':
    case 'estimate_minutes':
    case 'tags':
      return input[unit];
  }
}

/** 그 단위가 저장될 모양으로 같은지. */
function same(unit: MergeUnit, a: TaskInput, b: TaskInput): boolean {
  return canonicalJson(stored(unit, a)) === canonicalJson(stored(unit, b));
}

/**
 * 기준 값(`before`)에서 고친 단위가 있는지(`mergeForm`과 같은 판정: 앞뒤 공백만 친 글은 고친 것이 아니다). 업무
 * 대화상자가 닫기 전에 저장하지 않은 입력을 버릴지 물을 때 쓴다.
 */
export function edited(before: TaskInput, mine: TaskInput): boolean {
  return MERGE_UNITS.some((unit) => !same(unit, mine, before));
}

/** 단위마다 고른 쪽(`from`)의 값으로 폼 값을 만든다. 일정은 세 칸을 한쪽에서 함께 가져온다. */
function assemble(from: (unit: MergeUnit) => TaskInput): TaskInput {
  return {
    title: from('title').title,
    description: from('description').description,
    status: from('status').status,
    priority: from('priority').priority,
    ...scheduleOf(from('schedule')),
    estimate_minutes: from('estimate_minutes').estimate_minutes,
    tags: from('tags').tags,
    assignee_ids: from('assignee_ids').assignee_ids,
  };
}

/**
 * 서버의 새 값(`fresh`)으로 다시 채우되, 기준 값(`before`, 마지막으로 불러온 판)에서 사용자가 바꾼 단위(`mine`)는
 * 그대로 둔다. 사용자가 바꾼 단위를 서버에서도 누가 다른 값으로 바꿨으면 `clash`이고 그 단위들이 `clashes`(폼 순서):
 * 다시 채울 때는 옛 판 번호로 저장해 충돌 화면(409)이 고르게 하고, 충돌 화면의 "내 입력으로 다시 저장"은 그 단위들을
 * 보이고 덮어쓸지 묻는다(D-94). `form`의 그 단위는 내 값이다. `baseline`은 다음 비교의 기준: 사용자가 바꾸지 않은
 * 단위만 새 값으로 옮긴다. 일정은 한 단위라(`MergeUnit`) 고치지 않았으면 서버의 일정 전체, 나만 바꿨으면 내 일정 전체,
 * 양쪽이 다르게 바꿨으면 충돌 하나로 양쪽 일정 전체를 보인다.
 */
export function mergeForm(
  before: TaskInput,
  mine: TaskInput,
  fresh: TaskInput,
): { form: TaskInput; baseline: TaskInput; clash: boolean; clashes: MergeUnit[] } {
  // 고치지 않았거나(앞뒤 공백만 친 글 포함, 2차 검증) 서버의 새 값과 같아진 단위는 새 값을 따르고, 기준도 새 값으로
  // 옮긴다(그 뒤 사용자가 다시 고쳐도 옛 기준과 비교해 충돌로 보지 않게).
  const takesFresh = (unit: MergeUnit): boolean =>
    same(unit, mine, before) || same(unit, mine, fresh);
  const clashes = MERGE_UNITS.filter((unit) => !takesFresh(unit) && !same(unit, fresh, before));
  // 저장될 값이 새 값과 같은 글은 폼에 친 그대로 둔다(친 중인 뒤 공백을 지워 다음 낱말이 붙지 않게).
  const keepsTyped = (unit: MergeUnit): boolean =>
    (unit === 'title' || unit === 'description') && same(unit, mine, fresh);
  const form = assemble((unit) => (takesFresh(unit) && !keepsTyped(unit) ? fresh : mine));
  const baseline = assemble((unit) => (takesFresh(unit) ? fresh : before));
  return { form, baseline, clash: clashes.length > 0, clashes };
}

/** 서버가 저장하는 예상 시간의 상한(i32). */
const MAX_ESTIMATE = 2_147_483_647;

/**
 * 예상 시간 입력 글자의 값: 빈칸은 없음, 서버 범위(0~i32 상한)의 정수만 값, 그 밖은 잘못된 입력(아주 큰 수는
 * Infinity가 되어 JSON의 null로 보내져 저장된 값을 지웠다).
 */
export function estimateOf(text: string): number | null | 'invalid' {
  const t = text.trim();
  if (t === '') return null;
  if (!/^\d+$/.test(t)) return 'invalid';
  const n = Number.parseInt(t, 10);
  return n <= MAX_ESTIMATE ? n : 'invalid';
}

/**
 * 다시 채울 때의 예상 시간 칸. 예상 시간은 글자로 입력하므로 `mergeForm`과 따로, 입력한 글자의 값으로 판정한다. 값이
 * 기준과 같으면("01"과 1) 고치지 않은 칸이라 서버 값을 쓰고(`keepText` 거짓), 잘못된 입력("abc")은 고친 칸으로 지킨다.
 * 화면 글자와 병합이 같은 판정을 써야 저장할 때 서버의 새 값을 조용히 덮지 않는다.
 */
export function mergeEstimate(
  before: number | null,
  text: string,
  fresh: number | null,
): { keepText: boolean; baseline: number | null; clash: boolean } {
  const typed = estimateOf(text);
  const keepText = typed !== before && typed !== fresh;
  return {
    keepText,
    baseline: keepText ? before : fresh,
    clash: keepText && fresh !== before,
  };
}
