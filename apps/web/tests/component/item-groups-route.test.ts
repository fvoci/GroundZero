import { describe, expect, it } from 'vitest';

import {
  ITEM_GROUP_DEFAULT,
  itemGroupPath,
  itemGroupsFor,
  itemPath,
  itemTaskPath,
  itemViewPath,
  moduleCalendarPath,
  modulePath,
  parseRoute,
} from '$lib/router.svelte';

// 항목 묶음과 모듈 보기는 하위 경로다(D-121, 사용자 R7 A): 항목 `/w/:ws/i/:item/<묶음>`, 모듈 `/w/:ws/m/:mod/calendar`.
const ws = '11111111-1111-1111-1111-111111111111';
const item = '22222222-2222-2222-2222-222222222222';
const task = '33333333-3333-3333-3333-333333333333';
const mod = '44444444-4444-4444-4444-444444444444';

describe('item group routes', () => {
  it('parses each group path to the item route with its group', () => {
    for (const group of [
      'docs',
      'events',
      'records',
      'references',
      'plan',
      'episodes',
      'activities',
    ]) {
      expect(parseRoute(`/w/${ws}/i/${item}/${group}`)).toEqual({
        name: 'item',
        workspaceId: ws,
        itemId: item,
        group,
      });
      expect(parseRoute(`/w/${ws}/i/${item}/${group}/`)).toEqual({
        name: 'item',
        workspaceId: ws,
        itemId: item,
        group,
      });
    }
    expect(parseRoute(`/w/${ws}/i/${item}/unknown`)).toEqual({ name: 'not-found' });
  });

  it('keeps the task views and the task detail in the tasks group', () => {
    expect(parseRoute(`/w/${ws}/i/${item}/tasks/list`)).toEqual({
      name: 'item',
      workspaceId: ws,
      itemId: item,
      group: 'tasks',
      view: 'list',
    });
    expect(parseRoute(`/w/${ws}/i/${item}/tasks/board/t/${task}`)).toEqual({
      name: 'item',
      workspaceId: ws,
      itemId: item,
      group: 'tasks',
      view: 'board',
      taskId: task,
    });
    // 뿌리 경로는 묶음이 없다. 화면이 세션 기억·종류별 기본 묶음으로 바꾼다(replaceState).
    expect(parseRoute(itemPath(ws, item))).toEqual({ name: 'item', workspaceId: ws, itemId: item });
  });

  it('formats group paths, with the tasks group under its board view', () => {
    expect(itemGroupPath(ws, item, 'docs')).toBe(`/w/${ws}/i/${item}/docs`);
    expect(itemGroupPath(ws, item, 'events')).toBe(`/w/${ws}/i/${item}/events`);
    expect(itemGroupPath(ws, item, 'records')).toBe(`/w/${ws}/i/${item}/records`);
    expect(itemGroupPath(ws, item, 'references')).toBe(`/w/${ws}/i/${item}/references`);
    expect(itemGroupPath(ws, item, 'tasks')).toBe(itemViewPath(ws, item, 'board'));
    expect(itemGroupPath(ws, item, 'tasks', 'gantt')).toBe(itemViewPath(ws, item, 'gantt'));
    expect(itemTaskPath(ws, item, task)).toBe(`/w/${ws}/i/${item}/tasks/board/t/${task}`);
  });

  it('puts the kind group first and the common groups after it', () => {
    expect(itemGroupsFor('project')).toEqual(['tasks', 'docs', 'events', 'records']);
    expect(itemGroupsFor('research')).toEqual(['references', 'tasks', 'docs', 'events', 'records']);
    expect(itemGroupsFor('study')).toEqual(['plan', 'tasks', 'docs', 'events', 'records']);
    expect(itemGroupsFor('course')).toEqual(['episodes', 'tasks', 'docs', 'events', 'records']);
    expect(itemGroupsFor('hobby')).toEqual(['activities', 'tasks', 'docs', 'events', 'records']);
    expect(ITEM_GROUP_DEFAULT).toEqual({
      project: 'tasks',
      research: 'references',
      study: 'plan',
      course: 'episodes',
      hobby: 'activities',
    });
  });
});

describe('module view routes', () => {
  it('parses and formats the module items and calendar views', () => {
    expect(parseRoute(modulePath(ws, mod))).toEqual({
      name: 'module',
      workspaceId: ws,
      moduleId: mod,
    });
    expect(parseRoute(`/w/${ws}/m/${mod}/calendar`)).toEqual({
      name: 'module',
      workspaceId: ws,
      moduleId: mod,
      view: 'calendar',
    });
    expect(moduleCalendarPath(ws, mod)).toBe(`/w/${ws}/m/${mod}/calendar`);
  });
});
