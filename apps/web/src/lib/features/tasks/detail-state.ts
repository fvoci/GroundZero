import type {
  LinkKind,
  LinkBase,
  ShiftProposalView,
  UndatedTaskView,
  PlanBlockedView,
} from './model';
export interface LinkPending {
  predecessor: string;
  kind: LinkKind;
  lag: number;
  base: LinkBase | null;
  proposals: ShiftProposalView[];
  undated: UndatedTaskView[];
  blocked: PlanBlockedView | null;
  retried: boolean;
}
