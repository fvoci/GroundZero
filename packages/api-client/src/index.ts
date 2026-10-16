// contracts/openapi.json에서 생성한 타입을 재노출한다. `pnpm run generate:api`로 갱신.
import type { components, paths } from './generated/schema';

export type { components, paths };
export type Health = components['schemas']['Health'];
