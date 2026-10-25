import type { Module } from '@fvoci/api-client';
import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, screen, userEvent, waitFor, within } from 'storybook/test';

import { NetworkError } from '$lib/api/client';
import { modulePath } from '$lib/router.svelte';

import AppShellFixture from '../fixtures/AppShellFixture.svelte';
import { never, setUpScreen } from '../fixtures/screens';
import {
  EMPTY_MODULE,
  emptyCalendarView,
  emptyHomeView,
  MODULES,
  notificationsView,
  ONE_PER_KIND,
  personName,
  PROJECT_MODULE,
  WORKSPACE,
} from '../fixtures/synthetic';

// 앱 셸: 실제 사이드바(src/lib/features/nav/AppSidebar.svelte)와 머리글, 경로별 화면(홈·모듈)을 App.svelte와 같은 방식으로
// 조립한다(stories/fixtures/AppShellFixture.svelte). 모듈 목록·위키 트리는 사이드바가 같이 쓰는 저장소(workspaceNav)가
// `api`로 받고, story는 그 응답만 바꾼다. 확인은 보이는 글·상태만 한다(배치는 UI 정리 작업이 바꾸는 중이라 보지 않는다).
const meta = {
  title: 'Screens/AppShell',
  component: AppShellFixture,
  parameters: { layout: 'fullscreen' },
  args: {
    workspaces: [WORKSPACE],
    displayName: personName('owner'),
    unreadCount: notificationsView().length,
    onsearch: fn(),
    ondisplay: fn(),
    onexport: fn(),
    onlogout: fn(),
  },
  beforeEach: () => setUpScreen('/'),
} satisfies Meta<typeof AppShellFixture>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 머리글의 사이드바 단추(사이드바 가장자리의 접기 막대도 같은 이름이다). */
function headerToggle(canvasElement: HTMLElement): HTMLElement {
  const header = canvasElement.querySelector('header');
  if (header === null) throw new Error('no header');
  return within(header).getByRole('button', { name: '사이드바 열고 닫기' });
}

/** 펼친 데스크톱 사이드바 또는 열린 서랍. */
const SIDEBAR = '[data-slot="sidebar"][data-state], [data-slot="sidebar"][data-mobile="true"]';

/**
 * 사이드바 안에서 찾는다. 화면 폭과 상관없이 같은 확인을 하려고, 좁은 화면(768px 미만)에서는 닫힌 서랍을 머리글 단추로
 * 먼저 연다. 서랍은 body에 붙고 열린 동안 본문을 가리므로 본문 확인은 이보다 먼저 한다.
 */
async function sidebar(canvasElement: HTMLElement): Promise<HTMLElement> {
  // 이미 열린 서랍은 다시 누르지 않는다(열린 동안 본문은 누를 수 없다).
  if (document.querySelector(SIDEBAR) === null) await userEvent.click(headerToggle(canvasElement));
  const root = await waitFor(() => {
    const found = document.querySelector<HTMLElement>(SIDEBAR);
    if (found === null) throw new Error('no sidebar');
    return found;
  });
  return root;
}

/** 사이드바의 모듈 목록 안 링크(모듈 이름과 키). */
async function moduleLinks(bar: HTMLElement): Promise<HTMLElement[]> {
  return within(await within(bar).findByTestId('sidebar-modules')).getAllByRole('link');
}

/** 많은 모듈: 개발 seed의 모듈 전부(긴 한국어·공백 없는 이름·이모지)에 같은 모양의 합성 모듈을 더 붙인다. */
const MANY: Module[] = [
  ...MODULES,
  ...Array.from({ length: 12 }, (_, i) => ({
    ...PROJECT_MODULE,
    id: `5e1d0000-0000-4000-8000-0009${String(i + 1).padStart(8, '0')}`,
    name: `합성 모듈 ${String(i + 1)}`,
    key: `SYN${String(i + 1)}`,
  })),
];

/** 새 워크스페이스: 모듈·위키 문서가 아직 없고 홈도 비어 있다. */
export const NewWorkspace: Story = {
  beforeEach: () =>
    setUpScreen('/', {
      modules: () => Promise.resolve([]),
      wikiDocuments: () => Promise.resolve([]),
      home: () => Promise.resolve(emptyHomeView()),
      calendar: () => Promise.resolve(emptyCalendarView()),
      notifications: () => Promise.resolve({ notifications: [], next: null }),
    }),
  args: { unreadCount: 0 },
  play: async ({ canvasElement }) => {
    await within(canvasElement).findByText('이번 주까지 마감인 내 업무가 없습니다.');
    const bar = within(await sidebar(canvasElement));
    await bar.findByText('아직 모듈이 없습니다.');
    await expect(bar.queryByTestId('sidebar-modules')).toBeNull();
  },
};

/** 다섯 종류의 모듈(프로젝트·연구·스터디·강의·취미)이 있고 프로젝트 모듈 화면을 연 상태. */
export const AllModuleKinds: Story = {
  beforeEach: () =>
    setUpScreen(modulePath(WORKSPACE.id, PROJECT_MODULE.id), {
      modules: () => Promise.resolve(ONE_PER_KIND),
    }),
  play: async ({ canvasElement }) => {
    await within(canvasElement).findByRole('heading', { level: 1, name: PROJECT_MODULE.name });
    const links = await moduleLinks(await sidebar(canvasElement));
    await expect(links).toHaveLength(ONE_PER_KIND.length);
    for (const [i, mod] of ONE_PER_KIND.entries()) {
      await expect(links[i]).toHaveTextContent(mod.name);
      await expect(links[i]).toHaveTextContent(mod.key);
    }
    const current = links.find((a) => a.getAttribute('aria-current') === 'page');
    await expect(current?.textContent).toContain(PROJECT_MODULE.name);
  },
};

/**
 * 모듈이 많고 이름이 길다(긴 한국어, 공백 없는 문자열, 이모지). 이름 전체는 링크의 title에 있다. 홈 대신 빈 이모지
 * 모듈 화면을 열어 사이드바만 본다(홈의 좁은 화면 넘침은 UI 정리 작업이 고친다).
 */
export const ManyModules: Story = {
  beforeEach: () =>
    setUpScreen(modulePath(WORKSPACE.id, EMPTY_MODULE.id), {
      modules: () => Promise.resolve(MANY),
      calendar: () => Promise.resolve(emptyCalendarView()),
    }),
  play: async ({ canvasElement }) => {
    const links = await moduleLinks(await sidebar(canvasElement));
    await expect(links).toHaveLength(MANY.length);
    for (const mod of MODULES)
      await expect(
        links.some((a) => a.getAttribute('title')?.startsWith(`${mod.name} · `) === true),
      ).toBe(true);
  },
};

/** 불러오는 중: 목록을 받기 전에는 "없음"도 오류도 보이지 않는다. */
export const Loading: Story = {
  beforeEach: () =>
    setUpScreen('/', {
      modules: () => never(),
      wikiDocuments: () => never(),
      home: () => never(),
    }),
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).queryByRole('alert')).toBeNull();
    const bar = within(await sidebar(canvasElement));
    await bar.findByRole('link', { name: '위키' });
    await expect(bar.queryByText('아직 모듈이 없습니다.')).toBeNull();
    await expect(bar.queryByTestId('sidebar-modules')).toBeNull();
    await expect(bar.queryByRole('alert')).toBeNull();
  },
};

/** 목록을 불러오지 못함: 모듈 목록에 오류와 다시 시도. 다시 시도하면 받은 목록을 보인다. */
let moduleCalls = 0;
export const LoadFailed: Story = {
  beforeEach: () => {
    moduleCalls = 0;
    return setUpScreen('/', {
      modules: () => {
        moduleCalls += 1;
        return moduleCalls === 1
          ? Promise.reject(new NetworkError('offline'))
          : Promise.resolve(ONE_PER_KIND);
      },
      wikiDocuments: () => Promise.reject(new NetworkError('offline')),
    });
  },
  play: async ({ canvasElement }) => {
    const bar = within(await sidebar(canvasElement));
    const alert = await bar.findByRole('alert');
    await expect(alert).toHaveTextContent('목록을 불러오지 못했습니다.');
    await expect(bar.queryByTestId('sidebar-modules')).toBeNull();
    // 모듈 묶음의 다시 시도(위키 트리는 접혀 있어 그 오류는 펼쳐야 보인다).
    await userEvent.click(within(alert).getByRole('button', { name: '다시 시도' }));
    await expect(await moduleLinks(await sidebar(canvasElement))).toHaveLength(ONE_PER_KIND.length);
    await expect(moduleCalls).toBe(2);
  },
};

/** 계정 메뉴: 표시 이름과 실행 중인 빌드(/healthz의 합성 응답). */
export const AccountMenu: Story = {
  play: async ({ canvasElement }) => {
    const bar = within(await sidebar(canvasElement));
    await userEvent.click(bar.getByRole('button', { name: `계정 메뉴: ${personName('owner')}` }));
    await screen.findByText('fvoci 개발 빌드(dev)');
  },
};

/** 데스크톱에서 사이드바를 접은 상태(아이콘 막대). 머리글의 단추로 다시 펼친다. */
export const DesktopCollapsed: Story = {
  args: { sidebarOpen: false },
  globals: { viewport: { value: 'desktop', isRotated: false } },
  play: async ({ canvasElement }) => {
    const root = canvasElement.querySelector('[data-slot="sidebar"][data-state]');
    // 좁은 화면(768px 미만)에는 접힌 막대가 없다(서랍). 이 story는 데스크톱 폭으로 고정한다.
    if (root === null) return;
    await expect(root.getAttribute('data-state')).toBe('collapsed');
    await userEvent.click(headerToggle(canvasElement));
    await waitFor(() => expect(root.getAttribute('data-state')).toBe('expanded'));
  },
};

/** 좁은 화면(375px): 사이드바는 서랍이다. 머리글의 단추로 열면 모듈 목록이 보인다. */
export const Narrow375: Story = {
  globals: { viewport: { value: 'narrow375', isRotated: false } },
  play: async ({ canvasElement }) => {
    await expect(await moduleLinks(await sidebar(canvasElement))).toHaveLength(MODULES.length);
  },
};

/** 500px: 아직 모바일 폭(768px 미만)이라 서랍이다. */
export const Narrow500: Story = {
  ...Narrow375,
  globals: { viewport: { value: 'narrow500', isRotated: false } },
};
