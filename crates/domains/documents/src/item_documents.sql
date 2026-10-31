-- item_id implies module placement (documents_module_shape). The scalar
-- subquery reads the shared item key once, retaining items/module_keys RLS
-- and NULL when the item or current key is not visible.
-- documents_tree_check keeps parents in the same item. Reuse this statement's
-- RLS-visible rows to mask hidden parents without evaluating their access twice.
WITH visible AS MATERIALIZED (
    SELECT id, title, revision, updated_at, parent_id, number, position, created_at
    FROM documents WHERE item_id = $1
)
SELECT d.id,
       (SELECT k.key FROM fvoci.items i
        JOIN fvoci.module_keys k ON k.module_id = i.module_id AND k.retired_at IS NULL
        WHERE i.id = $1) || '-' || d.number AS reference,
       d.title, d.revision, d.updated_at, p.id AS parent_id
FROM visible d LEFT JOIN visible p ON p.id = d.parent_id
ORDER BY d.position, d.number NULLS LAST, d.created_at, d.id
