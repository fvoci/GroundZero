-- Install reference functions BEFORE any function experiments, only in held T6 DB.
SELECT replace(pg_get_functiondef('fvoci.app_recent_activity(uuid,integer)'::regprocedure),
               'FUNCTION fvoci.app_recent_activity(', 'FUNCTION fvoci.perf240_recent_reference(') \gexec
ALTER FUNCTION fvoci.perf240_recent_reference(uuid,integer) OWNER TO fvoci_migrator;
REVOKE ALL ON FUNCTION fvoci.perf240_recent_reference(uuid,integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fvoci.perf240_recent_reference(uuid,integer) TO fvoci_app;
SELECT replace(pg_get_functiondef('fvoci.app_grade_document_at(uuid,uuid,uuid,uuid,uuid)'::regprocedure),
               'FUNCTION fvoci.app_grade_document_at(', 'FUNCTION fvoci.perf240_document_reference(') \gexec
ALTER FUNCTION fvoci.perf240_document_reference(uuid,uuid,uuid,uuid,uuid) OWNER TO fvoci_migrator;
REVOKE ALL ON FUNCTION fvoci.perf240_document_reference(uuid,uuid,uuid,uuid,uuid) FROM PUBLIC;
-- Document reference remains internal; comparisons run as the original function owner.
