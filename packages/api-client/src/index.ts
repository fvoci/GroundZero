// contracts/openapi.json에서 생성한 타입을 재노출한다. `pnpm run generate:api`로 갱신.
// 생성 타입은 컴파일 시점 계약이다. 런타임 검증은 apps/web/src/lib/api에서 한다.
import type { components, paths } from './generated/schema';

export type { components, paths };
type Schemas = components['schemas'];

export type Health = Schemas['Health'];
export type Me = Schemas['Me'];
export type AuthInfo = Schemas['AuthInfo'];
export type LogoutResponse = Schemas['LogoutResponse'];
export type Role = Schemas['Role'];
export type Workspace = Schemas['Workspace'];
export type CreateWorkspace = Schemas['CreateWorkspace'];
export type DocumentSummary = Schemas['DocumentSummary'];
export type CreateDocument = Schemas['CreateDocument'];
export type ErrorBody = Schemas['ErrorBody'];
/** 본문은 계약상 임의 객체(`type: "doc"`)라 생성 타입에서는 구체화하지 않는다. */
export type DocumentWire = Schemas['Document'];
export type SaveDocumentWire = Schemas['SaveDocument'];
