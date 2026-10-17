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
- ESLint: 이 디렉터리와 `hooks/`, `utils.ts`에는 `eslint.config.mjs`의 외부 코드 예외가 적용된다(규칙 10개). 자체 코드에는 적용되지 않는다.

규칙

- 이 디렉터리의 컴포넌트는 가져온 코드다. 수정은 최소화하고, 수정한 파일은 상단 주석에 이유를 적어 업데이트 시 비교할 수 있게 한다.
- 디자인 변경은 `src/styles/app.css`의 토큰(색·글자·간격·모서리·포커스)으로 한다.
- 새 컴포넌트가 필요하면 같은 CLI로 추가하고 이 README의 명령에 덧붙인다. 같은 역할의 ui 디렉터리를 다른 곳에 만들지 않는다.
- 앱 고유 컴포넌트는 `src/lib/features/<기능>/`에 둔다. 여기에는 두지 않는다.
