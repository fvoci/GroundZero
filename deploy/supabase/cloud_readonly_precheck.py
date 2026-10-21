#!/usr/bin/env python3
"""One bounded, read-only Supabase Cloud session-pooler catalog inspection.

No connection is attempted without --connect. See docs/supabase-cloud-readonly-precheck.md.
All connection inputs and psql output remain private to this process; only the
strict bool/int catalog schema below may leave it. No application code is run.
"""

import hashlib
import hmac
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from urllib.parse import unquote, urlsplit

SECRET_NAME = "SUPABASE_TEST_DATABASE_ADMIN_URL_SESSION_POOLER"
CA_FILE = "/etc/ssl/certs/ca-certificates.crt"
CONNECTED_MARKER = "FVOCI_PRECHECK_CONNECTED_TLS_VERIFIED"
PROCESS_TIMEOUT_SECONDS = 20

ROLE_KEYS = (
    "exists", "superuser", "create_role", "create_db", "bypass_rls", "login",
    "inherit", "replication", "member_with_admin_option", "can_set_role",
)
CATALOG_BOOL_KEYS = (
    "transaction_read_only", "current_account_matches", "current_database_matches",
    "database_connect", "database_create", "database_temporary", "fvoci_schema_exists",
    "fvoci_schema_owned_by_migrator", "fvoci_schema_usage", "fvoci_schema_create",
    "uuid_extract_version_catalog_present", "plpgsql_language_present",
    "createrole_self_grant_setting_present", "supautils_settings_present",
    "session_replication_role_set_catalog_privilege",
)
ROLE_NAMES = ("current", "fvoci_app", "fvoci_migrator")

# These are fixed statements, never assembled from the URI or database content.
# The first marker is emitted only after libpq connects with verify-full. It
# records client -> pooler TLS, not pg_stat_ssl's pooler -> database connection.
SQL = r"""\echo FVOCI_PRECHECK_CONNECTED_TLS_VERIFIED
BEGIN TRANSACTION READ ONLY;
SET LOCAL statement_timeout = '5s';
SET LOCAL lock_timeout = '2s';
SET LOCAL idle_in_transaction_session_timeout = '5s';
SET LOCAL search_path = pg_catalog;
WITH wanted(label, role_name) AS (
  VALUES ('current', current_user::text),
         ('fvoci_app', 'fvoci_app'), ('fvoci_migrator', 'fvoci_migrator')
), roles AS (
  SELECT w.label, pg_catalog.jsonb_build_object(
    'exists', r.oid IS NOT NULL,
    'superuser', COALESCE(r.rolsuper, false),
    'create_role', COALESCE(r.rolcreaterole, false),
    'create_db', COALESCE(r.rolcreatedb, false),
    'bypass_rls', COALESCE(r.rolbypassrls, false),
    'login', COALESCE(r.rolcanlogin, false),
    'inherit', COALESCE(r.rolinherit, false),
    'replication', COALESCE(r.rolreplication, false),
    'member_with_admin_option', CASE WHEN r.oid IS NULL THEN false ELSE
      pg_catalog.pg_has_role(current_user, r.oid, 'MEMBER WITH ADMIN OPTION') END,
    'can_set_role', CASE WHEN r.oid IS NULL THEN false ELSE
      pg_catalog.pg_has_role(current_user, r.oid, 'SET') END
  ) AS attributes
  FROM wanted w LEFT JOIN pg_catalog.pg_roles r ON r.rolname = w.role_name
)
SELECT pg_catalog.jsonb_build_object(
  'server_version_num', pg_catalog.current_setting('server_version_num')::integer,
  'transaction_read_only', pg_catalog.current_setting('transaction_read_only') = 'on',
  'current_account_matches', current_user = 'postgres' AND session_user = 'postgres',
  'current_database_matches', pg_catalog.current_database() = 'postgres',
  'database_connect', pg_catalog.has_database_privilege(
      current_user, pg_catalog.current_database(), 'CONNECT'),
  'database_create', pg_catalog.has_database_privilege(
      current_user, pg_catalog.current_database(), 'CREATE'),
  'database_temporary', pg_catalog.has_database_privilege(
      current_user, pg_catalog.current_database(), 'TEMPORARY'),
  'roles', (SELECT pg_catalog.jsonb_object_agg(label, attributes) FROM roles),
  'fvoci_schema_exists', EXISTS (
      SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname = 'fvoci'),
  'fvoci_schema_owned_by_migrator', EXISTS (
      SELECT 1 FROM pg_catalog.pg_namespace n
      JOIN pg_catalog.pg_roles r ON r.oid = n.nspowner
      WHERE n.nspname = 'fvoci' AND r.rolname = 'fvoci_migrator'),
  'fvoci_schema_usage', COALESCE((
      SELECT pg_catalog.has_schema_privilege(current_user, oid, 'USAGE')
      FROM pg_catalog.pg_namespace WHERE nspname = 'fvoci'), false),
  'fvoci_schema_create', COALESCE((
      SELECT pg_catalog.has_schema_privilege(current_user, oid, 'CREATE')
      FROM pg_catalog.pg_namespace WHERE nspname = 'fvoci'), false),
  'uuid_extract_version_catalog_present',
      pg_catalog.to_regprocedure('pg_catalog.uuid_extract_version(uuid)') IS NOT NULL,
  'plpgsql_language_present', EXISTS (
      SELECT 1 FROM pg_catalog.pg_language WHERE lanname = 'plpgsql'),
  'createrole_self_grant_setting_present', EXISTS (
      SELECT 1 FROM pg_catalog.pg_settings WHERE name = 'createrole_self_grant'),
  'supautils_settings_present', EXISTS (
      SELECT 1 FROM pg_catalog.pg_settings WHERE name LIKE 'supautils.%'),
  'session_replication_role_set_catalog_privilege',
      pg_catalog.has_parameter_privilege(current_user, 'session_replication_role', 'SET')
);
ROLLBACK;
"""


class SafeError(Exception):
    """Only internal, fixed error codes belong here; never attach an input."""

    def __init__(self, code):
        known = {
            "invalid_uri", "target_approval_missing_or_invalid",
            "target_confirmation_required", "uri_options_not_allowed",
            "account_or_project_not_allowed", "session_pooler_5432_required",
            "shared_pooler_host_required", "target_not_allowed",
            "invalid_catalog_output", "trusted_ca_file_unavailable",
            "connection_tls_or_query_failed", "read_only_or_identity_check_failed",
        }
        super().__init__(code if code in known else "internal_failure")


@dataclass(frozen=True, repr=False)
class Target:
    project_ref: str
    host: str
    user: str
    password: str


def target_digest(project_ref, host):
    """Hash independent Dashboard metadata, never derive the allowlist from a URL."""
    canonical = (
        "supabase-cloud-readonly-v1\n" + project_ref + "\n" + host
        + "\npostgres\n5432\npostgres\n"
    )
    return hashlib.sha256(canonical.encode("ascii")).hexdigest()


def decode_component(value):
    if re.search(r"%(?![0-9a-fA-F]{2})", value):
        raise SafeError("invalid_uri")
    try:
        decoded = unquote(value, encoding="utf-8", errors="strict")
    except UnicodeError:
        raise SafeError("invalid_uri") from None
    if not decoded or any(ord(c) < 32 or ord(c) == 127 for c in decoded):
        raise SafeError("invalid_uri")
    return decoded


def parse_target(uri, approved_digest, confirmed):
    """Accept one exact, independently approved Cloud session-pooler target."""
    if not re.fullmatch(r"[0-9a-f]{64}", approved_digest):
        raise SafeError("target_approval_missing_or_invalid")
    if confirmed != "true":
        raise SafeError("target_confirmation_required")
    if not uri or len(uri) > 8192 or any(ord(c) <= 32 or ord(c) == 127 for c in uri):
        raise SafeError("invalid_uri")
    try:
        parsed = urlsplit(uri)
        if parsed.scheme not in ("postgres", "postgresql") or parsed.path != "/postgres":
            raise SafeError("invalid_uri")
        # Reject all query options and fragments, including empty delimiters.
        if "?" in uri or "#" in uri or parsed.netloc.count("@") != 1:
            raise SafeError("uri_options_not_allowed")
        auth, authority = parsed.netloc.split("@")
        raw_user, raw_password = auth.split(":", 1)
        user, password = decode_component(raw_user), decode_component(raw_password)
        if ":" in raw_password:
            raise SafeError("invalid_uri")
        match = re.fullmatch(r"postgres\.([a-z]{20})", user)
        if not match:
            raise SafeError("account_or_project_not_allowed")
        host, port = authority.rsplit(":", 1)
        if port != "5432":
            raise SafeError("session_pooler_5432_required")
        if not re.fullmatch(r"aws-[0-9]+-[a-z0-9]+(?:-[a-z0-9]+)*\.pooler\.supabase\.com", host):
            raise SafeError("shared_pooler_host_required")
        project_ref = match.group(1)
        if not hmac.compare_digest(target_digest(project_ref, host), approved_digest):
            raise SafeError("target_not_allowed")
        return Target(project_ref, host, user, password)
    except (ValueError, UnicodeError):
        raise SafeError("invalid_uri") from None


def child_env(target, home):
    """Discard inherited libpq, loader, proxy and psql settings."""
    return {
        "PATH": os.defpath,
        "HOME": home,
        "LC_ALL": "C",
        "PGHOST": target.host,
        "PGPORT": "5432",
        "PGDATABASE": "postgres",
        "PGUSER": target.user,
        "PGPASSWORD": target.password,
        "PGSSLMODE": "verify-full",
        "PGSSLROOTCERT": CA_FILE,
        "PGGSSENCMODE": "disable",
        "PGCONNECT_TIMEOUT": "5",
        "PGPASSFILE": "/dev/null",
        "PGSERVICEFILE": "/dev/null",
        "PGAPPNAME": "fvoci-cloud-readonly-precheck",
        "PGOPTIONS": "-c default_transaction_read_only=on -c statement_timeout=5000 "
                     "-c lock_timeout=2000 -c idle_in_transaction_session_timeout=5000 "
                     "-c search_path=pg_catalog",
    }


def validate_catalog(value):
    """Reject extra keys and arbitrary strings even from an authenticated server."""
    if type(value) is not dict or set(value) != set(CATALOG_BOOL_KEYS) | {"server_version_num", "roles"}:
        raise SafeError("invalid_catalog_output")
    version = value["server_version_num"]
    if type(version) is not int or not 100000 <= version <= 999999:
        raise SafeError("invalid_catalog_output")
    if any(type(value[key]) is not bool for key in CATALOG_BOOL_KEYS):
        raise SafeError("invalid_catalog_output")
    roles = value["roles"]
    if type(roles) is not dict or set(roles) != set(ROLE_NAMES):
        raise SafeError("invalid_catalog_output")
    for role in roles.values():
        if type(role) is not dict or set(role) != set(ROLE_KEYS):
            raise SafeError("invalid_catalog_output")
        if any(type(flag) is not bool for flag in role.values()):
            raise SafeError("invalid_catalog_output")
    return value


def empty_report():
    return {
        "connection_kind": "session_pooler",
        "account_purpose": "ADMIN",
        "connection": "NOT RUN",
        "tls_certificate_and_hostname": "NOT RUN",
        "read_only_catalog": "NOT RUN",
        "direct_connection": "NOT RUN",
        "mutation_tests": "NOT RUN",
    }


def run_precheck(environ):
    report = empty_report()
    try:
        target = parse_target(
            environ.get(SECRET_NAME, ""), environ.get("APPROVED_TARGET_SHA256", ""),
            environ.get("CONFIRM_DISPOSABLE_TARGET", ""),
        )
        if not os.path.isfile(CA_FILE):
            raise SafeError("trusted_ca_file_unavailable")
        with tempfile.TemporaryDirectory(prefix="fvoci-cloud-precheck-") as home:
            report["connection"] = "UNCONFIRMED"
            report["tls_certificate_and_hostname"] = "UNCONFIRMED"
            result = subprocess.run(
                ["psql", "-X", "-w", "-A", "-t", "-q", "--set=ON_ERROR_STOP=1", "--file=-"],
                input=SQL, capture_output=True, text=True, encoding="utf-8", errors="strict",
                timeout=PROCESS_TIMEOUT_SECONDS, env=child_env(target, home), check=False,
            )
        lines = result.stdout.splitlines()
        if lines and lines[0] == CONNECTED_MARKER:
            report["connection"] = "PASS"
            report["tls_certificate_and_hostname"] = "PASS"
            report["read_only_catalog"] = "FAIL"
        if result.returncode != 0:
            raise SafeError("connection_tls_or_query_failed")
        if len(lines) != 2 or lines[0] != CONNECTED_MARKER or len(lines[1]) > 16384:
            raise SafeError("invalid_catalog_output")
        catalog = validate_catalog(json.loads(lines[1]))
        report["catalog"] = catalog
        if not all(catalog[key] for key in (
            "transaction_read_only", "current_account_matches", "current_database_matches",
        )):
            raise SafeError("read_only_or_identity_check_failed")
        report["read_only_catalog"] = "PASS"
        return report, 0
    except SafeError as error:
        report["failure_code"] = str(error)
    except subprocess.TimeoutExpired:
        # Never print TimeoutExpired: it can contain captured credentials/errors.
        report["failure_code"] = "process_timeout"
    except (OSError, UnicodeError, ValueError):
        report["failure_code"] = "client_or_output_failure"
    except Exception:
        # A last boundary prevents traceback text from exposing connection inputs.
        report["failure_code"] = "internal_failure"
    return report, 1


def main(argv=None):
    arguments = sys.argv[1:] if argv is None else argv
    if arguments != ["--connect"]:
        report = empty_report()
        report["failure_code"] = "explicit_connect_required"
        code = 2
    else:
        report, code = run_precheck(os.environ)
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return code


if __name__ == "__main__":
    sys.exit(main())
