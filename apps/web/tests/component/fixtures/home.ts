import { homeApi } from '$lib/features/home/api';
import { activityApi } from '$lib/features/activity/api';
import { vi } from 'vitest';

/** 기존 홈 회귀 시험의 추가 조회. 개별 시험은 필요한 업무/활동 응답을 덮어쓴다. */
export function homeExtras(): void {
  vi.spyOn(homeApi, 'myTasks').mockResolvedValue({
    counts: { overdue: 0, today: 0, upcoming: 0, undated: 0, done: 0 },
    workspaces: [],
  });
  vi.spyOn(activityApi, 'recentActivity').mockResolvedValue([]);
}
