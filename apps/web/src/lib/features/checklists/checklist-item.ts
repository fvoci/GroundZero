/** 일반 체크리스트의 한 항목. 저장 구조·권한 연결은 첫 사용자 흐름에서 정한다(결정 D-30, O-04). */
export interface ChecklistItem {
  id: string;
  text: string;
  done: boolean;
}
