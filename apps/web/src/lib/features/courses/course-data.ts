// 강좌 화면은 정보·회차·개인 진도를 일정한 세 읽기로 조합한다.
import { str } from '$lib/shared/api/parse';
import { courseResources } from './api';
import { parseCourseView, type CourseView, type EpisodeView } from './model';

function emptyProgress(): EpisodeView['mine'] {
  return {
    status: 'not_started',
    planned_on: null,
    completed_on: null,
    watch_count: 0,
    last_position_seconds: null,
    note: '',
  };
}

export async function loadCourseView(item: string): Promise<CourseView> {
  const [info, episodes, progress] = await Promise.all([
    courseResources.info(item),
    courseResources.episodes(item),
    courseResources.progress(item),
  ]);
  const mine = new Map(progress.map((row) => [str(row, 'episode_id'), row]));
  return parseCourseView({
    ...info,
    can_write: info.access === 'write',
    episodes: episodes.map((episode) => ({
      ...episode,
      mine: mine.get(str(episode, 'id')) ?? emptyProgress(),
    })),
  });
}
