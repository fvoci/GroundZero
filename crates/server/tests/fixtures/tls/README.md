# Runtime private CA fixtures

`private_ca.rs` generates all keys and certificates at test runtime in a unique
temporary directory (mode 0700 on Unix), removed when the test ends. Certificates
are valid for one day. No key or certificate files are tracked.
The OpenSSL CLI is required; a missing or failing CLI makes the test fail.

Command summary (full arguments are in `private_ca.rs`):

- `openssl req -x509 -newkey rsa:2048 -days 1`: test CA and unrelated CA.
- `openssl req -new -newkey rsa:2048`: server key and CSR with loopback SANs.
- `openssl x509 -req -days 1 -copy_extensions copy`: sign the server certificate.
- Concatenate the unrelated CA followed by the test CA into the combined bundle.

Separate client processes check rejection without a CA or with the unrelated CA,
and trust with the test CA or combined bundle via `SSL_CERT_FILE`. All HTTPS
requests stay on loopback; no external service is contacted.
