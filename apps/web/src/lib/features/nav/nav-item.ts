// 사이드바 링크 한 줄의 모양. `.svelte`에서 내보낸 타입은 ESLint가 읽지 못해 여기에 둔다.
import type { Component } from 'svelte';

export interface NavItem {
  path: string;
  label: string;
  icon: Component;
  active: boolean;
  /** 읽지 않은 수 등. 0이면 보이지 않는다. */
  badge?: number;
}
