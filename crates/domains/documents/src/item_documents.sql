-- item_id implies module placement (documents_module_shape). The scalar
-- subquery reads the shared item key once, retaining items/module_keys RLS
-- and NULL when the item or current key is not visible.
SELECT d.id,
       (SELECT k.key FROM fvoci.items i
        JOIN fvoci.module_keys k ON k.module_id = i.module_id AND k.retired_at IS NULL
        WHERE i.id = $1) || '-' || d.number AS reference,
       d.title, d.revision, d.updated_at,
       CASE WHEN app_access('document', d.parent_id) <> 'none' THEN d.parent_id END AS parent_id
FROM documents d WHERE d.item_id = $1
ORDER BY d.position, d.number NULLS LAST, d.created_at, d.id
