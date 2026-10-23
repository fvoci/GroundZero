# 테이블 RLS와 ACL

> **역사 기록이며 현재 상태를 나타내지 않음.** 본문 판단·상태·결과는 명시된 조사/실행 SHA에만 적용한다. 관련 결함이나 SHA를 조사할 때만 참조하며, 현재 기준과 분리 검증은 [증거 안내](../evidence.md)를 따른다.

기준과 열·제약·인덱스 상세는 [목록 설명](README.md)과 [보존 카탈로그](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/inventory/catalog-postgres.jsonl)를 따른다. 모두 소유자는 fvoci_migrator다.
아래는 테이블 ACL이다. 열 단위 권한은 별도 column 항목에 있다. ACL 문자는 PostgreSQL 원문을 보존했다.

| 테이블                      | RLS  | FORCE RLS | 테이블 ACL                                                              |
| --------------------------- | ---- | --------- | ----------------------------------------------------------------------- |
| `audit_log`                 | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `checklist_items`           | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `comments`                  | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `course_details`            | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `course_episodes`           | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `document_collab_file_refs` | 꺼짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `document_collab_states`    | 켜짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `document_file_refs`        | 켜짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `document_links`            | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `document_mentions`         | 켜짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `document_revisions`        | 켜짐 | 꺼짐      | `fvoci_app=ar/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator` |
| `document_shares`           | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `documents`                 | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `episode_progress`          | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `events`                    | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `file_versions`             | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `files`                     | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `group_members`             | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `groups`                    | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `instance_state`            | 켜짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `invitations`               | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `item_members`              | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `item_records`              | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `items`                     | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `module_keys`               | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `modules`                   | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `notifications`             | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `numbered_targets`          | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `oidc_login_states`         | 꺼짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `ownership_offers`          | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `public_links`              | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `record_files`              | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `reference_counters`        | 켜짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `saved_views`               | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `scheduled_runs`            | 꺼짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `search_queue`              | 켜짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `sessions`                  | 꺼짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `storage_deletions`         | 켜짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `study_units`               | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `task_assignees`            | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `task_dependencies`         | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `task_series`               | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `tasks`                     | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `time_entries`              | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `time_entry_revisions`      | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `time_sharing`              | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `trash_batches`             | 켜짐 | 꺼짐      | `fvoci_migrator=arwdDxtm/fvoci_migrator`                                |
| `unit_progress`             | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `upload_parts`              | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `uploads`                   | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `user_marks`                | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `user_settings`             | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `users`                     | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `workspace_members`         | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
| `workspaces`                | 켜짐 | 꺼짐      | `fvoci_app=r/fvoci_migrator`, `fvoci_migrator=arwdDxtm/fvoci_migrator`  |
