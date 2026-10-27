// 홈 초대 목록과 알림함은 같은 초대의 변경 뒤 각자 서버 정본을 다시 읽는다(D-118).
class InvitationChanges {
  version = $state(0);
  changed(): void {
    this.version += 1;
  }
}
export const invitationChanges = new InvitationChanges();
