-- 항목의 '고친 날'을 다시 색인하는 때 (코드 전반 감사 델타 SD2).
--
-- 검색 색인의 항목 날짜(`day`)는 `updated_at`의 날짜다. 그런데 항목 색인 트리거는 제목·설명·삭제·공개 범위·보관이 바뀔
-- 때만 돌아, 항목 정보(상태·리더·목표 등, `app_set_item_profile`)나 템플릿 표시(`app_set_item_template`)만 바꾸면
-- `updated_at`은 바뀌어도 다시 색인되지 않았다. 그래서 같은 날짜 필터가 Meilisearch 경로(옛 날짜)와 대체(DB) 경로(지금의
-- `updated_at`)에서 다른 결과를 냈다. 트리거가 `updated_at`이 바뀔 때도 돌게 하고, 이미 어긋났을 수 있는 항목을 다시
-- 대기열에 올린다(검색 색인은 다시 만들 수 있는 파생 자료다).

DROP TRIGGER items_search ON items;
CREATE TRIGGER items_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, visibility,
  archived_at, updated_at ON items FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('item');

INSERT INTO search_queue (kind, target_id)
SELECT 'item', id FROM items WHERE deleted_at IS NULL
ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
