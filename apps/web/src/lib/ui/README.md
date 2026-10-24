# 공통 UI (shadcn-svelte)

출처: shadcn-svelte 1.7.0 레지스트리(`https://shadcn-svelte.com/registry`), 스타일 `nova`, 기본색 `neutral`, 아이콘 `@lucide/svelte`. 기반 컴포넌트는 Bits UI. 결정은 `docs/decisions.md` D-66.

설치 명령(2026-10-17):

```bash
pnpm dlx shadcn-svelte@1.7.0 add button input checkbox label dialog dropdown-menu sidebar --yes --no-deps-install
```

`sidebar`의 레지스트리 의존성으로 `tooltip`, `skeleton`, `separator`, `sheet`, `$lib/hooks/is-mobile`이 함께 들어왔다. `$lib/utils.ts`의 `cn`과 타입 보조도 레지스트리 산출물이다.

수정 기록

- `dropdown-menu/`: `RadioGroup`·`RadioItem`을 제거(2026-10-17). 현재 미사용이고 `dropdown-menu-radio-group.svelte`가 `exactOptionalPropertyTypes`와 충돌한다(bits-ui `value?: string`에 `undefined` 전달). 필요해지면 `add dropdown-menu --overwrite` 후 타입을 맞춘다.
- `select/select-item.svelte`: `label`을 값이 있을 때만 넘기도록 수정(2026-10-17). 같은 `exactOptionalPropertyTypes` 충돌.
- 추가(2026-10-17): `pnpm dlx shadcn-svelte@1.7.0 add select badge --yes --no-deps-install`. 이미 있는 `separator`를 덮어쓰겠냐는 질문에 막히므로 잠시 옮겨 두고 추가한 뒤 원본과 같음(포맷 차이만)을 확인했다.
- 추가(2026-10-17, D-82): `pnpm dlx shadcn-svelte@1.7.0 add sonner --yes --no-deps-install`, 의존성은 `svelte-sonner` 1.2.1만 고정 설치(CLI가 넣으려던 `mode-watcher`와 `@lucide/svelte` 범위 변경은 되돌림). `sonner/sonner.svelte`: `mode-watcher`를 빼고 처음에는 `theme="light"`로 고정했다. 2026-10-25부터 앱의 화면 밝기(`$lib/theme.svelte`의 `themeState`)를 따른다.
- 손댐(2026-10-18, D-81): 생성기가 넣은 영어 문구를 화면 언어 문구로 바꿈. `dialog/dialog-content.svelte`·`dialog/dialog-footer.svelte`·`sheet/sheet-content.svelte`의 닫기(`m.common_close()`), `sidebar/sidebar-trigger.svelte`·`sidebar/sidebar-rail.svelte`의 사이드바 열고 닫기(`m.app_toggle_sidebar()`). 다시 생성하면 이 다섯 곳을 같은 식으로 고친다.
- 손댐(2026-10-18): `sidebar/sidebar-menu-button.svelte`·`sidebar/sidebar-menu-sub-button.svelte`가 `data-active={isActive}`로 거짓일 때도 `data-active="false"`를 남겨, Tailwind v4 `data-active:` 변형(속성 존재만 봄)이 모든 메뉴를 활성처럼 칠했다. 거짓이면 속성을 빼도록 바꿈. `sidebar/sidebar.svelte`의 모바일 서랍 화면 낭독기 제목·설명(영어 고정)을 `m.app_nav_label()`·`m.app_nav_description()`으로. 다시 생성하면 이 세 곳을 같은 식으로 고친다.
- 추가(2026-10-22): `pnpm dlx shadcn-svelte@1.7.0 add alert-dialog --yes --no-deps-install`(저장하지 않은 입력을 버릴지 묻는 확인, `features/common/DiscardChangesConfirm.svelte`). 이미 있는 `button`을 덮어쓰겠냐는 질문에 막혀 잠시 옮겨 두고 추가했고, 생성된 `button`은 원본과 포맷만 달라 원본으로 되돌렸다. 생성 파일은 prettier로 맞췄고, `alert-dialog/alert-dialog-action.svelte`·`alert-dialog/alert-dialog-cancel.svelte`는 `buttonVariants(...)` 호출을 ESLint가 해석하지 못해(no-unsafe-call) `dialog-content`의 닫기 단추처럼 `child`로 `Button`을 그리게 바꿨다(겉모습 같음). 다시 생성하면 이 두 곳을 같은 식으로 고친다.
- 손댐(2026-10-25): `dialog/dialog-title.svelte`에 `pr-6`(두 줄 이상인 제목이 오른쪽 위 닫기 단추 밑으로 들어가 겹쳤다), `leading-none` 제거(alert-dialog·sheet 제목처럼 기본 줄 높이. 두 줄 한글 제목의 줄이 붙었다). 다시 생성하면 같은 식으로 고친다.
- 추가(2026-10-25): `pnpm dlx shadcn-svelte@1.7.0 add textarea native-select --yes --no-deps-install`. 기능마다 손으로 꾸민 여러 줄 칸·기본 선택 상자(모서리·여백·글자 크기·높이·초점 표시가 서로 달랐다)를 공통 모양으로 맞춘다. CLI가 `@lucide/svelte`를 `^1.61.0` 범위로 바꾼 것은 되돌려 고정 판을 유지했다. 생성 파일은 prettier로 맞췄다. `native-select/native-select.svelte`: 글자 크기를 `text-sm`에서 `Input`·`textarea`처럼 `text-base md:text-sm`으로(16px 미만 입력 칸은 iOS가 초점 때 화면을 확대한다). 다시 생성하면 같은 식으로 고친다. `class`는 감싸개 `div`에 붙으므로 폭은 `class`로, `select` 자체 속성(`id`·`aria-*`·`bind:value`)은 그대로 넘긴다.
- 추가(2026-10-25): `pnpm dlx shadcn-svelte@1.7.0 add collapsible --yes --no-deps-install`(업무 보드의 접는 "필터·정렬" 영역). `--no-deps-install`인데도 CLI가 `bits-ui`를 `^2.20.0`으로 바꾸고 설치까지 해 `package.json`·`pnpm-lock.yaml`을 되돌리고 고정 판 2.19.5로 다시 설치했다. 생성 파일은 prettier만 맞췄다.
- ESLint: 이 디렉터리와 `hooks/`, `utils.ts`에는 `eslint.config.mjs`의 외부 코드 예외가 적용된다(규칙 10개). 자체 코드에는 적용되지 않는다.

규칙

- 이 디렉터리의 컴포넌트는 가져온 코드다. 수정은 최소화하고, 수정한 파일은 상단 주석에 이유를 적어 업데이트 시 비교할 수 있게 한다.
- 디자인 변경은 `src/styles/app.css`의 토큰(색·글자·간격·모서리·포커스)으로 한다.
- 새 컴포넌트가 필요하면 같은 CLI로 추가하고 이 README의 명령에 덧붙인다. 같은 역할의 ui 디렉터리를 다른 곳에 만들지 않는다.
- 앱 고유 컴포넌트는 `src/lib/features/<기능>/`에 둔다. 여기에는 두지 않는다.
