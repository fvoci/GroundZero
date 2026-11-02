<script lang="ts">
  import { activityApi } from '$lib/features/activity/api';
  import { taskScreenApi, type TaskContext } from '$lib/features/tasks/screen-api';
  import { startTaskReads, type TaskStartupSeed } from '$lib/features/tasks/startup-reads';
  import {
    createContextRead,
    readTaskSessionGeneration,
    trackTaskSessionPublications,
    type ContextCapture,
  } from '$lib/features/tasks/context-read';
  import {
    createTaskSessionRecovery,
    TASK_SESSION_RECOVERY,
    type TaskSessionProof,
  } from '$lib/features/tasks/session-recovery';
  // 앱 셸: 로그인 확인 → 사이드바(워크스페이스) + 경로별 화면.
  import type { Me, Workspace } from '@fvoci/api-client';

  import { api, ApiError, NetworkError } from '$lib/api/client';
  import { SessionListMismatch } from '$lib/shared/api';
  import { errorText } from '$lib/api/errors';
  import LoginScreen from '$lib/features/auth/LoginScreen.svelte';
  import {
    SESSION_CHECK_MAX_ATTEMPTS,
    SESSION_SIGNAL_RECHECK_MS,
  } from '$lib/features/auth/session-check';
  import { createSessionNotificationRetry } from '$lib/features/auth/session-notification-retry';
  import {
    isSessionSignal,
    publishLogoutPending,
    publishLogoutResult,
    publishSessionCheck,
    readLogoutPending,
    readSessionGeneration,
  } from '$lib/features/auth/session-signal';
  import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
  import NotesScreen from '$lib/features/notes/NotesScreen.svelte';
  import SharedScreen from '$lib/features/notes/SharedScreen.svelte';
  import PublicView from '$lib/features/public/PublicView.svelte';
  import {
    attachRecordDraftCleanup,
    clearLegacyRecordDrafts,
    discardLegacyRecordDraft,
    legacyRecordDrafts,
    type LegacyRecordDraft,
  } from '$lib/features/records/legacy-drafts';
  import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
  import TrashScreen from '$lib/features/trash/TrashScreen.svelte';
  import AuditScreen from '$lib/features/audit/AuditScreen.svelte';
  import DeletedWorkspaces from '$lib/features/trash/DeletedWorkspaces.svelte';
  import MembersScreen from '$lib/features/members/MembersScreen.svelte';
  import OrganizationScreen from '$lib/features/organizations/OrganizationScreen.svelte';
  import ServerAdminScreen from '$lib/features/server-admin/ServerAdminScreen.svelte';
  import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
  import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
  import SearchDialog from '$lib/features/search/SearchDialog.svelte';
  import { displaySettings } from '$lib/display';
  import { m } from '$lib/paraglide/messages.js';
  import { setLocale } from '$lib/paraglide/runtime.js';
  import DisplaySettingsDialog from '$lib/features/settings/DisplaySettingsDialog.svelte';
  import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
  // 계정 전환은 입력을 보존하고(D-96), 로그아웃은 비운다. 둘 다 다시 읽기 전까지 기존 화면을 가린다.
  import {
    accountSafety,
    auth,
    connection,
    hideSession,
    reportSessionExpired,
    switchAccount,
  } from '$lib/features/feedback/feedback.svelte';
  import NotificationsScreen from '$lib/features/references/NotificationsScreen.svelte';
  // 홈에서 초대를 처리한 경우도 서버 미독 수를 즉시 다시 읽는다(D-118).
  import { invitationChanges } from '$lib/features/home/invitations.svelte';
  import TimeScreen from '$lib/features/time/TimeScreen.svelte';
  import TimerWidget from '$lib/features/time/TimerWidget.svelte';
  import HomeScreen from '$lib/features/workspaces/HomeScreen.svelte';
  import WorkspaceScreen from '$lib/features/workspaces/WorkspaceScreen.svelte';
  import { me as currentUser } from '$lib/me.svelte';
  import MyTasksScreen from '$lib/features/home/MyTasksScreen.svelte';
  import AppSidebar from '$lib/features/nav/AppSidebar.svelte';
  import { createLatest } from '$lib/latest';
  import { org } from '$lib/org.svelte';
  import { itemPath, router, workspacePath } from '$lib/router.svelte';
  import { asTheme, setTheme } from '$lib/theme.svelte';
  import { Button } from '$lib/ui/button';
  import * as Sidebar from '$lib/ui/sidebar';
  import { flushSync, onDestroy, onMount, setContext } from 'svelte';
  import { SvelteMap } from 'svelte/reactivity';
  import { timer } from '$lib/features/time/timer.svelte';

  import BackLink from './BackLink.svelte';

  clearLegacyRecordDrafts();
  let recordDiscards = $state<LegacyRecordDraft[]>([]);
  function refreshRecordDiscards(): void {
    recordDiscards = legacyRecordDrafts(currentUser.userId);
  }
  $effect(() => {
    refreshRecordDiscards();
  });
  onMount(() => attachRecordDraftCleanup(refreshRecordDiscards));

  interface Props {
    /** 브라우저 이동 경계. 시험에서도 창 자체와 이벤트 수명은 바꾸지 않는다. */
    replace?: (url: string) => void;
    reload?: () => void;
  }
  const {
    replace = (url) => {
      window.location.replace(url);
    },
    reload = () => {
      window.location.reload();
    },
  }: Props = $props();

  type Session =
    | { state: 'loading' }
    | { state: 'anonymous'; oidcEnabled: boolean }
    | { state: 'signed-in'; me: Me; workspaces: Workspace[] }
    /** `network`: 서버에 닿지 않았다(연결 배너가 내려가면 다시 읽는다). */
    | { state: 'error'; message: string; network: boolean };

  let session = $state<Session>({ state: 'loading' });
  let searchOpen = $state(false);
  let displayOpen = $state(false);
  const sessionHidden = $derived(accountSafety.switching || accountSafety.restoring);
  let sessionSequence = 0;
  let taskContextSeed = $state<TaskStartupSeed>();
  let restoreSequence = 0;
  let restoringUserId: string | undefined;
  let checkingSession = false;
  let loadingSession = false;
  let restoringSession = false;
  let sessionCheckPending = false;
  let cacheRestorePending = false;
  let sessionVerification: Promise<TaskSessionProof | undefined> | undefined;
  let taskSessionActive = true;
  const taskSessionLifetime = new AbortController();
  let taskSessionConfirmations = 0;
  const taskPublications = trackTaskSessionPublications();
  setContext(TASK_SESSION_RECOVERY, createTaskSessionRecovery(confirmTaskSession));
  const signalRechecks = new SvelteMap<string, ReturnType<typeof setTimeout> | undefined>();
  onMount(schedulePendingSessionCheck);
  onDestroy(() => {
    taskSessionActive = false;
    taskSessionLifetime.abort();
    taskPublications.dispose();
    sessionSequence += 1;
    restoreSequence += 1;
    sessionCheckPending = false;
    cacheRestorePending = false;
    for (const timeout of signalRechecks.values()) clearTimeout(timeout);
    signalRechecks.clear();
  });

  async function load(
    verified?: { me: Me; generation: number },
    attempt = 0,
    notificationRetry = createSessionNotificationRetry(),
  ): Promise<boolean> {
    const prepareStartup = session.state !== 'signed-in';
    const mine = ++sessionSequence;
    taskContextSeed = undefined;
    loadingSession = true;
    const generation = verified?.generation ?? readSessionGeneration();
    const safetyGeneration = accountSafety.generation;
    const retryLoad = async (): Promise<boolean> => {
      if (attempt + 1 >= SESSION_CHECK_MAX_ATTEMPTS) throw new Error(m.feedback_restore_failed());
      return load(undefined, attempt + 1, notificationRetry);
    };
    const retryNotification = async (): Promise<boolean> => {
      if (!(await notificationRetry.wait(taskSessionLifetime.signal))) return false;
      if (mine !== sessionSequence || cacheRestorePending || !taskSessionActive) return false;
      if (safetyGeneration !== accountSafety.generation) return retryLoad();
      return load(undefined, attempt, notificationRetry);
    };
    try {
      const me = verified?.me ?? (await api.me());
      if (mine !== sessionSequence || cacheRestorePending) return false;
      const taskStartup =
        verified === undefined && router.route.name === 'item' && router.route.group === 'tasks';
      const publishNonTaskStartup =
        verified === undefined && router.route.name !== 'public' && !taskStartup;
      if (verified === undefined && router.route.name !== 'public') {
        if (safetyGeneration !== accountSafety.generation) return await retryLoad();
        if (taskStartup) {
          if (generation !== readSessionGeneration()) return await retryNotification();
        }
      }
      if (me === undefined) {
        if (publishNonTaskStartup) void publishSessionCheck(generation);
        const info = await api.authInfo();
        if (mine !== sessionSequence) return false;
        if (taskStartup) {
          if (safetyGeneration !== accountSafety.generation) return await retryLoad();
          if (generation !== readSessionGeneration()) return await retryNotification();
          void publishSessionCheck(generation);
        }
        session = { state: 'anonymous', oidcEnabled: info.oidc_enabled };
        return true;
      }
      currentUser.userId = me.id;
      currentUser.timeZone = me.time_zone;
      currentUser.locale = me.locale;
      currentUser.weekStart = me.week_start;
      currentUser.hourCycle = me.hour_cycle;
      currentUser.maxTitleChars = me.limits.max_title_chars;
      const theme = asTheme(me.theme);
      currentUser.theme = theme;
      // 화면 밝기의 정본은 계정이다. 다른 기기에서 바꿨으면 여기서 맞추고 다음 첫 그림을 위한 사본도 고친다(D-109).
      setTheme(theme);
      currentUser.displayName = me.display_name;
      // 저장한 표시 언어로 그린다(D-81). 화면 낭독기·브라우저가 쓰는 문서 언어도 맞춘다.
      await setLocale(displaySettings().locale, { reload: false });
      if (mine !== sessionSequence) return false;
      if (taskStartup) {
        if (safetyGeneration !== accountSafety.generation) return await retryLoad();
        if (generation !== readSessionGeneration()) return await retryNotification();
      }
      document.documentElement.lang = displaySettings().locale;
      const taskRoute =
        router.route.name === 'item' && router.route.group === 'tasks' ? router.route : undefined;
      const capture = (): ContextCapture => ({
        userId: currentUser.userId,
        sequence: sessionSequence,
        navigationGeneration: router.navigationGeneration,
        sessionGeneration: readTaskSessionGeneration(),
        safetyGeneration: accountSafety.generation,
        path: router.path,
      });
      const captured = capture();
      const pendingContext =
        taskRoute === undefined
          ? undefined
          : createContextRead(captured, capture, taskScreenApi.context(taskRoute.itemId));
      // 조직 목록(D-111)은 전환기와 개인 화면의 범위다. 서버는 기본 조직을 정하지 않으므로 선택은 org 문맥이 든다.
      const [workspaces, organizations, initialContext] = await Promise.all([
        api.workspaces(me.id),
        api.myOrganizations(me.id),
        pendingContext,
      ]);
      if (mine !== sessionSequence) return false;
      if (taskStartup) {
        if (safetyGeneration !== accountSafety.generation) return await retryLoad();
        if (generation !== readSessionGeneration()) return await retryNotification();
      }
      let startupSeed: TaskStartupSeed | undefined;
      if (initialContext !== undefined && taskRoute !== undefined) {
        const rechecked =
          initialContext.kind === 'loaded'
            ? await createContextRead(captured, capture, Promise.resolve(initialContext.value))
            : initialContext.kind === 'failed'
              ? await createContextRead(
                  captured,
                  capture,
                  Promise.reject<TaskContext>(initialContext.error),
                )
              : initialContext;
        // 복원 문맥을 일반 시작 재시도로 넘기기 전에 원래 /me의 세대를 확인한다.
        if (verified !== undefined && generation !== readSessionGeneration()) return false;
        if (taskStartup) {
          if (safetyGeneration !== accountSafety.generation) return await retryLoad();
          if (generation !== readSessionGeneration()) return await retryNotification();
        }
        if (captured.path === router.path && rechecked.kind === 'discarded')
          return await retryLoad();
        if (captured.path === router.path)
          startupSeed = {
            itemId: taskRoute.itemId,
            navigationGeneration: captured.navigationGeneration,
            result: rechecked,
          };
      }
      if (
        (publishNonTaskStartup || verified !== undefined) &&
        generation !== readSessionGeneration()
      ) {
        // 복원은 원래 사용자와도 다시 비교해야 하므로 restoreSession이 /me부터 재확인한다.
        if (verified !== undefined) return false;
        // 표식은 재확인 신호다. 안전 세대의 재시도 상한을 소진하지 않고 자기 /me와 목록을 다시 읽는다.
        return await load(undefined, attempt);
      }
      org.load(me.id, organizations, me.last_organization_id ?? null);
      const startupContext =
        startupSeed?.result.kind === 'loaded' ? startupSeed.result.value : undefined;
      if (
        prepareStartup &&
        !sessionHidden &&
        startupSeed !== undefined &&
        startupContext !== undefined &&
        taskRoute !== undefined &&
        workspaces.some(
          (space) =>
            space.id === taskRoute.workspaceId && space.id === startupContext.item.workspace_id,
        )
      ) {
        startupSeed.reads = startTaskReads(
          captured,
          capture,
          taskRoute.itemId,
          startupContext,
          taskRoute.taskId,
        );
      }
      taskContextSeed = startupSeed;
      // 자기 게시가 비교할 세대를 바꾸지 않게 목록을 채택한 뒤 알린다. 게시 잠금은 화면을 막지 않는다.
      if (publishNonTaskStartup) void publishSessionCheck(generation);
      session = { state: 'signed-in', me, workspaces };
      if (taskStartup)
        void publishSessionCheck(generation, (published) => {
          if (
            taskSessionActive &&
            currentUser.userId === me.id &&
            safetyGeneration === accountSafety.generation &&
            !sessionHidden &&
            !auth.expired
          )
            taskPublications.published(generation, published);
        });
      return true;
    } catch (e: unknown) {
      if (mine !== sessionSequence || cacheRestorePending) return false;
      if (e instanceof SessionListMismatch) {
        if (verified !== undefined) throw e;
        if (safetyGeneration === accountSafety.generation) return await load(undefined, attempt);
      }
      if (
        verified === undefined &&
        router.route.name === 'item' &&
        router.route.group === 'tasks' &&
        safetyGeneration === accountSafety.generation &&
        generation !== readSessionGeneration()
      )
        return await retryNotification();
      if (verified === undefined && safetyGeneration !== accountSafety.generation) {
        try {
          return await retryLoad();
        } catch {
          /* 아래 안전한 시작 실패 화면을 쓴다. */
        }
      }
      // 브라우저·서버의 원문 대신 번역된 문구(D-81). 서버에 닿지 않은 경우는 다시 시도하면 된다고 알린다.
      const network =
        e instanceof NetworkError || (e instanceof ApiError && e.code === 'unreachable');
      session = {
        state: 'error',
        message: network ? m.app_server_unreachable() : errorText(e, m.app_start_failed()),
        network,
      };
      return false;
    } finally {
      if (mine === sessionSequence) {
        loadingSession = false;
        drainSessionCheck();
      }
    }
  }

  void load();

  async function retry(): Promise<void> {
    if (accountSafety.switching && accountSafety.reason === 'restore') {
      await restoreSession();
      return;
    }
    session = { state: 'loading' };
    await load();
  }

  // 첫 화면을 서버에 닿지 못해 그리지 못했으면, 연결 배너가 내려갈 때(서버가 다시 답함) 다시 읽는다.
  $effect(() => {
    if (session.state === 'error' && session.network && !connection.lost) void retry();
  });

  // 읽지 않은 알림 수는 서버가 현재 조직 범위로 센다(D-118; 초대는 어느 조직에서나 포함). 화면을 옮길 때 다시 세되
  // 30초 안에는 다시 묻지 않는다. 알림 화면에서 읽음 처리하면 곧바로, 창으로 돌아오거나 조직을 바꾸면 다시 센다.
  const UNREAD_FRESH_MS = 30_000;
  // 초대 수락·거절은 경로 이동의 30초 캐시를 기다리지 않는다.
  let seenInvitationVersion = invitationChanges.version;
  $effect(() => {
    const version = invitationChanges.version;
    if (version === seenInvitationVersion) return;
    seenInvitationVersion = version;
    void refreshUnread(true);
  });
  let unreadCount = $state(0);
  let unreadAt = 0;
  let unreadSeq = 0;
  /** 마지막으로 센 조직. 조직이 바뀌었을 때만 바로 다시 센다(처음 그릴 때 두 번 묻지 않게). */
  let unreadOrganization: string | undefined;
  async function refreshUnread(force = false): Promise<void> {
    if (session.state !== 'signed-in') return;
    const organizationId = org.currentId;
    if (organizationId === undefined) {
      // 선택이 풀리면 아직 안 온 이전 조직의 수가 뒤늦게 0을 덮지 않게 그 요청을 버리고, 다음 선택 때 바로 다시 센다.
      unreadSeq += 1;
      unreadAt = 0;
      unreadOrganization = undefined;
      unreadCount = 0;
      return;
    }
    if (!force && Date.now() - unreadAt < UNREAD_FRESH_MS) return;
    unreadAt = Date.now();
    unreadOrganization = organizationId;
    // 늦게 도착한 이전 응답(읽음 처리 전 수)이 새 수를 덮지 않게 한다.
    const mine = ++unreadSeq;
    try {
      const count = await activityApi.unreadCount(organizationId);
      if (mine === unreadSeq) unreadCount = count;
    } catch {
      // 읽지 못했으면 0("알림 없음")으로 보이지 않게 앞의 수를 둔다. 다음 이동·창 초점 때 바로 다시 묻는다.
      if (mine === unreadSeq) unreadAt = 0;
    }
  }
  $effect(() => {
    if (session.state === 'signed-in' && router.path !== '') void refreshUnread();
  });
  $effect(() => {
    // 조직을 바꾸면 그 조직의 수로 바로 다시 센다.
    const organizationId = org.currentId;
    if (
      session.state === 'signed-in' &&
      organizationId !== undefined &&
      organizationId !== unreadOrganization
    )
      void refreshUnread(true);
  });
  $effect(() => {
    const onFocus = (): void => {
      if (checkSession() || sessionHidden) return;
      void refreshUnread();
    };
    const onVisible = (): void => {
      if (document.visibilityState === 'visible') checkSession();
    };
    window.addEventListener('focus', onFocus);
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      window.removeEventListener('focus', onFocus);
      document.removeEventListener('visibilitychange', onVisible);
    };
  });

  // 데스크톱 사이드바를 접었는지(사이드바가 쿠키에 남긴 값). 처음 열 때 그대로 되살린다.
  const sidebarOpen = !document.cookie.split('; ').includes('sidebar_state=false');

  // 늦게 도착한 이전 목록 응답이 새로 읽은 멤버십을 덮지 않게 마지막 요청의 응답만 넣는다.
  const workspacesReload = createLatest();

  /**
   * 멤버십이 바뀐 뒤(초대 수락·탈퇴·인계) 워크스페이스·조직 목록을 다시 읽는다. 겹치면 마지막 응답만 적용하고, 먼저 부른
   * 쪽도 그 뒤에 돌아간다(초대 수락 뒤 `org.select`가 새 조직이 든 목록을 봐야 한다).
   */
  async function reloadWorkspaces(): Promise<void> {
    const sessionAtStart = sessionSequence;
    const viewerId = currentUser.userId;
    await workspacesReload.run(async (isCurrent) => {
      const [workspaces, organizations] = await Promise.all([
        api.workspaces(viewerId),
        api.myOrganizations(viewerId),
      ]);
      if (isCurrent() && sessionAtStart === sessionSequence && session.state === 'signed-in') {
        session.workspaces = workspaces;
        org.load(session.me.id, organizations);
      }
    });
  }

  function clearSession(): void {
    sessionSequence += 1;
    session = { state: 'loading' };
    org.reset();
    Object.assign(currentUser, {
      userId: '',
      timeZone: '',
      displayName: '',
      locale: '',
      weekStart: '',
      hourCycle: '',
      theme: 'system',
      maxTitleChars: 200,
    });
    searchOpen = false;
    displayOpen = false;
    unreadSeq += 1;
    unreadCount = 0;
    unreadAt = 0;
    unreadOrganization = undefined;
    timer.chooseRestart(undefined);
    timer.entry = null;
    timer.error = undefined;
  }

  let loggingOut = false;
  async function logout(): Promise<void> {
    if (loggingOut) return;
    loggingOut = true;
    const attemptId = crypto.randomUUID();
    restoreSequence += 1;
    restoringUserId = undefined;
    hideSession('logout');
    flushSync(clearSession);
    try {
      publishLogoutPending(attemptId);
      const redirect = await api.logout();
      void publishLogoutResult('success', attemptId);
      replace(redirect);
    } catch (error: unknown) {
      // 서버 세션이 남았을 수 있다. 비운 화면을 되살리지 않고 같은 안전 상태에서 다시 로그아웃한다(#212 1행).
      void publishLogoutResult('failure', attemptId);
      accountSafety.error = errorText(error, m.feedback_logout_failed());
    } finally {
      loggingOut = false;
    }
  }

  function hideForCache(event: PageTransitionEvent): void {
    if (!event.persisted || route.name === 'public' || sessionHidden) return;
    restoringUserId = currentUser.userId;
    // 복원 첫 그림에 이전 DOM이 쓰이지 않도록 BFCache에 들어갈 때도 가린다.
    hideSession('restore');
  }

  async function restoreSession(attempt = 0): Promise<void> {
    if (sessionHidden && accountSafety.reason !== 'restore' && accountSafety.reason !== 'reauth')
      return;
    const mine = ++restoreSequence;
    checkingSession = false;
    restoringSession = true;
    restoringUserId ??= currentUser.userId;
    hideSession('restore');
    const generation = readSessionGeneration();
    const safetyGeneration = accountSafety.generation;
    const active = (): boolean =>
      mine === restoreSequence &&
      !cacheRestorePending &&
      sessionHidden &&
      (accountSafety.reason === 'restore' || accountSafety.reason === 'reauth');
    const recheck = (): void => {
      if (attempt + 1 < SESSION_CHECK_MAX_ATTEMPTS) void restoreSession(attempt + 1);
      else {
        accountSafety.switching = true;
        accountSafety.error = m.feedback_restore_failed();
      }
    };
    try {
      const now = await api.me();
      if (!active()) return;
      if (safetyGeneration !== accountSafety.generation) {
        recheck();
        return;
      }
      if (now === undefined) {
        void publishSessionCheck(generation);
        flushSync(clearSession);
        hideSession('signed-out');
        replace(router.path + router.hash);
      } else if (now.id !== restoringUserId) {
        void publishSessionCheck(generation);
        hideSession('account');
        replace(router.path + router.hash);
      } else {
        // 하위 화면도 새로 마운트해 목록·문서·개인 데이터를 다시 읽는다.
        flushSync(clearSession);
        // 사용자 ID가 맞았다. 시각적 가림은 유지하고 새 읽기만 연다(공통 request가 쓰기는 계속 막는다).
        accountSafety.switching = false;
        const loaded = await load({ me: now, generation });
        if (mine !== restoreSequence || cacheRestorePending || !restoring()) return;
        if (generation !== readSessionGeneration()) {
          // 알림은 목록을 버리고 재확인하라는 신호다. 안전 세대의 재시도 상한은 소진하지 않는다.
          void restoreSession(attempt);
          return;
        }
        if (loaded) {
          void publishSessionCheck(generation);
          restoringUserId = undefined;
          accountSafety.restoring = false;
        } else {
          accountSafety.switching = true;
          accountSafety.error = m.feedback_restore_failed();
        }
      }
    } catch (error: unknown) {
      if (!active()) return;
      if (safetyGeneration !== accountSafety.generation) {
        recheck();
        return;
      }
      if (error instanceof SessionListMismatch) {
        void restoreSession(attempt);
        return;
      }
      // 확인 실패는 기존 DOM·입력을 보존하고 직접 다시 시도하게 한다.
      accountSafety.switching = true;
      accountSafety.error = errorText(error, m.feedback_restore_failed());
    } finally {
      if (mine === restoreSequence) {
        restoringSession = false;
        drainSessionCheck();
      }
    }
  }

  function restoring(): boolean {
    return sessionHidden && accountSafety.reason === 'restore';
  }

  function restorePage(event: PageTransitionEvent): void {
    schedulePendingSessionCheck();
    if (event.persisted && route.name !== 'public') {
      cacheRestorePending = true;
      hideForCache(event);
    }
    checkSession();
  }

  /** 살아 있던 화면의 짧은 값·저장 실패 입력은 재마운트하지 않는다(D-27/D-96/#88, 코디네이터 SR 정정). */
  function verifyingSession(mine: number): boolean {
    return (
      mine === restoreSequence &&
      !cacheRestorePending &&
      (!sessionHidden || accountSafety.reason === 'reauth' || accountSafety.reason === 'restore')
    );
  }

  function verifySession(attempt = 0): Promise<TaskSessionProof | undefined> {
    const pending = verifySessionNow(attempt);
    sessionVerification = pending;
    return pending;
  }

  async function confirmTaskSession(userId: string): Promise<TaskSessionProof | undefined> {
    const canVerify = (): boolean =>
      taskSessionActive &&
      currentUser.userId === userId &&
      (!sessionHidden || (accountSafety.reason === 'reauth' && accountSafety.error === '')) &&
      router.route.name === 'item' &&
      router.route.group === 'tasks';
    if (!canVerify()) return;
    const notificationRetry = createSessionNotificationRetry();
    taskSessionConfirmations += 1;
    try {
      for (;;) {
        if (!canVerify()) return;
        let pending =
          checkingSession && sessionVerification !== undefined
            ? sessionVerification
            : verifySession();
        let awaited: Promise<TaskSessionProof | undefined>;
        let proof: TaskSessionProof | undefined;
        do {
          awaited = pending;
          proof = await awaited;
          pending = sessionVerification ?? awaited;
        } while (pending !== awaited);
        if (!canVerify()) return;
        if (
          proof?.userId === userId &&
          proof.sessionGeneration === readTaskSessionGeneration() &&
          proof.safetyGeneration === accountSafety.generation
        ) {
          if (sessionHidden && accountSafety.reason === 'reauth') {
            auth.expired = false;
            accountSafety.switching = false;
            accountSafety.restoring = false;
            void refreshUnread();
          }
          return proof;
        }
        if (auth.expired) return;
        if (
          !(await notificationRetry.wait(taskSessionLifetime.signal, () => {
            if (!sessionHidden) hideSession('reauth');
          }))
        )
          return;
      }
    } finally {
      taskSessionConfirmations -= 1;
      drainSessionCheck();
    }
  }

  async function verifySessionNow(attempt = 0): Promise<TaskSessionProof | undefined> {
    if (sessionHidden && accountSafety.reason !== 'reauth' && accountSafety.reason !== 'restore')
      return;
    const expected = currentUser.userId;
    const mine = ++restoreSequence;
    checkingSession = true;
    const generation = readTaskSessionGeneration();
    const safetyGeneration = accountSafety.generation;
    const recheck = (): void => {
      hideSession('reauth');
      if (attempt + 1 < SESSION_CHECK_MAX_ATTEMPTS) void verifySession(attempt + 1);
      else accountSafety.error = m.feedback_restore_failed();
    };
    try {
      const now = await api.me();
      if (!verifyingSession(mine)) return;
      if (safetyGeneration !== accountSafety.generation) {
        recheck();
        return;
      }
      if (now === undefined) {
        hideSession('reauth');
        reportSessionExpired();
      } else if (now.id !== expected) {
        switchAccount();
      } else {
        // Both recovery and a later manual retry need a current task-session proof.
        if (
          sessionHidden &&
          accountSafety.reason === 'reauth' &&
          router.route.name === 'item' &&
          router.route.group === 'tasks'
        ) {
          if (generation !== readTaskSessionGeneration()) return undefined;
          if (taskConfirmationPending())
            return { userId: now.id, sessionGeneration: generation, safetyGeneration };
        }
        auth.expired = false;
        accountSafety.switching = false;
        accountSafety.restoring = false;
        void refreshUnread();
        if (generation === readTaskSessionGeneration())
          return { userId: now.id, sessionGeneration: generation, safetyGeneration };
      }
    } catch (error: unknown) {
      if (verifyingSession(mine)) {
        if (safetyGeneration !== accountSafety.generation) {
          recheck();
          return;
        }
        hideSession('reauth');
        accountSafety.error = errorText(error, m.feedback_restore_failed());
      }
    } finally {
      if (mine === restoreSequence) {
        checkingSession = false;
        drainSessionCheck();
      }
    }
    return undefined;
  }

  function drainSessionCheck(): void {
    if (
      !sessionCheckPending ||
      loadingSession ||
      restoringSession ||
      checkingSession ||
      taskConfirmationPending()
    )
      return;
    sessionCheckPending = false;
    checkSession();
  }

  function taskConfirmationPending(): boolean {
    return (
      taskSessionConfirmations > 0 && router.route.name === 'item' && router.route.group === 'tasks'
    );
  }

  /** 확인 중 받은 복귀 이벤트는 버리지 않고 후속 확인 하나로 합친다(SR-221-10). */
  function checkSession(): boolean {
    if (router.route.name === 'public') return false;
    if (loadingSession || restoringSession || checkingSession || taskConfirmationPending()) {
      sessionCheckPending = true;
      return true;
    }
    if (sessionHidden && accountSafety.reason !== 'restore' && accountSafety.reason !== 'reauth')
      return false;
    if (cacheRestorePending) {
      cacheRestorePending = false;
      void restoreSession();
      return true;
    }
    const expected = restoringUserId ?? currentUser.userId;
    if (expected === '') return false;
    if (restoring()) void restoreSession();
    else void verifySession();
    return true;
  }

  function sessionSignal(event: StorageEvent): void {
    if (!isSessionSignal(event)) return;
    schedulePendingSessionCheck();
    checkSession();
  }

  function schedulePendingSessionCheck(): void {
    if (router.route.name === 'public') return;
    const pending = readLogoutPending();
    if (pending === undefined) return;
    const attemptKey = JSON.stringify([pending.attemptId, pending.startedAt]);
    if (signalRechecks.has(attemptKey)) return;
    // SR-242-1: 같은 시도만 합친다. 다른 시도의 예약을 취소하지 않고 각자 기한을 지킨다.
    const remaining = Math.min(
      SESSION_SIGNAL_RECHECK_MS,
      Math.max(0, pending.startedAt + SESSION_SIGNAL_RECHECK_MS - Date.now()),
    );
    const timeout = setTimeout(() => {
      signalRechecks.set(attemptKey, undefined);
      checkSession();
    }, remaining);
    signalRechecks.set(attemptKey, timeout);
  }

  const route = $derived(router.route);
  const WORKSPACE_ROUTES = new Set([
    'workspace',
    'document',
    'members',
    'calendar',
    'trash',
    'audit',
    'module',
    'item',
    'item-document',
  ]);
  // 글을 읽고 쓰는 화면은 줄 길이를 좁게, 표·칸반·달력이 있는 화면은 넓게 둔다.
  const READING_ROUTES = new Set([
    'document',
    'item-document',
    'standalone-document',
    'notes',
    'shared',
    'notifications',
  ]);
  // 머리글 제목: 워크스페이스 안이면 그 이름, 개인 화면이면 그 화면 이름.
  const headerTitle = $derived.by(() => {
    if (currentWorkspace !== undefined) return currentWorkspace.name;
    switch (route.name) {
      case 'server-admin':
        return m.organizations_server_manage();
      case 'organization':
        return (
          org.organizations.find((organization) => organization.id === route.organizationId)
            ?.name ?? m.organizations_member_entry()
        );
      case 'notes':
        return m.app_nav_notes();
      case 'shared':
        return m.app_nav_shared();
      case 'notifications':
        return m.app_nav_notifications();
      case 'my-tasks':
        return m.app_nav_my_tasks();
      case 'time':
        return m.app_nav_time();
      case 'my-trash':
        return m.app_nav_my_trash();
      case 'my-audit':
        return m.app_nav_my_audit();
      default:
        return m.app_title_home();
    }
  });
  const currentWorkspace = $derived.by(() => {
    if (session.state !== 'signed-in') return undefined;
    if (!WORKSPACE_ROUTES.has(route.name)) return undefined;
    if (!('workspaceId' in route)) return undefined;
    const id = route.workspaceId;
    return session.workspaces.find((w) => w.id === id);
  });
  // 지금 보는 워크스페이스의 조직이 현재 조직이다(D-111). 워크스페이스로 들어간 것은 사용자의 선택이므로 그 조직을 기억해
  // 뒤이어 홈·알림함 같은 개인 화면도 같은 조직 범위로 본다(구현 해석; 여러 조직 중 첫 조직을 조용히 고르는 것과 다르다).
  $effect(() => {
    org.routeOrganizationId =
      route.name === 'organization' ? route.organizationId : currentWorkspace?.organization_id;
    if (route.name === 'organization') org.select(route.organizationId);
    if (currentWorkspace !== undefined) org.select(currentWorkspace.organization_id);
  });
</script>

<svelte:window onpageshow={restorePage} onpagehide={hideForCache} onstorage={sessionSignal} />

{#if route.name !== 'public' && (session.state !== 'loading' || sessionHidden)}
  <!-- 표시 언어(D-81)를 정한 뒤에 그린다(토스트 영역 라벨도 그 언어로). -->
  <FeedbackHost
    {reload}
    sessionManaged
    retryLogout={() => void logout()}
    retryRestore={() => {
      if (accountSafety.reason === 'reauth') void verifySession();
      else void restoreSession();
    }}
  />
{/if}

<!-- 계정 전환 안전 상태. 입력 보존을 위해 조건부 제거하지 않는다. -->
<div style:display={sessionHidden ? 'none' : 'contents'} inert={sessionHidden}>
  {#if route.name !== 'public' && currentUser.userId !== ''}
    {#each recordDiscards as entry (entry.key)}
      <div class="m-3 rounded border p-3" data-testid="record-draft-unrecoverable">
        <p>{m.records_draft_unrecoverable()}</p>
        <Button
          variant="outline"
          onclick={() => {
            if (sessionHidden) return;
            discardLegacyRecordDraft(entry, currentUser.userId);
            refreshRecordDiscards();
          }}>{m.common_draft_discard()}</Button
        >
      </div>
    {/each}
  {/if}
  {#if route.name === 'public'}
    <PublicView token={route.token} />
  {:else if session.state === 'loading'}
    <p class="p-6" aria-live="polite">{m.app_loading()}</p>
  {:else if session.state === 'error'}
    <div class="flex flex-col items-start gap-3 p-6">
      <p class="text-destructive" role="alert">{session.message}</p>
      <Button variant="outline" onclick={() => void retry()}>{m.common_retry()}</Button>
    </div>
  {:else if session.state === 'anonymous'}
    <LoginScreen oidcEnabled={session.oidcEnabled} returnTo={router.path} />
  {:else}
    {@const signedIn = session}
    <SearchDialog
      bind:open={searchOpen}
      memberOf={(ws: string) => signedIn.workspaces.some((w) => w.id === ws)}
      workspace={currentWorkspace}
    />
    <DisplaySettingsDialog bind:open={displayOpen} />
    <Sidebar.Provider open={sidebarOpen}>
      <AppSidebar
        workspaces={signedIn.workspaces}
        current={currentWorkspace}
        displayName={signedIn.me.display_name}
        isServerAdmin={signedIn.me.is_server_admin}
        {unreadCount}
        onsearch={() => {
          searchOpen = true;
        }}
        ondisplay={() => {
          displayOpen = true;
        }}
        onexport={() => {
          window.location.assign('/api/me/export');
        }}
        onlogout={() => void logout()}
      />

      <Sidebar.Inset class="min-w-0">
        <header
          class="sticky top-0 z-10 flex h-12 items-center gap-2 border-b bg-background/95 px-3 backdrop-blur supports-[backdrop-filter]:bg-background/80"
        >
          <Sidebar.Trigger />
          <span class="min-w-0 flex-1 truncate text-base font-semibold" title={headerTitle}
            >{headerTitle}</span
          >
          <div class="flex shrink-0 items-center gap-2">
            <TimerWidget />
          </div>
        </header>

        <!-- 화면 안 여러 열 배치는 창 폭(md·lg)이 아니라 이 본문 폭(@container)을 본다. 사이드바가 차지한 폭과 사용자가 키운 루트
           글꼴(컨테이너 쿼리의 rem은 루트 글꼴을 따른다)을 함께 반영한다. -->
        <main
          class={`@container mx-auto flex w-full min-w-0 flex-col gap-8 px-4 py-6 sm:px-6 ${READING_ROUTES.has(route.name) ? 'max-w-3xl' : 'max-w-5xl'}`}
        >
          {#if route.name === 'home'}
            <!-- 조직 선택 전 홈은 선택 안내만 표시(D-111·D-120). -->
            {#if org.currentId === undefined}
              <p role="status">{m.app_organization_required()}</p>
            {:else}
              <HomeScreen
                workspaces={signedIn.workspaces.filter((w) => w.organization_id === org.currentId)}
                oncreated={(ws: Workspace) => {
                  if (
                    session.state === 'signed-in' &&
                    !session.workspaces.some((w) => w.id === ws.id)
                  ) {
                    session.workspaces = [...session.workspaces, ws];
                  }
                }}
                onjoined={reloadWorkspaces}
              />
            {/if}
          {:else if route.name === 'server-admin'}
            {#key signedIn.me.id}
              <ServerAdminScreen
                tab={route.tab}
                isServerAdmin={signedIn.me.is_server_admin}
                meId={signedIn.me.id}
                oncreated={() => void reloadWorkspaces()}
              />
            {/key}
          {:else if route.name === 'organization'}
            {#key `${signedIn.me.id}:${route.organizationId}`}
              <OrganizationScreen
                organizationId={route.organizationId}
                tab={route.tab}
                meId={signedIn.me.id}
                onchanged={() => void reloadWorkspaces()}
              />
            {/key}
          {:else if WORKSPACE_ROUTES.has(route.name)}
            {#if currentWorkspace === undefined}
              <p role="alert">{m.app_workspace_missing()}</p>
            {:else if route.name === 'members'}
              <MembersScreen
                workspace={currentWorkspace}
                tab={route.tab}
                meId={signedIn.me.id}
                onchanged={() => void reloadWorkspaces()}
              />
            {:else if route.name === 'workspace'}
              <WorkspaceScreen workspace={currentWorkspace} />
            {:else if route.name === 'trash'}
              <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
              <TrashScreen workspace={currentWorkspace} />
            {:else if route.name === 'audit'}
              <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
              <AuditScreen workspace={currentWorkspace} />
            {:else if route.name === 'calendar'}
              <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
              <CalendarScreen workspace={currentWorkspace} />
            {:else if route.name === 'module'}
              <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
              <ModuleScreen workspace={currentWorkspace} moduleId={route.moduleId} />
            {:else if route.name === 'item'}
              <ItemScreen
                aggregate={route.group === 'tasks'}
                initialContext={taskContextSeed}
                workspace={currentWorkspace}
                itemId={route.itemId}
                taskId={route.taskId}
                workspaces={signedIn.workspaces}
              />
            {:else if route.name === 'item-document'}
              <BackLink
                path={itemPath(currentWorkspace.id, route.itemId)}
                label={m.app_back_to_item()}
              />
              <DocumentScreen documentId={route.documentId} workspaces={signedIn.workspaces} />
            {:else if route.name === 'document'}
              <BackLink
                path={workspacePath(currentWorkspace.id)}
                label={m.app_wiki_of({ name: currentWorkspace.name })}
              />
              <DocumentScreen documentId={route.documentId} workspaces={signedIn.workspaces} />
            {/if}
          {:else if route.name === 'notes'}
            <NotesScreen />
          {:else if route.name === 'my-tasks'}
            <MyTasksScreen />
          {:else if route.name === 'notifications'}
            <!-- 초대 수락 뒤 조직·공간 문맥을 함께 갱신(D-112·D-118). 상세 id는 router.route에서 읽는다. -->
            <NotificationsScreen
              workspaces={signedIn.workspaces}
              onchanged={() => void refreshUnread(true)}
              onaccepted={reloadWorkspaces}
            />
          {:else if route.name === 'my-audit'}
            <AuditScreen workspace={undefined} />
          {:else if route.name === 'my-trash'}
            <div class="flex flex-col gap-8">
              <TrashScreen workspace={undefined} />
              <DeletedWorkspaces onchanged={() => void reloadWorkspaces()} />
            </div>
          {:else if route.name === 'time'}
            <!-- 공유 설정 제거(D-117)로 워크스페이스 목록을 넘기지 않는다. -->
            <TimeScreen />
          {:else if route.name === 'shared'}
            <SharedScreen />
          {:else if route.name === 'standalone-document'}
            <DocumentScreen documentId={route.documentId} workspaces={signedIn.workspaces} />
          {:else}
            <p role="alert">{m.app_not_found()}</p>
          {/if}
        </main>
      </Sidebar.Inset>
    </Sidebar.Provider>
  {/if}
</div>
