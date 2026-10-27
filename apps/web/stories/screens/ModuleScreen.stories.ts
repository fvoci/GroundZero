import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, within, userEvent, screen } from 'storybook/test';

import { ApiError, NetworkError } from '$lib/api/client';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import { moduleCalendarPath, modulePath, router, workspacePath } from '$lib/router.svelte';
import { me } from '$lib/me.svelte';
import { formDraftTabKey, writeFormDraftAt } from '$lib/features/common/form-draft';
import { m } from '$lib/paraglide/messages.js';

import MainFrame from '../fixtures/MainFrame.svelte';
import { never, setUpScreen } from '../fixtures/screens';
import {
  EMPTY_MODULE,
  emptyCalendarView,
  itemsOf,
  PROJECT_MODULE,
  WORKSPACE,
  workspaceAs,
} from '../fixtures/synthetic';

// 모듈 화면(src/lib/features/modules/ModuleScreen.svelte): 설명, 항목 목록과 만들기, 모듈 달력, 관리자 설정. `api.module`·
// `api.items`와 모듈 범위의 `api.calendar`를 부른다. 모듈 달력은 실제 오늘이 든 달을 보이므로(제품 동작) 그 칸은 확인하지
// 않는다. 확인은 보이는 글·권한별 조작·오류만 한다.
const meta = {
  title: 'Screens/ModuleScreen',
  component: ModuleScreen,
  args: { workspace: WORKSPACE, moduleId: PROJECT_MODULE.id },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => {
    const previousUser = me.userId;
    me.userId = 'story-module-user';
    const cleanup = setUpScreen(modulePath(WORKSPACE.id, PROJECT_MODULE.id));
    return () => {
      me.userId = previousUser;
      cleanup();
    };
  },
} satisfies Meta<typeof ModuleScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 프로젝트 모듈과 항목(상태·기간·긴 제목·이모지). 관리자는 만들기와 모듈 설정을 본다. */
export const ProjectWithItems: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole('heading', { level: 1, name: PROJECT_MODULE.name });
    const list = canvas.getByTestId('item-list');
    const items = itemsOf(PROJECT_MODULE);
    await expect(within(list).getAllByRole('link')).toHaveLength(items.length);
    for (const it of items) await expect(list).toHaveTextContent(it.title);
    await expect(canvas.getByLabelText('새 항목 제목')).toBeEnabled();
    await expect(canvas.getByRole('form', { name: '모듈 설정' })).toBeInTheDocument();
  },
};

/** 항목이 없는 모듈(이모지 이름). */
export const EmptyModule: Story = {
  args: { moduleId: EMPTY_MODULE.id },
  beforeEach: () =>
    setUpScreen(modulePath(WORKSPACE.id, EMPTY_MODULE.id), {
      calendar: () => Promise.resolve(emptyCalendarView()),
    }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('볼 수 있는 항목이 없습니다.');
    await expect(canvas.queryByTestId('item-list')).toBeNull();
  },
};

/** 읽기 전용(워크스페이스 읽기 역할): 항목은 보이지만 만들기·설정·지우기가 없다. */
export const ReadOnly: Story = {
  args: { workspace: workspaceAs('readonly') },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByTestId('item-list');
    await expect(canvas.queryByLabelText('새 항목 제목')).toBeNull();
    await expect(canvas.queryByRole('form', { name: '모듈 설정' })).toBeNull();
    await expect(canvas.queryByRole('button', { name: /지우기/ })).toBeNull();
  },
};

export const Loading: Story = {
  beforeEach: () =>
    setUpScreen(modulePath(WORKSPACE.id, PROJECT_MODULE.id), { module: () => never() }),
  play: async ({ canvasElement }) => {
    await within(canvasElement).findByText('불러오는 중…');
  },
};

/** 없거나 볼 수 없는 모듈(404). */
export const NotFound: Story = {
  beforeEach: () =>
    setUpScreen(modulePath(WORKSPACE.id, PROJECT_MODULE.id), {
      module: () => Promise.reject(new ApiError(404, 'not_found', 'module not found')),
    }),
  play: async ({ canvasElement }) => {
    const alert = await within(canvasElement).findByRole('alert');
    await expect(alert).toHaveTextContent('모듈이 없거나 볼 권한이 없습니다.');
  },
};

export const LoadFailed: Story = {
  beforeEach: () =>
    setUpScreen(modulePath(WORKSPACE.id, PROJECT_MODULE.id), {
      items: () => Promise.reject(new NetworkError('offline')),
    }),
  play: async ({ canvasElement }) => {
    const alert = await within(canvasElement).findByRole('alert');
    await expect(alert).toHaveTextContent('모듈을 불러오지 못했습니다.');
  },
};

/** Settings-only edits use the same departure confirmation as the other editors. */
export const UnsavedSettings: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const cleanup = setUpScreen(modulePath(WORKSPACE.id, PROJECT_MODULE.id));
    const previousUser = me.userId;
    me.userId = 'story-module-draft';
    const key = formDraftTabKey(`module-settings:${PROJECT_MODULE.id}`);
    writeFormDraftAt(key, undefined);
    return () => {
      writeFormDraftAt(key, undefined);
      me.userId = previousUser;
      cleanup();
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.type(await canvas.findByLabelText('설명'), ' 보존할 설정 입력');
    router.navigate(workspacePath(WORKSPACE.id));
    await expect(await screen.findByRole('alertdialog')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
    await expect(canvas.getByLabelText('설명')).toHaveValue(
      `${PROJECT_MODULE.description} 보존할 설정 입력`,
    );
  },
};

export const UnsavedSettingsDarkNarrow200: Story = {
  ...UnsavedSettings,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};

/** 달력 보기(`/m/:mod/calendar`, D-121). 달력은 이 보기에서만 마운트해 조회하고 항목 목록은 보이지 않는다. */
export const CalendarView: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const before = router.path;
    router.path = moduleCalendarPath(WORKSPACE.id, PROJECT_MODULE.id);
    return () => {
      router.path = before;
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const views = await canvas.findByRole('navigation', { name: '모듈 보기' });
    await expect(within(views).getByRole('link', { name: '달력' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    await expect(canvasElement.querySelector('#calendar-heading')).not.toBeNull();
    await expect(canvas.queryByTestId('item-list')).toBeNull();
  },
};

export const CalendarViewDark: Story = { ...CalendarView, globals: { theme: 'dark' } };
export const CalendarViewNarrow375: Story = {
  ...CalendarView,
  globals: { viewport: { value: 'narrow375' } },
};
export const CalendarViewZoom200: Story = {
  ...CalendarView,
  globals: { fontScale: '200', viewport: { value: 'narrow375' } },
};
