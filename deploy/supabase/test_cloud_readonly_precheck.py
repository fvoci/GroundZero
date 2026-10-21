"""Offline safety tests. Every credential and psql response here is synthetic."""

import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "cloud_readonly_precheck", HERE / "cloud_readonly_precheck.py"
)
precheck = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = precheck
SPEC.loader.exec_module(precheck)

# Deliberately fake credentials; tests never resolve this hostname or contact a DB.
PROJECT = "abcdefghijklmnopqrst"
HOST = "aws-0-fake-region-1.pooler.supabase.com"
PASSWORD = "FAKE-password!"
USER = f"postgres.{PROJECT}"
URI = f"postgresql://{USER}:FAKE-password%21@{HOST}:5432/postgres"
SECRET_NAME = "SUPABASE_TEST_DATABASE_ADMIN_URL_SESSION_POOLER"
CANONICAL_TARGET = (
    f"supabase-cloud-readonly-v1\n{PROJECT}\n{HOST}\npostgres\n5432\npostgres\n"
)
DIGEST = hashlib.sha256(CANONICAL_TARGET.encode("utf-8")).hexdigest()


def synthetic_env():
    return {
        SECRET_NAME: URI,
        "APPROVED_TARGET_SHA256": DIGEST,
        "CONFIRM_DISPOSABLE_TARGET": "true",
        "PATH": "/definitely-no-real-psql-in-offline-tests",
    }


def synthetic_catalog():
    value = {key: False for key in precheck.CATALOG_BOOL_KEYS}
    value.update(
        server_version_num=170006,
        transaction_read_only=True,
        current_account_matches=True,
        current_database_matches=True,
        database_connect=True,
        roles={
            role: {key: False for key in precheck.ROLE_KEYS}
            for role in precheck.ROLE_NAMES
        },
    )
    value["roles"]["current"].update(exists=True, login=True)
    return value


def synthetic_output(catalog=None):
    if catalog is None:
        catalog = synthetic_catalog()
    return precheck.CONNECTED_MARKER + "\n" + json.dumps(catalog) + "\n"


class OfflineTestCase(unittest.TestCase):
    def setUp(self):
        # No Python path in these tests may resolve a hostname or open a socket.
        # The sole real child process below is a generated recorder, not psql.
        for name in ("socket", "getaddrinfo", "create_connection"):
            guard = mock.patch.object(
                socket, name, side_effect=AssertionError("Network access is forbidden in offline tests")
            )
            guard.start()
            self.addCleanup(guard.stop)


class TargetValidationTests(OfflineTestCase):
    def test_canonical_target_and_password_decoding(self):
        target = precheck.parse_target(URI, DIGEST, "true")
        self.assertEqual(target.project_ref, PROJECT)
        self.assertEqual(target.host, HOST)
        self.assertEqual(target.user, USER)
        self.assertEqual(target.password, PASSWORD)
        self.assertEqual(precheck.target_digest(PROJECT, HOST), DIGEST)
        for sensitive in (PROJECT, HOST, USER, PASSWORD):
            self.assertNotIn(sensitive, repr(target))

    def test_target_requires_approved_fingerprint_and_disposable_confirmation(self):
        for approved, confirmed in (
            ("", ""),
            (DIGEST, ""),
            ("", "true"),
            ("0" * 64, "true"),
            (DIGEST, "false"),
            (DIGEST, "TRUE"),
            (DIGEST, "true\n"),
            (DIGEST, DIGEST),
            (DIGEST.upper(), "true"),
            (DIGEST + "\n", "true"),
        ):
            with self.subTest(approved=approved, confirmed=confirmed):
                with self.assertRaises(precheck.SafeError):
                    precheck.parse_target(URI, approved, confirmed)

    def test_rejects_non_session_targets_and_connection_overrides(self):
        invalid = {
            "missing": "",
            "conninfo": "host=localhost user=postgres password=FAKE-password",
            "scheme": URI.replace("postgresql:", "https:"),
            "transaction": URI.replace(":5432/", ":6543/"),
            "direct": URI.replace(HOST, f"db.{PROJECT}.supabase.co"),
            "no_port": URI.replace(":5432/", "/"),
            "other_database": URI.replace("/postgres", "/template1"),
            "custom_role": URI.replace("postgres.", "fvoci_app.", 1),
            "missing_project": URI.replace(USER, "postgres"),
            "other_project": URI.replace(PROJECT, "abcdefghijklmnopqrsz"),
            "other_host": URI.replace(HOST, "aws-1-fake-region-1.pooler.supabase.com"),
            "suffix_attack": URI.replace(HOST, HOST + ".example.invalid"),
            "localhost": URI.replace(HOST, "127.0.0.1"),
            "multiple_hosts": URI.replace(HOST, HOST + ",example.invalid"),
            "no_password": URI.replace(":FAKE-password%21", ""),
            "empty_password": URI.replace("FAKE-password%21", ""),
            "raw_newline": URI + "\n",
            "encoded_newline": URI.replace("FAKE-password%21", "FAKE%0apassword"),
            "encoded_nul": URI.replace("FAKE-password%21", "FAKE%00password"),
            "bad_percent": URI.replace("%21", "%zz"),
            "truncated_percent": URI.replace("%21", "%2"),
            "invalid_utf8": URI.replace("%21", "%ff"),
            "fragment": URI + "#ignored",
            "ssl_downgrade": URI + "?sslmode=require",
            "host_override": URI + "?host=example.invalid",
            "user_override": URI + "?user=other",
            "port_override": URI + "?port=6543",
            "service_override": URI + "?service=other",
            "options_override": URI + "?options=-c%20default_transaction_read_only=off",
        }
        for label, uri in invalid.items():
            with self.subTest(case=label):
                with self.assertRaises(precheck.SafeError):
                    precheck.parse_target(uri, DIGEST, "true")

    def test_child_environment_drops_ambient_connection_settings(self):
        target = precheck.parse_target(URI, DIGEST, "true")
        poison = {
            "PGHOST": "example.invalid",
            "PGPORT": "6543",
            "PGSERVICE": "hostile",
            "PGSERVICEFILE": "/hostile/service",
            "PGPASSFILE": "/hostile/passwords",
            "PGSSLMODE": "disable",
            "PGSSLROOTCERT": "/hostile/root.pem",
            "PGOPTIONS": "-c default_transaction_read_only=off",
            "LD_PRELOAD": "/hostile/interpose.so",
            "PSQLRC": "/hostile/startup",
            "UNRELATED_SECRET": "FAKE-unrelated-secret",
        }
        with mock.patch.dict(os.environ, poison, clear=True):
            env = precheck.child_env(target, "/offline/empty-home")
        self.assertEqual(env["PGHOST"], HOST)
        self.assertEqual(env["PGPORT"], "5432")
        self.assertEqual(env["PGUSER"], USER)
        self.assertEqual(env["PGPASSWORD"], PASSWORD)
        self.assertEqual(env["PGDATABASE"], "postgres")
        self.assertEqual(env["PGSSLMODE"], "verify-full")
        self.assertEqual(env["PGSSLROOTCERT"], "/etc/ssl/certs/ca-certificates.crt")
        self.assertEqual(env["PGCONNECT_TIMEOUT"], "5")
        self.assertEqual(env["PGGSSENCMODE"], "disable")
        self.assertIn("default_transaction_read_only=on", env["PGOPTIONS"])
        self.assertIn("statement_timeout=5000", env["PGOPTIONS"])
        self.assertIn("idle_in_transaction_session_timeout=5000", env["PGOPTIONS"])
        for key, value in poison.items():
            self.assertNotEqual(env.get(key), value)
        self.assertNotIn(SECRET_NAME, env)


class ExecutionBoundaryTests(OfflineTestCase):
    def assert_private(self, report):
        text = json.dumps(report)
        for sensitive in (PROJECT, HOST, USER, PASSWORD, URI, "FAKE-unrelated-secret"):
            self.assertNotIn(sensitive, text)
        self.assertEqual(report["connection_kind"], "session_pooler")
        self.assertEqual(report["account_purpose"], "ADMIN")
        self.assertEqual(report["mutation_tests"], "NOT RUN")
        self.assertEqual(report["direct_connection"], "NOT RUN")

    def invoke(self, stdout=None, stderr="", returncode=0, exception=None):
        if stdout is None:
            stdout = synthetic_output()
        result = subprocess.CompletedProcess(["psql"], returncode, stdout, stderr)
        with (
            mock.patch.object(precheck.os.path, "isfile", return_value=True),
            mock.patch.object(precheck.subprocess, "run", return_value=result, side_effect=exception) as run,
        ):
            report, code = precheck.run_precheck(synthetic_env())
        self.assertEqual(run.call_count, 1, "No reconnect or TLS fallback is permitted")
        self.assert_private(report)
        return report, code, run.call_args

    def test_default_cli_never_reads_credentials_or_runs_a_client(self):
        for args in ([], ["--help"], ["--connect", URI], [URI]):
            with self.subTest(args_count=len(args)):
                output = io.StringIO()
                with (
                    mock.patch.object(precheck, "run_precheck") as run,
                    contextlib.redirect_stdout(output),
                    contextlib.redirect_stderr(io.StringIO()) as errors,
                ):
                    code = precheck.main(args)
                run.assert_not_called()
                self.assertNotEqual(code, 0)
                self.assertEqual(errors.getvalue(), "")
                report = json.loads(output.getvalue())
                self.assert_private(report)
                self.assertEqual(report["connection"], "NOT RUN")
                self.assertEqual(report["tls_certificate_and_hostname"], "NOT RUN")
                self.assertEqual(report["read_only_catalog"], "NOT RUN")

    def test_missing_configuration_or_ca_stops_before_psql(self):
        for missing in (SECRET_NAME, "APPROVED_TARGET_SHA256", "CONFIRM_DISPOSABLE_TARGET"):
            env = synthetic_env()
            del env[missing]
            with self.subTest(missing=missing), mock.patch.object(precheck.subprocess, "run") as run:
                report, code = precheck.run_precheck(env)
            run.assert_not_called()
            self.assertNotEqual(code, 0)
            self.assert_private(report)
            self.assertEqual(report["connection"], "NOT RUN")
        with (
            mock.patch.object(precheck.os.path, "isfile", return_value=False),
            mock.patch.object(precheck.subprocess, "run") as run,
        ):
            report, code = precheck.run_precheck(synthetic_env())
        run.assert_not_called()
        self.assertNotEqual(code, 0)
        self.assert_private(report)
        self.assertEqual(report["failure_code"], "trusted_ca_file_unavailable")

    def test_single_client_uses_bounded_read_only_query_without_uri_arguments(self):
        report, code, call = self.invoke(stderr=f"Ignored synthetic notice: {URI}")
        self.assertEqual(code, 0)
        self.assertEqual(report["connection"], "PASS")
        self.assertEqual(report["tls_certificate_and_hostname"], "PASS")
        self.assertEqual(report["read_only_catalog"], "PASS")
        self.assertEqual(report["catalog"]["server_version_num"], 170006)
        argv = call.args[0]
        for sensitive in (URI, PASSWORD, USER, HOST, PROJECT):
            self.assertNotIn(sensitive, " ".join(argv))
            self.assertNotIn(sensitive, call.kwargs["input"])
        for option in ("-X", "-w", "-A", "-t", "-q", "--set=ON_ERROR_STOP=1", "--file=-"):
            self.assertIn(option, argv)
        self.assertFalse(call.kwargs.get("shell", False))
        self.assertTrue(call.kwargs["capture_output"])
        self.assertEqual(call.kwargs["timeout"], 20)
        self.assertEqual(call.kwargs["env"]["PGSSLMODE"], "verify-full")
        self.assertEqual(call.kwargs["env"]["PGGSSENCMODE"], "disable")

    def test_connection_failure_never_marks_tls_pass_or_exposes_errors(self):
        report, code, _ = self.invoke(
            stdout="", stderr=f"certificate rejected for {HOST}: {URI}", returncode=2
        )
        self.assertNotEqual(code, 0)
        self.assertNotEqual(report["connection"], "PASS")
        self.assertNotEqual(report["tls_certificate_and_hostname"], "PASS")
        self.assertNotEqual(report["read_only_catalog"], "PASS")
        self.assertNotIn("catalog", report)

    def test_catalog_failure_keeps_successful_connection_separate(self):
        report, code, _ = self.invoke(
            stdout=precheck.CONNECTED_MARKER + "\n",
            stderr=f"query denied to {USER} in {URI}",
            returncode=3,
        )
        self.assertNotEqual(code, 0)
        self.assertEqual(report["connection"], "PASS")
        self.assertEqual(report["tls_certificate_and_hostname"], "PASS")
        self.assertEqual(report["read_only_catalog"], "FAIL")
        self.assertNotIn("catalog", report)

    def test_timeout_and_client_exceptions_do_not_dump_exception_details(self):
        exceptions = (
            subprocess.TimeoutExpired(["psql", URI], 20, output=URI, stderr=PASSWORD),
            OSError(URI),
            ValueError(HOST),
            RuntimeError(USER),
            precheck.SafeError(URI),
        )
        for error in exceptions:
            with self.subTest(error_type=type(error).__name__):
                report, code, _ = self.invoke(exception=error)
                self.assertNotEqual(code, 0)
                self.assertNotEqual(report["read_only_catalog"], "PASS")
                self.assertNotIn("catalog", report)

    def test_untrusted_output_cannot_escape_fixed_bool_integer_schema(self):
        cases = []
        for key, value in (
            ("host", HOST),
            ("server_version_num", USER),
            ("server_version_num", True),
            ("server_version_num", 10**30),
            ("transaction_read_only", "true"),
            ("roles", {USER: {}}),
        ):
            catalog = synthetic_catalog()
            catalog[key] = value
            cases.append((key, synthetic_output(catalog)))
        catalog = synthetic_catalog()
        catalog["roles"]["current"]["login"] = USER
        cases.append(("role_value", synthetic_output(catalog)))
        catalog = synthetic_catalog()
        catalog["roles"]["current"]["password"] = PASSWORD
        cases.append(("extra_role_key", synthetic_output(catalog)))
        cases.extend(
            (
                ("no_marker", json.dumps(synthetic_catalog())),
                ("raw_error", URI),
                ("invalid_json", precheck.CONNECTED_MARKER + "\n" + URI),
                ("extra_output", synthetic_output() + URI),
                ("oversize", precheck.CONNECTED_MARKER + "\n" + "x" * 17000),
            )
        )
        for label, stdout in cases:
            with self.subTest(case=label):
                report, code, _ = self.invoke(stdout=stdout)
                self.assertNotEqual(code, 0)
                self.assertNotEqual(report["read_only_catalog"], "PASS")
                self.assertNotIn("catalog", report)

    def test_read_only_and_account_database_checks_are_required(self):
        for key in ("transaction_read_only", "current_account_matches", "current_database_matches"):
            catalog = synthetic_catalog()
            catalog[key] = False
            with self.subTest(key=key):
                report, code, _ = self.invoke(stdout=synthetic_output(catalog))
                self.assertNotEqual(code, 0)
                self.assertEqual(report["read_only_catalog"], "FAIL")
                self.assertEqual(report["connection"], "PASS")

    def test_sql_contains_only_fixed_catalog_inspection_and_transaction_settings(self):
        sql = precheck.SQL
        self.assertRegex(sql, r"(?m)^BEGIN(?: TRANSACTION)? READ ONLY;")
        self.assertRegex(sql, r"ROLLBACK;\s*\Z")
        self.assertEqual(len(re.findall(r"(?m)^BEGIN\b", sql)), 1)
        self.assertEqual(len(re.findall(r"(?m)^ROLLBACK\b", sql)), 1)
        self.assertNotRegex(
            sql,
            r"(?im)^\s*(CREATE|ALTER|DROP|INSERT|UPDATE|DELETE|TRUNCATE|COPY|GRANT|REVOKE|CALL|DO|COMMIT|RESET)\b",
        )
        self.assertNotRegex(sql, r"(?im)^\s*SET(?: LOCAL)?\s+(ROLE|session_replication_role)\b")
        self.assertNotRegex(sql, r"(?im)pg_(?:try_)?advisory_(?:xact_)?lock\s*\(")
        self.assertNotRegex(sql, r"(?im)^\\(?:connect|c\s|i\s|include|gexec|!|o\s)")
        self.assertIn("pg_catalog.pg_roles", sql)
        self.assertIn("pg_catalog.pg_namespace", sql)
        self.assertIn("pg_catalog.pg_settings", sql)
        self.assertNotIn("pg_stat_ssl", sql)
        self.assertNotIn("pg_catalog.version()", sql)

    def test_real_subprocess_boundary_with_local_fake_psql_only(self):
        # A real process tests argv/env/stdin plumbing, but the executable is a
        # generated local recorder. Its PATH cannot contain a real DB client.
        with tempfile.TemporaryDirectory(prefix="fvoci-offline-fake-psql-") as directory:
            root = Path(directory)
            recorded = root / "invocation.json"
            executable = root / "psql"
            executable.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                f"with open({str(recorded)!r}, 'w') as output:\n"
                "    json.dump({'argv': sys.argv[1:], 'env': dict(os.environ), 'sql': sys.stdin.read()}, output)\n"
                f"sys.stdout.write({synthetic_output()!r})\n"
                f"sys.stderr.write({'synthetic ignored stderr ' + URI!r})\n"
            )
            executable.chmod(0o700)
            with (
                mock.patch.object(precheck.os, "defpath", directory),
                mock.patch.object(precheck.os.path, "isfile", return_value=True),
                mock.patch.dict(os.environ, {"PGHOSTADDR": "127.0.0.1", "UNRELATED_SECRET": "FAKE-unrelated-secret"}, clear=True),
            ):
                report, code = precheck.run_precheck(synthetic_env())
            self.assertEqual(code, 0)
            self.assert_private(report)
            invocation = json.loads(recorded.read_text())
            self.assertEqual(invocation["sql"], precheck.SQL)
            self.assertEqual(invocation["env"]["PATH"], directory)
            self.assertEqual(invocation["env"]["PGPASSWORD"], PASSWORD)
            self.assertNotIn("PGHOSTADDR", invocation["env"])
            self.assertNotIn("UNRELATED_SECRET", invocation["env"])
            self.assertNotIn(PASSWORD, " ".join(invocation["argv"]))
            self.assertFalse(Path(invocation["env"]["HOME"]).exists())


class WorkflowBoundaryTests(OfflineTestCase):
    """Regression guards for the credential boundary; root also runs actionlint."""

    def test_manual_dormant_workflow_has_no_external_code_entry(self):
        source = (HERE.parents[1] / ".github/workflows/supabase-cloud-readonly.yml").read_text()
        self.assertRegex(source, r"(?m)^on:\n  workflow_dispatch:")
        for trigger in ("push", "pull_request", "pull_request_target", "workflow_run", "schedule"):
            self.assertNotRegex(source, rf"(?m)^  {trigger}:")
        self.assertRegex(source, r"\$\{\{ false && github.event_name == 'workflow_dispatch'")
        self.assertIn("github.repository == 'fvoci/GroundZero'", source)
        self.assertIn("github.ref == 'refs/heads/main'", source)
        self.assertIn("ref: ${{ github.sha }}", source)
        self.assertNotRegex(source, r"ref:.*inputs\.")
        self.assertIn("persist-credentials: false", source)
        self.assertRegex(source, r"actions/checkout@[a-f0-9]{40}\b")
        self.assertRegex(source, r"permissions:\n  contents: read\n")
        self.assertNotRegex(source, r"(?m)^\s+\w[\w-]*: write\s*$")
        self.assertIn("group: supabase-cloud-readonly", source)
        self.assertIn("cancel-in-progress: false", source)
        self.assertIn("timeout-minutes: 3", source)
        self.assertNotIn("upload-artifact", source)
        self.assertNotIn("cargo xtask test", source)
        self.assertEqual(re.findall(r"secrets\.([A-Z_]+)", source), [SECRET_NAME])
        self.assertIn("vars.SUPABASE_TEST_TARGET_SHA256", source)
        self.assertIn("inputs.confirm_disposable_target", source)
        self.assertIn("type: boolean", source)
        self.assertIn("default: false", source)
        self.assertNotIn("inputs.target_sha256", source)


if __name__ == "__main__":
    unittest.main()
