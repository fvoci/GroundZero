// 브라우저 시험 공통 도구. 실제 Keycloak 화면으로 로그인하고, 같은 쿠키를 쓰는 요청으로 합성 데이터를 만들거나
// 서버 상태를 확인한다. HTTP를 대신 응답(mock)하지 않는다.
//
// 실패 증거: 로그인 뒤에만 trace를 켜서(아이디·비밀번호 입력과 인증 교환은 기록하지 않는다) 실패한 시험의 trace와
// 화면을 원시 출력 폴더(FVOCI_BROWSER_RAW)에 남긴다. 쿠키 값은 같은 폴더의 sensitive.txt에 적어, 공개 단계
// (e2e/publish.ts)가 그 값이 공개 파일에 없는지 검사하게 한다. 원시 출력은 올리지 않는다.
import { appendFileSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';

import {
  type APIRequestContext,
  type Browser,
  type BrowserContext,
  type Page,
  type TestInfo,
  expect,
  test as base,
} from '@playwright/test';

export { expect };

const raw = process.env.FVOCI_BROWSER_RAW ?? '';
const realmFile = path.resolve(import.meta.dirname, '../../../deploy/keycloak/realm-test.json');

export type UserName = 'test-user' | 'test-user-2';

export interface TestUser {
  username: UserName;
  email: string;
  password: string;
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** 응답 객체의 문자열 칸. 없으면 시험 실패. */
export function str(v: unknown, key: string): string {
  if (!isRecord(v) || typeof v[key] !== 'string') throw new Error(`missing string ${key}`);
  return v[key];
}

/** 날짜처럼 저장 전 null일 수 있는 계약 칸. 잘못된 타입은 계속 거절한다. */
export function nullableStr(v: unknown, key: string): string | null {
  if (isRecord(v) && v[key] === null) return null;
  return str(v, key);
}

export function listField(v: unknown, key: string): unknown[] {
  if (!isRecord(v)) throw new Error(`missing object for ${key}`);
  return list(v[key]);
}

/** 응답 객체의 숫자 칸. 없으면 시험 실패. */
export function num(v: unknown, key: string): number {
  if (!isRecord(v) || typeof v[key] !== 'number') throw new Error(`missing number ${key}`);
  return v[key];
}

/** 응답 목록. 목록이 아니면 시험 실패. */
export function list(v: unknown): unknown[] {
  if (!Array.isArray(v)) throw new Error('expected a list');
  return v;
}

/** deploy/keycloak/realm-test.json의 명백한 테스트 계정. 비밀번호를 코드에 다시 적지 않는다. */
export function testUser(username: UserName): TestUser {
  const realm: unknown = JSON.parse(readFileSync(realmFile, 'utf8'));
  const users = isRecord(realm) ? list(realm.users) : [];
  const user = users.find((u) => isRecord(u) && u.username === username);
  const credentials = isRecord(user) ? list(user.credentials) : [];
  const password = credentials.find((c) => isRecord(c) && c.type === 'password');
  return { username, email: str(user, 'email'), password: str(password, 'value') };
}

/** 실행마다 다른 합성 식별자. 다른 실행·시험의 데이터를 찾거나 지우지 않는다. */
export function unique(): string {
  return crypto.randomUUID().slice(0, 8);
}

/** 원시 출력 폴더의 민감 값 목록에 더한다. 공개 단계가 이 값이 들어간 파일을 거절한다. */
export function recordSensitive(values: readonly string[]): void {
  if (raw === '') return;
  mkdirSync(raw, { recursive: true });
  const lines = values.filter((v) => v.length >= 8);
  if (lines.length > 0) appendFileSync(path.join(raw, 'sensitive.txt'), `${lines.join('\n')}\n`);
}

/** 로그인한 사용자의 쿠키로 서버를 직접 부르는 도구(합성 데이터 준비와 서버 상태 확인). */
export class Api {
  constructor(
    private readonly request: APIRequestContext,
    private readonly origin: string,
  ) {}

  async get(url: string): Promise<unknown> {
    const res = await this.request.get(url);
    expect(res.status(), `GET ${url}`).toBe(200);
    const body: unknown = await res.json();
    return body;
  }

  /** 상태를 바꾸는 요청. 서버의 Origin 검사(CSRF)를 브라우저처럼 통과하게 원점을 붙인다. */
  async send(method: 'POST' | 'PUT', url: string, data: unknown): Promise<unknown> {
    const res = await this.request.fetch(url, {
      method,
      data,
      headers: { origin: this.origin },
    });
    expect(res.ok(), `${method} ${url} -> ${String(res.status())}`).toBe(true);
    if (res.status() === 204) return undefined;
    const body: unknown = await res.json();
    return body;
  }

  async status(url: string): Promise<number> {
    return (await this.request.get(url)).status();
  }

  /** 격리 browser 환경의 최초 관리자 조직. 여러 조직이면 임의 선택하지 않고 준비를 실패시킨다. */
  async initialOrganization(): Promise<{ id: string; name: string }> {
    const organizations = list(await this.get('/api/me/organizations'));
    expect(
      organizations,
      'isolated initial-admin fixture has exactly one organization',
    ).toHaveLength(1);
    const organization = organizations[0];
    return { id: str(organization, 'id'), name: str(organization, 'name') };
  }

  async createWorkspace(name: string): Promise<string> {
    const organization = await this.initialOrganization();
    return str(
      await this.send('POST', '/api/workspaces', {
        request_id: crypto.randomUUID(),
        organization_id: organization.id,
        name,
      }),
      'id',
    );
  }

  /** 업무가 있는 프로젝트 모듈과 그 안의 항목 하나를 만든다. */
  async createProjectItem(workspace: string, tag: string): Promise<string> {
    const module = await this.send('POST', `/api/workspaces/${workspace}/modules`, {
      request_id: crypto.randomUUID(),
      kind: 'project',
      name: `시험 프로젝트 ${tag}`,
      key: `T${tag
        .replace(/[^a-z0-9]/gi, '')
        .slice(0, 5)
        .toUpperCase()}`,
    });
    const item = await this.send('POST', `/api/modules/${str(module, 'id')}/items`, {
      request_id: crypto.randomUUID(),
      title: `시험 항목 ${tag}`,
    });
    return str(item, 'id');
  }

  async createWikiDocument(workspace: string, title: string): Promise<string> {
    const doc = await this.send('POST', `/api/workspaces/${workspace}/wiki/documents`, {
      request_id: crypto.randomUUID(),
      title,
      parent_id: null,
      template_id: null,
    });
    return str(doc, 'id');
  }

  async setRealtime(workspace: string, enabled: boolean): Promise<void> {
    await this.send('PUT', `/api/workspaces/${workspace}/realtime`, { enabled });
  }

  /** 관리자가 이메일로 초대한다. 받는 사람은 그 이메일이 확인된 계정으로 로그인해 받아들인다. */
  async invite(
    workspace: string,
    email: string,
    role: 'admin' | 'member' | 'readonly',
  ): Promise<void> {
    await this.send('POST', `/api/workspaces/${workspace}/invitations`, { email, role });
  }

  /** 이 워크스페이스에서 받은 초대를 받아들인다. */
  async acceptInvitation(workspace: string): Promise<void> {
    const invitation = list(await this.get('/api/invitations')).find(
      (i) => str(i, 'kind') === 'workspace' && str(i, 'workspace_id') === workspace,
    );
    const accepted = await this.send(
      'POST',
      `/api/invitations/${str(invitation, 'id')}/accept`,
      {},
    );
    expect(str(accepted, 'kind')).toBe('workspace');
    expect(str(accepted, 'workspace_id')).toBe(workspace);
  }

  /** test-user-2는 최초에 조직이 없다. 조직 초대 수락 뒤에만 공간 초대를 준비한다. */
  async ensureOrganizationMember(member: Api, email: string): Promise<void> {
    const organization = await this.initialOrganization();
    const memberships = list(await member.get('/api/me/organizations'));
    if (memberships.some((o) => str(o, 'id') === organization.id)) return;
    await this.send('POST', `/api/organizations/${organization.id}/invitations`, {
      email,
      role: 'member',
    });
    const invitation = list(await member.get('/api/invitations')).find(
      (i) => str(i, 'kind') === 'organization' && str(i, 'organization_id') === organization.id,
    );
    const accepted = await member.send(
      'POST',
      `/api/invitations/${str(invitation, 'id')}/accept`,
      {},
    );
    expect(str(accepted, 'kind')).toBe('organization');
    expect(str(accepted, 'organization_id')).toBe(organization.id);
    expect(
      list(await member.get('/api/me/organizations')).some((o) => str(o, 'id') === organization.id),
    ).toBe(true);
  }

  async createTask(item: string, title: string, fields: Record<string, unknown> = {}) {
    return this.send('POST', `/api/items/${item}/tasks`, {
      ...fields,
      request_id: crypto.randomUUID(),
      title,
      parent_id: null,
    });
  }
}

export interface Session {
  context: BrowserContext;
  page: Page;
  api: Api;
  user: TestUser;
  /**
   * 인증 이동(로그아웃·다시 로그인) 전에 trace를 끝낸다. 로그아웃 주소의 id_token_hint와 다시 받는 로그인 화면이
   * trace에 들어가지 않는다. 그때까지의 기록은 시험이 실패하면 남는다.
   */
  endTrace: () => Promise<void>;
}

/** 개인 화면은 저장소 주입 없이 실제 전환기로 조직을 고른다. 좁은 화면에서는 서랍을 연다. */
export async function selectOrganization(
  page: Page,
  organization: { id: string; name: string },
): Promise<void> {
  const switcher = page.getByTestId('organization-switcher');
  const narrow = (page.viewportSize()?.width ?? 1280) < 768;
  if (narrow) await page.getByRole('button', { name: '사이드바 열고 닫기' }).click();
  await switcher.click();
  await page.getByRole('menuitem').filter({ hasText: organization.name }).click();
  await expect(switcher).toContainText(organization.name);
  if (narrow) await page.keyboard.press('Escape');
}

/** 실패 증거를 남길 브라우저 컨텍스트(이름은 증거 파일 이름에 쓴다). */
interface Tracked {
  name: string;
  context: BrowserContext;
  page: Page;
  tracing: boolean;
  /** 시험 도중 끝낸 trace(실패했을 때만 남긴다). */
  savedTrace?: string;
}

/** 실제 Keycloak 화면으로 로그인한다. 아이디·비밀번호 입력은 trace를 켜기 전이다. */
export async function logIn(page: Page, user: TestUser): Promise<void> {
  await page.goto('/');
  const origin = new URL(page.url()).origin;
  await page.getByRole('link', { name: '로그인' }).click();
  await page.locator('#username').fill(user.username);
  await page.locator('#password').fill(user.password);
  await page.locator('#kc-login').click();
  await page.waitForURL((url) => url.origin === origin);
  await expect.poll(async () => (await page.request.get('/api/me')).status()).toBe(200);
}

export interface SignInOptions {
  /** trace를 켜기 전에 컨텍스트에 할 일(실패 증거 확인 시험이 합성 쿠키를 넣는다). */
  beforeTrace?: (context: BrowserContext) => Promise<void>;
}

type SignIn = (username: UserName, options?: SignInOptions) => Promise<Session>;

const TRACE_OPTIONS = { screenshots: true, snapshots: true, sources: false };

function tracePath(testInfo: TestInfo, name: string): string {
  const slug = `${testInfo.project.name}-${testInfo.titlePath.slice(1).join('-')}`
    .replace(/[^\p{L}\p{N}_-]+/gu, '-')
    .slice(0, 120);
  return path.join(raw, 'traces', `${slug}-${name}.zip`);
}

/**
 * 실패한 시험의 화면·trace를 남기고 컨텍스트를 닫는다. 증거를 남기지 못한 것은 숨기지 않고 시험 오류로 더한다
 * (원래 실패는 그대로 먼저 남는다).
 */
async function finish(sessions: Tracked[], testInfo: TestInfo): Promise<void> {
  const failed = testInfo.status !== testInfo.expectedStatus;
  const problems: string[] = [];
  const attempt = async (what: string, work: () => Promise<unknown>): Promise<void> => {
    try {
      await work();
    } catch (e: unknown) {
      problems.push(`${what}: ${e instanceof Error ? e.message : String(e)}`);
    }
  };
  for (const [i, s] of sessions.entries()) {
    const name = `${s.name}-${String(i)}`;
    if (failed) {
      // 화면은 마지막 상태 그대로. 실패 원인 확인용이다.
      await attempt(`screenshot ${name}`, () =>
        s.page.screenshot({ path: testInfo.outputPath(`${name}.png`), fullPage: true }),
      );
      if (s.tracing) {
        await attempt(`trace ${name}`, () =>
          s.context.tracing.stop({ path: tracePath(testInfo, name) }),
        );
      }
    } else {
      if (s.tracing) await attempt(`trace ${name}`, () => s.context.tracing.stop());
      if (s.savedTrace !== undefined) rmSync(s.savedTrace, { force: true });
    }
    await attempt(`close ${name}`, () => s.context.close());
  }
  if (problems.length > 0) throw new Error(`failure evidence incomplete: ${problems.join('; ')}`);
}

async function signIn(
  browser: Browser,
  username: UserName,
  testInfo: TestInfo,
  sessions: Tracked[],
  options: SignInOptions,
): Promise<Session> {
  // 설정의 use(baseURL·viewport·locale 등)는 시험 실행기가 새 컨텍스트에도 넣는다(Playwright의 기본 동작).
  const context = await browser.newContext();
  const page = await context.newPage();
  const user = testUser(username);
  const origin = new URL(testInfo.project.use.baseURL ?? '').origin;
  const tracked: Tracked = { name: username, context, page, tracing: false };
  sessions.push(tracked);
  await logIn(page, user);
  await options.beforeTrace?.(context);
  recordSensitive((await context.cookies()).map((c) => c.value));
  await context.tracing.start({ title: `${testInfo.title} (${username})`, ...TRACE_OPTIONS });
  tracked.tracing = true;
  const endTrace = async (): Promise<void> => {
    if (!tracked.tracing) return;
    tracked.tracing = false;
    const file = tracePath(
      testInfo,
      `${username}-${String(sessions.indexOf(tracked))}-before-auth`,
    );
    await context.tracing.stop({ path: file });
    tracked.savedTrace = file;
  };
  return { context, page, api: new Api(context.request, origin), user, endTrace };
}

/** 로그인하지 않은 브라우저. 처음부터 trace를 켠다(인증 정보가 없다). */
async function anonymous(browser: Browser, testInfo: TestInfo, sessions: Tracked[]): Promise<Page> {
  const context = await browser.newContext();
  const page = await context.newPage();
  sessions.push({ name: 'anonymous', context, page, tracing: true });
  await context.tracing.start({ title: `${testInfo.title} (anonymous)`, ...TRACE_OPTIONS });
  return page;
}

export const test = base.extend<{
  tracked: Tracked[];
  signIn: SignIn;
  anonymous: () => Promise<Page>;
}>({
  // Playwright는 고정 도구의 의존을 첫 인자의 구조 분해로 읽으므로, 의존이 없어도 빈 패턴을 써야 한다.
  // eslint-disable-next-line no-empty-pattern -- Playwright fixture signature
  tracked: async ({}, use, testInfo) => {
    const sessions: Tracked[] = [];
    await use(sessions);
    await finish(sessions, testInfo);
  },
  signIn: async ({ browser, tracked }, use, testInfo) => {
    await use((username, options = {}) => signIn(browser, username, testInfo, tracked, options));
  },
  anonymous: async ({ browser, tracked }, use, testInfo) => {
    await use(() => anonymous(browser, testInfo, tracked));
  },
});
