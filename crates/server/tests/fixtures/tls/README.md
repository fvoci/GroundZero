# Private CA test fixtures

These certificates and the unencrypted server key are public test data only.
Never use them outside tests. The CA private key is discarded after generation.
`private_ca.rs` uses the existing OpenSSL CLI and the default reqwest client;
each trust configuration runs in a separate process.

Generated with the following commands from the repository root. The certificates
last ten years; regenerate them together before they expire. The `tls-test` DNS
name also permits the same client probe on an isolated container network.

```bash
mkdir -p target/tls-test-fixture
openssl req -x509 -newkey rsa:2048 -noenc -sha256 -days 3650 \
  -subj '/CN=fvoci private CA test only' \
  -addext 'basicConstraints=critical,CA:TRUE' \
  -addext 'keyUsage=critical,keyCertSign,cRLSign' \
  -keyout target/tls-test-fixture/ca.key \
  -out crates/server/tests/fixtures/tls/ca.pem
openssl req -new -newkey rsa:2048 -noenc -sha256 \
  -subj '/CN=fvoci TLS server test only' \
  -addext 'subjectAltName=DNS:localhost,DNS:tls-test,IP:127.0.0.1' \
  -addext 'basicConstraints=critical,CA:FALSE' \
  -addext 'keyUsage=critical,digitalSignature,keyEncipherment' \
  -addext 'extendedKeyUsage=serverAuth' \
  -keyout crates/server/tests/fixtures/tls/server.key \
  -out target/tls-test-fixture/server.csr
openssl x509 -req -sha256 -days 3650 -copy_extensions copy \
  -in target/tls-test-fixture/server.csr \
  -CA crates/server/tests/fixtures/tls/ca.pem \
  -CAkey target/tls-test-fixture/ca.key -CAcreateserial \
  -CAserial target/tls-test-fixture/ca.srl \
  -out crates/server/tests/fixtures/tls/server.pem
rm target/tls-test-fixture/ca.key
```
