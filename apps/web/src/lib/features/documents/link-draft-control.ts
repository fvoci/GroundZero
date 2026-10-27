// 서식 막대(BubbleToolbar)의 링크 주소 초안을 편집기 본문 위(TiptapEditor)에서 고르게 하는 연결. 말풍선은 글자를 골라야
// 나타나 그 안의 제안은 새로고침 뒤 보이지 않을 수 있었다(⑱).
import type { StoredFormDraft } from '$lib/features/common/form-draft';

export type LinkDraftOffer = StoredFormDraft<{ savedAt: string; input: string }>;

export interface LinkDraftControl {
  /** 남은 링크 주소 초안의 제안. */
  offers: () => LinkDraftOffer[];
  restore: (offer: LinkDraftOffer) => void;
  discard: (offer: LinkDraftOffer) => void;
  /** 남긴 링크 주소를 붙일 글자를 아직 고르지 않았는지(고르라고 안내한다). */
  waitsForSelection: () => boolean;
}
