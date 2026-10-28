import { api } from '$lib/api/client';
import { eventsApi } from '$lib/features/events/api';
import type { ModuleKind } from '@fvoci/api-client';
import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, waitFor, within } from 'storybook/test';

import type { ItemView, RecordKindView } from '$lib/api/client';

import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { me } from '$lib/me.svelte';
import type { ItemGroupName } from '$lib/router.svelte';
import { itemGroupPath, router } from '$lib/router.svelte';

import MainFrame from '../fixtures/MainFrame.svelte';
import { ACTIVITY } from '../fixtures/personal';
import { DOCS, itemsOf, ONE_PER_KIND, TASK_ITEM, WORKSPACE } from '../fixtures/synthetic';
import { setUpTime } from '../time/setup';

// 항목 화면(src/lib/features/modules/ItemScreen.svelte)의 묶음(D-121, 사용자 R7 A): 종류별 첫 묶음(연구 문헌, 스터디 계획,
// 강의 회차, 취미 활동) 하나 뒤에 공통 업무·문서·일정·기록. 고른 묶음만 마운트한다. 묶음 단추는 좁은 화면에서 줄바꿈한다.
// 묶음마다 기본 story를 두고, 어려운 조건(375px·어두움·200% 글꼴)을 합친 story는 하나만 둔다(D-108 개정 2026-10-27).
// 서버 함수는 합성 데이터로 답한다(preview.ts의 요청 차단).

function moduleOf(kind: ModuleKind) {
  const found = ONE_PER_KIND.find((mod) => mod.kind === kind);
  if (found === undefined) throw new RangeError(`no ${kind} module`);
  return found;
}

function itemOf(kind: ModuleKind): ItemView {
  if (kind === 'project') return TASK_ITEM;
  const first = itemsOf(moduleOf(kind))[0];
  if (first === undefined) throw new RangeError(`no ${kind} item`);
  return first;
}

function kindDef(kind: string, label: string, moduleKind: ModuleKind): RecordKindView {
  return { kind, label, module_kind: moduleKind, date_label: null, fields: [], file_roles: [] };
}

const RECORD_KINDS: RecordKindView[] = [
  kindDef('risk', '위험', 'project'),
  kindDef('retrospective', '회고', 'project'),
  kindDef('reference', '문헌', 'research'),
  kindDef('dataset', '데이터', 'research'),
  kindDef('material', '자료', 'study'),
  kindDef('question', '질문', 'course'),
  kindDef('activity', '활동', 'hobby'),
  kindDef('supply', '재료', 'hobby'),
];

// 묶음의 대표 제목(그 묶음이 마운트됐는지 본다).
const GROUP_HEADING: Record<ItemGroupName, string> = {
  tasks: 'tasks-heading',
  docs: 'item-docs-heading',
  events: 'events-heading',
  records: 'attachments-heading',
  references: 'records-heading',
  plan: 'study-heading',
  episodes: 'episodes-heading',
  activities: 'records-heading',
};

const GROUP_LABEL: Record<ItemGroupName, string> = {
  tasks: '업무',
  docs: '문서',
  events: '일정',
  records: '기록',
  references: '문헌',
  plan: '계획',
  episodes: '회차',
  activities: '활동',
};

interface Args {
  kind: ModuleKind;
  group: ItemGroupName;
}

const meta = {
  title: 'Screens/ItemScreen',
  component: ItemScreen,
  args: { workspace: WORKSPACE, itemId: TASK_ITEM.id, workspaces: [WORKSPACE] },
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
} satisfies Meta<typeof ItemScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 이 종류 항목을 그 묶음 경로로 연다. 경로는 라우터가 보는 값만 바꾼다(iframe 주소는 그대로). */
function groupStory({ kind, group }: Args, globals: Story['globals'] = {}): Story {
  const it = itemOf(kind);
  return {
    args: { itemId: it.id },
    globals,
    beforeEach: async () => {
      const previousUser = me.userId;
      me.userId = 'story-item-user';
      const restore = await setUpTime();
      const all = [TASK_ITEM, ...ONE_PER_KIND.flatMap((mod) => itemsOf(mod))];
      spyOn(api, 'item').mockImplementation((id) =>
        Promise.resolve(all.find((candidate) => candidate.id === id) ?? it),
      );
      spyOn(api, 'itemDocuments').mockResolvedValue(DOCS.slice(0, 3));
      spyOn(api, 'documentTemplates').mockResolvedValue([]);
      spyOn(api, 'itemProfile').mockResolvedValue({
        leader: null,
        goal: '정확한 권한 표시',
        scope: '공유와 공개 링크',
        starts_on: null,
        ends_on: null,
        status: 'active',
        details: {},
        members: [],
        groups: [],
        can_edit: true,
        can_manage_members: false,
        version: 1,
      });
      spyOn(api, 'groups').mockResolvedValue([]);
      spyOn(api, 'recordKinds').mockResolvedValue(RECORD_KINDS);
      spyOn(api, 'itemRecords').mockResolvedValue([]);
      spyOn(api, 'itemFiles').mockResolvedValue([]);
      spyOn(eventsApi, 'itemEvents').mockResolvedValue([]);
      spyOn(api, 'itemBacklinks').mockResolvedValue([]);
      spyOn(api, 'mark').mockResolvedValue({ pin: false, favorite: false });
      spyOn(api, 'targetActivity').mockResolvedValue({
        activity: ACTIVITY,
        next_before: null,
        next_before_id: null,
      });
      const before = router.path;
      router.path = itemGroupPath(WORKSPACE.id, it.id, group);
      return async () => {
        router.path = before;
        me.userId = previousUser;
        await restore();
      };
    },
    play: async ({ canvasElement }) => {
      const canvas = within(canvasElement);
      const groups = await canvas.findByRole('navigation', { name: '항목 묶음' });
      await expect(within(groups).getByRole('link', { name: GROUP_LABEL[group] })).toHaveAttribute(
        'aria-current',
        'page',
      );
      await waitFor(() =>
        expect(canvasElement.querySelector(`#${GROUP_HEADING[group]}`)).not.toBeNull(),
      );
      // 고른 묶음만 마운트한다(기록 묶음에도 기록 패널이 있다).
      const shown = new Set([
        GROUP_HEADING[group],
        ...(group === 'records' ? ['records-heading'] : []),
      ]);
      for (const id of new Set(Object.values(GROUP_HEADING))) {
        if (shown.has(id)) continue;
        await expect(canvasElement.querySelector(`#${id}`)).toBeNull();
      }
    },
  };
}

const NARROW_DARK_LARGE_TEXT: Story['globals'] = {
  theme: 'dark',
  fontScale: '200',
  viewport: { value: 'narrow375' },
};

export const ProjectTasks = groupStory({ kind: 'project', group: 'tasks' });
export const ProjectTasksNarrowDarkLargeText = groupStory(
  { kind: 'project', group: 'tasks' },
  NARROW_DARK_LARGE_TEXT,
);
export const ProjectDocs = groupStory({ kind: 'project', group: 'docs' });
export const ProjectEvents = groupStory({ kind: 'project', group: 'events' });
export const ProjectRecords = groupStory({ kind: 'project', group: 'records' });
export const ResearchReferences = groupStory({ kind: 'research', group: 'references' });
export const StudyPlan = groupStory({ kind: 'study', group: 'plan' });
export const CourseEpisodes = groupStory({ kind: 'course', group: 'episodes' });
export const HobbyActivities = groupStory({ kind: 'hobby', group: 'activities' });

/** 묶음 단추를 키보드로 고른다(Tab으로 닿고 Enter로 연다). 고른 묶음만 남는다. */
export const SwitchWithKeyboard: Story = {
  ...groupStory({ kind: 'project', group: 'tasks' }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const groups = await canvas.findByRole('navigation', { name: '항목 묶음' });
    const links = within(groups).getAllByRole('link');
    const docs = within(groups).getByRole('link', { name: '문서' });
    // 첫 묶음 링크에서 Tab으로 "문서"까지 간다(초점을 바로 주지 않는다, R5 B 최종 FC-10).
    links[0]?.focus();
    for (let i = 0; i < links.length && document.activeElement !== docs; i += 1)
      await userEvent.tab();
    await expect(docs).toHaveFocus();
    await userEvent.keyboard('{Enter}');
    await expect(docs).toHaveAttribute('aria-current', 'page');
    await waitFor(() => expect(canvasElement.querySelector('#item-docs-heading')).not.toBeNull());
    await expect(canvasElement.querySelector('#tasks-heading')).toBeNull();
  },
};
