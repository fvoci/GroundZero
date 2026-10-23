-- Stable object identities, never physical OIDs. The pg_dump snapshot complements this catalog.
WITH objects AS (
 SELECT 'schema' AS kind, nspname::text AS identity,
   jsonb_build_object('owner',nspowner::regrole::text,'acl',ARRAY(SELECT x::text FROM unnest(coalesce(nspacl,acldefault('n',nspowner))) x ORDER BY x::text)) AS definition
 FROM pg_namespace WHERE nspname !~ '^pg_(toast_)?temp_'
 UNION ALL
 SELECT 'function', p.oid::regprocedure::text,
   jsonb_build_object('definition',pg_get_functiondef(p.oid),'owner',proowner::regrole::text,
     'language',l.lanname,'security_definer',prosecdef,'strict',proisstrict,'volatility',provolatile,
     'parallel',proparallel,'leakproof',proleakproof,'cost',procost,'rows',prorows,
     'config',ARRAY(SELECT x FROM unnest(proconfig) x ORDER BY x),
     'acl',ARRAY(SELECT x::text FROM unnest(coalesce(proacl,acldefault('f',proowner))) x ORDER BY x::text))
 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace JOIN pg_language l ON l.oid=p.prolang
 WHERE (n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' OR p.proname='uuid_extract_version') AND prokind <> 'a'
 UNION ALL
 SELECT 'relation', c.oid::regclass::text,
   jsonb_build_object('kind',relkind,'owner',relowner::regrole::text,'rls',relrowsecurity,'force_rls',relforcerowsecurity,
     'persistence',relpersistence,'replica_identity',relreplident,'options',reloptions,
     'acl',ARRAY(SELECT x::text FROM unnest(coalesce(relacl,acldefault(CASE WHEN relkind='S' THEN 's'::"char" ELSE 'r'::"char" END,relowner))) x ORDER BY x::text))
 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
 UNION ALL
 SELECT 'column', c.oid::regclass::text||'.'||a.attname,
   jsonb_build_object('type',format_type(a.atttypid,a.atttypmod),'not_null',attnotnull,
     'identity',attidentity,'generated',attgenerated,'default',pg_get_expr(d.adbin,d.adrelid),
     'collation',CASE WHEN a.attcollation<>0 THEN a.attcollation::regcollation::text END,
     'storage',attstorage,'compression',attcompression,'options',attoptions,
     'acl',ARRAY(SELECT x::text FROM unnest(attacl) x ORDER BY x::text))
 FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN pg_namespace n ON n.oid=c.relnamespace
 LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
 WHERE n.nspname='fvoci' AND a.attnum>0 AND NOT a.attisdropped
 UNION ALL
 SELECT 'constraint', coalesce(NULLIF(conrelid,0)::regclass::text,NULLIF(contypid,0)::regtype::text)||'.'||conname,
   jsonb_build_object('definition',pg_get_constraintdef(c.oid,true),'validated',convalidated,'deferrable',condeferrable,'deferred',condeferred)
 FROM pg_constraint c JOIN pg_namespace n ON n.oid=c.connamespace WHERE n.nspname='fvoci'
 UNION ALL
 SELECT 'index', indexrelid::regclass::text,
   jsonb_build_object('definition',pg_get_indexdef(indexrelid),'valid',indisvalid,'ready',indisready,'clustered',indisclustered,'replica_identity',indisreplident)
 FROM pg_index i JOIN pg_class c ON c.oid=i.indexrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='fvoci'
 UNION ALL
 SELECT 'policy', polrelid::regclass::text||'.'||polname,
   jsonb_build_object('command',polcmd,'permissive',polpermissive,
     'roles',ARRAY(SELECT CASE WHEN x=0 THEN 'PUBLIC' ELSE x::regrole::text END FROM unnest(polroles) x ORDER BY 1),
     'using',pg_get_expr(polqual,polrelid),'check',pg_get_expr(polwithcheck,polrelid))
 FROM pg_policy p JOIN pg_class c ON c.oid=p.polrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='fvoci'
 UNION ALL
 SELECT 'trigger', tgrelid::regclass::text||'.'||CASE WHEN tgisinternal THEN coalesce(k.conname,'')||'/'||tgfoid::regprocedure::text||'/'||tgtype::text ELSE tgname::text END,
   jsonb_build_object('definition',CASE WHEN NOT tgisinternal THEN pg_get_triggerdef(t.oid,true) END,
     'internal',tgisinternal,'enabled',tgenabled,'deferrable',tgdeferrable,'deferred',tginitdeferred)
 FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace
 LEFT JOIN pg_constraint k ON k.oid=t.tgconstraint WHERE n.nspname='fvoci'
 UNION ALL
 SELECT 'type', t.oid::regtype::text,
   jsonb_build_object('kind',typtype,'category',typcategory,'owner',typowner::regrole::text,
     'base',CASE WHEN typbasetype<>0 THEN typbasetype::regtype::text END,'default',typdefault,
     'not_null',typnotnull,'acl',ARRAY(SELECT x::text FROM unnest(coalesce(typacl,acldefault('T',typowner))) x ORDER BY x::text),
     'enum',ARRAY(SELECT enumlabel::text FROM pg_enum WHERE enumtypid=t.oid ORDER BY enumsortorder))
 FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='fvoci'
 UNION ALL
 SELECT 'sequence', seqrelid::regclass::text,
   to_jsonb(s)-'seqrelid'-'seqtypid'||jsonb_build_object('type',seqtypid::regtype::text)
 FROM pg_sequence s JOIN pg_class c ON c.oid=s.seqrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='fvoci'
 UNION ALL
 SELECT 'default_acl', defaclrole::regrole::text||'.'||coalesce(n.nspname,'*')||'.'||defaclobjtype::text,
   jsonb_build_object('acl',ARRAY(SELECT x::text FROM unnest(defaclacl) x ORDER BY x::text))
 FROM pg_default_acl d LEFT JOIN pg_namespace n ON n.oid=d.defaclnamespace
 UNION ALL
 SELECT 'role', rolname, to_jsonb(r)-'oid'-'rolpassword' FROM pg_roles r
 UNION ALL
 SELECT 'membership', roleid::regrole::text||'->'||member::regrole::text||'/'||grantor::regrole::text,
   jsonb_build_object('admin',admin_option,'inherit',inherit_option,'set',set_option)
 FROM pg_auth_members
 UNION ALL
 SELECT 'database', 'current', to_jsonb(d)-'oid'-'datname'-'datfrozenxid'-'datminmxid'-'dattablespace'-'datdba'-'datacl'
   ||jsonb_build_object('owner',datdba::regrole::text,'tablespace',(SELECT spcname FROM pg_tablespace WHERE oid=d.dattablespace),
     'acl',ARRAY(SELECT x::text FROM unnest(coalesce(datacl,acldefault('d',datdba))) x ORDER BY x::text))
 FROM pg_database d WHERE datname=current_database()
 UNION ALL
 SELECT 'setting', coalesce(r.rolname,'*')||'/'||CASE WHEN s.setdatabase=0 THEN '*' ELSE 'current' END,
   jsonb_build_object('config',ARRAY(SELECT x FROM unnest(setconfig) x ORDER BY x))
 FROM pg_db_role_setting s LEFT JOIN pg_roles r ON r.oid=s.setrole
 WHERE setdatabase=0 OR setdatabase=(SELECT oid FROM pg_database WHERE datname=current_database())
 UNION ALL
 SELECT 'extension', extname,
   jsonb_build_object('version',extversion,'owner',extowner::regrole::text,'schema',n.nspname,'relocatable',extrelocatable,
     'config',ARRAY(SELECT x::regclass::text FROM unnest(extconfig) x),'condition',extcondition)
 FROM pg_extension e JOIN pg_namespace n ON n.oid=e.extnamespace
 UNION ALL
 SELECT 'event_trigger', evtname, jsonb_build_object('event',evtevent,'owner',evtowner::regrole::text,
   'function',evtfoid::regprocedure::text,'enabled',evtenabled,'tags',evttags)
 FROM pg_event_trigger
)
SELECT kind, identity, definition FROM objects ORDER BY kind, identity;
