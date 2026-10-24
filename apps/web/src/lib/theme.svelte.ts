// 화면 밝기: 시스템 설정 따르기(기본)·밝게·어둡게. 문서 본문 서체(doc-font.ts, D-86)처럼 이 브라우저에만 저장하는 보기
// 설정이다(계정 설정·문서 내용은 바꾸지 않는다). 어두운 색은 app.css의 `.dark` 토큰이 정한다.
// 처음 그리기 전 적용은 index.html의 인라인 스크립트가 같은 키·규칙으로 한다. 둘을 함께 고친다(theme.test.ts가 맞춰 본다).

export type ThemeChoice = 'system' | 'light' | 'dark';

export const THEME_KEY = 'fvoci.theme';
const DARK_QUERY = '(prefers-color-scheme: dark)';

/** 지금 화면이 어두운지. 알림(sonner)처럼 테마를 따로 받는 구성요소가 읽는다. */
export const themeState = $state({ dark: false });

/** 저장한 선택. 없거나 읽을 수 없으면(사생활 보호 모드 등) 시스템 설정 따르기. */
export function storedTheme(): ThemeChoice {
  try {
    const value = window.localStorage.getItem(THEME_KEY);
    return value === 'light' || value === 'dark' ? value : 'system';
  } catch {
    return 'system';
  }
}

function systemDark(): boolean {
  return typeof window.matchMedia === 'function' && window.matchMedia(DARK_QUERY).matches;
}

/** 지금 화면에 적용된 선택. 저장소를 쓸 수 없는 브라우저에서는 저장한 값과 다를 수 있다(이 화면에만 적용). */
export function appliedTheme(): ThemeChoice {
  const value = document.documentElement.dataset.theme;
  return value === 'light' || value === 'dark' ? value : 'system';
}

/** 화면에 적용한다. `.dark`가 어두운 토큰과 `dark:` 변형을 켜고, `color-scheme`(app.css)이 기본 입력 칸·스크롤바를 맞춘다. */
export function applyTheme(choice: ThemeChoice): void {
  const root = document.documentElement;
  const dark = choice === 'dark' || (choice === 'system' && systemDark());
  root.classList.toggle('dark', dark);
  if (choice === 'system') delete root.dataset.theme;
  else root.dataset.theme = choice;
  themeState.dark = dark;
}

/** 고르고 저장한다. 저장할 수 없어도 지금 화면에는 적용한다. */
export function setTheme(choice: ThemeChoice): void {
  applyTheme(choice);
  try {
    if (choice === 'system') window.localStorage.removeItem(THEME_KEY);
    else window.localStorage.setItem(THEME_KEY, choice);
  } catch {
    // 저장소를 쓸 수 없는 브라우저: 이번 화면에만 적용한다.
  }
}

/** 시스템 설정을 따르는 동안 운영체제의 밝기가 바뀌면 바로 따라간다. 해제 함수를 돌려준다. */
export function followSystemTheme(): () => void {
  if (typeof window.matchMedia !== 'function') return () => undefined;
  const query = window.matchMedia(DARK_QUERY);
  const onchange = (): void => {
    if (appliedTheme() === 'system') applyTheme('system');
  };
  query.addEventListener('change', onchange);
  return () => {
    query.removeEventListener('change', onchange);
  };
}
