// 기존 import 경로의 타입·파서·API 이름을 유지한다. 요청과 응답 검증은 기능별 모듈에서 맡는다(D-09).
import { courseApi } from '$lib/features/courses/api';
import { loadCourseView } from '$lib/features/courses/course-data';
import { studyApi } from '$lib/features/study/api';
import { loadStudyView } from '$lib/features/study/study-data';
import { recordsApi } from '$lib/features/records/api';
import { systemApi, readTrash } from '$lib/features/trash/api';
import { filesApi } from '$lib/features/files/api';
import { timeData } from '$lib/features/time/data';
import { accountsApi } from './client/accounts';
import { organizationsApi } from './client/organizations';
import { workspacesApi } from './client/workspaces';
import { documentsApi } from './client/documents';
import { modulesApi } from './client/modules';
import { referencesApi } from './client/references';
import { revisionsApi } from './client/revisions';
import { calendarApi } from './client/calendar';
import { searchApi } from './client/search';

export type { TrashEntryView, TrashView } from '$lib/features/trash/api';
export { fileContentUrl } from '$lib/features/files/api';
export type { FileView, FileVersionView, UploadView, FileTarget } from '$lib/features/files/model';
export { TIME_KINDS, parseTimeEntry } from '$lib/features/time/model';
export type {
  TimeKind,
  TimeTargetKind,
  TimeTargetView,
  TimeEntryView,
  TimeEntryInput,
  TimeSummaryRowView,
  TaskTimeView,
  ItemTimeView,
} from '$lib/features/time/model';
export { ApiError, NetworkError } from '$lib/shared/api';
export { statusCategory, parseTaskStatusDefinition } from '$lib/shared/api/parse';
export * from '$lib/features/tasks/model';
export type {
  RecordFieldType,
  ChoiceView,
  RecordFieldDef,
  RecordKindView,
  RecordFieldValue,
  RecordFileView,
  RecordView,
  RecordInput,
  ZoteroLibraryInput,
} from '$lib/features/records/model';
export { parseRecordKind, parseRecord } from '$lib/features/records/model';
export type {
  StudyUnitKind,
  StudyUnitView,
  StudyPlanView,
  StudyParticipantView,
} from '$lib/features/study/model';
export { STUDY_UNIT_KINDS } from '$lib/features/study/model';
export type {
  EpisodeStatus,
  CourseDetailsView,
  EpisodeView,
  CourseView,
  ParticipantView,
  NewEpisodeInput,
} from '$lib/features/courses/model';
export { parseHealth, parseMe, fetchHealth } from './client/accounts';
export type { HealthResult } from './client/accounts';
export {
  parseOrganizationSummary,
  parseOrganizationLimits,
  parseOrganization,
} from './client/organizations';
export {
  parseWorkspace,
  parseAccepted,
  parseMembersView,
  parseInvitation,
  parseReceivedInvitation,
  parseGroup,
} from './client/workspaces';
export type { DeletedWorkspaceView } from './client/workspaces';
export {
  parseDocNode,
  parseSummary,
  parseDocument,
  parseSharedView,
  parseShareView,
  parseMovePreview,
  parsePublicLink,
  parsePublicDocument,
  parseMoveOutcome,
} from './client/documents';
export type {
  DocNode,
  Access,
  DocumentView,
  SummaryView,
  SharedView,
  ShareView,
  ShareRecipientView,
  MoveAccessChangeView,
  MovePreviewView,
  MoveOutcomeView,
  PublicLinkView,
  PublicDocumentView,
  MoveTargetInput,
} from './client/documents';
export {
  MODULE_KINDS,
  parseModule,
  ITEM_STATUSES,
  parseItem,
  parseItemProfile,
} from './client/modules';
export type {
  ItemStatus,
  ItemView,
  ItemDetailsView,
  PersonRefView,
  ItemProfileView,
} from './client/modules';
export { parseCandidate, parseResolved, parseBacklink } from './client/references';
export type {
  ReferenceKind,
  ReferenceCandidateView,
  ResolvedReferenceView,
  BacklinkView,
} from './client/references';
export { parseRevisionSummary, parseRevision } from './client/revisions';
export type { RevisionSummaryView, RevisionView } from './client/revisions';
export { CALENDAR_DATE_KINDS } from './client/calendar';
export type {
  CalendarTaskView,
  EventOccurrenceView,
  ProjectedTaskView,
  CalendarDateKind,
  CalendarDateView,
  CalendarView,
} from './client/calendar';
export type { SearchKind, SearchFilters, SearchResultView } from './client/search';

export const api = {
  ...timeData,
  authInfo: accountsApi.authInfo,
  me: accountsApi.me,
  logout: accountsApi.logout,
  myOrganizations: organizationsApi.myOrganizations,
  organization: organizationsApi.organization,
  workspaces: workspacesApi.workspaces,
  createWorkspace: workspacesApi.createWorkspace,
  wikiDocuments: documentsApi.wikiDocuments,
  createWikiDocument: documentsApi.createWikiDocument,
  setParent: documentsApi.setParent,
  document: documentsApi.document,
  members: workspacesApi.members,
  setRole: workspacesApi.setRole,
  removeMember: workspacesApi.removeMember,
  leave: workspacesApi.leave,
  deleteWorkspace: workspacesApi.deleteWorkspace,
  ...filesApi,
  ...courseApi,
  course: loadCourseView,
  ...studyApi,
  studyPlan: loadStudyView,
  deletedWorkspaces: workspacesApi.deletedWorkspaces,
  restoreWorkspace: workspacesApi.restoreWorkspace,
  purgeWorkspace: workspacesApi.purgeWorkspace,
  invitations: workspacesApi.invitations,
  revokeInvitation: workspacesApi.revokeInvitation,
  myInvitations: workspacesApi.myInvitations,
  acceptInvitation: workspacesApi.acceptInvitation,
  declineInvitation: workspacesApi.declineInvitation,
  offerOwnership: workspacesApi.offerOwnership,
  cancelOwnershipOffer: workspacesApi.cancelOwnershipOffer,
  acceptOwnership: workspacesApi.acceptOwnership,
  groups: workspacesApi.groups,
  createGroup: workspacesApi.createGroup,
  deleteGroup: workspacesApi.deleteGroup,
  setGroupMember: workspacesApi.setGroupMember,
  myNotes: documentsApi.myNotes,
  createNote: documentsApi.createNote,
  sharedWithMe: documentsApi.sharedWithMe,
  shares: documentsApi.shares,
  shareRecipient: documentsApi.shareRecipient,
  share: documentsApi.share,
  unshare: documentsApi.unshare,
  copyDocument: documentsApi.copyDocument,
  documentTemplates: documentsApi.documentTemplates,
  setDocumentTemplate: documentsApi.setDocumentTemplate,
  ...recordsApi,
  itemProfile: modulesApi.itemProfile,
  setItemProfile: modulesApi.setItemProfile,
  setItemMembers: modulesApi.setItemMembers,
  copyItem: modulesApi.copyItem,
  setItemTemplate: modulesApi.setItemTemplate,
  setModuleIcon: modulesApi.setModuleIcon,
  setModuleDescription: modulesApi.setModuleDescription,
  taskBacklinks: referencesApi.taskBacklinks,
  trashDocument: systemApi.trashDocument,
  trashItem: systemApi.trashItem,
  trashModule: systemApi.trashModule,
  trash: readTrash,
  restoreTrash: systemApi.restoreTrash,
  purgeTrash: systemApi.purgeTrash,
  purgeExpiredTrash: systemApi.purgeExpiredTrash,
  setVersionRetention: systemApi.setVersionRetention,
  setTrashRetention: systemApi.setTrashRetention,
  calendar: calendarApi.calendar,
  revisions: revisionsApi.revisions,
  revision: revisionsApi.revision,
  exportRevision: revisionsApi.exportRevision,
  search: searchApi.search,
  referenceCandidates: referencesApi.referenceCandidates,
  resolveReferences: referencesApi.resolveReferences,
  documentBacklinks: referencesApi.documentBacklinks,
  itemBacklinks: referencesApi.itemBacklinks,
  modules: modulesApi.modules,
  module: modulesApi.module,
  createModule: modulesApi.createModule,
  updateModule: modulesApi.updateModule,
  archiveModule: modulesApi.archiveModule,
  items: modulesApi.items,
  createItem: modulesApi.createItem,
  item: modulesApi.item,
  updateItem: modulesApi.updateItem,
  archiveItem: modulesApi.archiveItem,
  itemDocuments: documentsApi.itemDocuments,
  createItemDocument: documentsApi.createItemDocument,
  publicLinks: documentsApi.publicLinks,
  createPublicLink: documentsApi.createPublicLink,
  revokePublicLink: documentsApi.revokePublicLink,
  publicDocument: documentsApi.publicDocument,
};

/** 요청 ID. 재시도 때 같은 값을 다시 보내면 서버가 중복을 막는다. */
export function newRequestId(): string {
  return crypto.randomUUID();
}
