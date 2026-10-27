// contracts/openapi.json에서 생성한 타입을 재노출한다. `pnpm run generate:api`로 갱신.
// 생성 타입은 컴파일 시점 계약이다. 런타임 검증은 apps/web/src/lib/api에서 한다.
import type { components, paths } from './generated/schema';

export type { components, paths };
type Schemas = components['schemas'];

export type Health = Schemas['Health'];
export type Me = Schemas['Me'];
export type AuthInfo = Schemas['AuthInfo'];
export type Role = Schemas['Role'];
export type Workspace = Schemas['Workspace'];
export type ErrorBody = Schemas['ErrorBody'];
export type Member = Schemas['Member'];
export type MembersView = Schemas['MembersView'];
export type Invitation = Schemas['Invitation'];
export type ReceivedInvitation = Schemas['ReceivedInvitation'];
export type Group = Schemas['Group'];
export type ModuleKind = Schemas['ModuleKind'];
export type Module = Schemas['Module'];
export type Item = Schemas['Item'];
// 오버홀 '달리는 아비시니안'(D-111~D-126) 계약.
export type OrgRole = Schemas['OrgRole'];
export type OrganizationSummary = Schemas['OrganizationSummary'];
export type Organization = Schemas['Organization'];
export type OrganizationLimits = Schemas['OrganizationLimits'];
export type InvitationTarget = Schemas['InvitationTarget'];
export type Accepted = Schemas['Accepted'];
export type InputLimits = Schemas['InputLimits'];
export type StatusCategory = Schemas['StatusCategory'];
export type TaskStatusDefinition = Schemas['TaskStatus'];
export type MyTasks = Schemas['MyTasks'];
export type MyTask = Schemas['MyTask'];
export type MyTaskCounts = Schemas['MyTaskCounts'];
export type NotificationPage = Schemas['NotificationPage'];
export type Activity = Schemas['Activity'];
export type ActivityPage = Schemas['ActivityPage'];
