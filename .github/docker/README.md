# Linux CI environments

`ci.Dockerfile` builds recurring Linux native-test dependencies once. Jobs that
previously ran APT or built PHP SDKs now pull verified execution environments.
macOS jobs retain the native Apple toolchain and their existing PHP setup.

| Role | Used by | Platforms |
| --- | --- | --- |
| `ci` | Compiler, bridge and PDO variant producers; ordinary shards | amd64, arm64 |
| `smoke` | Managed native-package smoke and development packaging | amd64, arm64 |
| `smoke22` | Release and nightly probes retaining the Ubuntu 22.04 glibc baseline | amd64, arm64 |
| `php-nts`, `php-ts` | PHP 8.5 release SDK and debug embed SDK, including Valgrind | amd64, arm64 |
| `reference84`, `reference85` | PHPUnit differential tests and tokenizer reference tests | amd64 |
| `pdo` | Optional-driver tests with preinstalled Oracle and Microsoft clients | amd64 |
| `kdc` | The PostgreSQL GSSAPI Kerberos fixture | amd64 |

The producer image also includes libclang for libpq-sys binding generation; its
content check parses the actual PostgreSQL and standard C headers. Runtime
variants use the common stage without this build-only parser.

The smoke images deliberately omit system PCRE2 development files. Managed
native packages still build from verified source where that behavior is under
test. The PHP SDK images retain separate release and debug hosts, and separate
NTS and ZTS builds. These are existing test distinctions, not interchangeable
software bundles.

`CI Image` builds natively for each architecture, verifies installed tools with
network access disabled, then publishes source-addressed manifest tags. Candidate
branch dispatches never replace `latest`. The committed `scripts/ci/images.json`
catalog activates immutable digests for every role. Producers and shards share
its single `ci` digest; a new image publication cannot change a running CI job.

To refresh the catalog:

1. Push the image change on a candidate branch and dispatch `ci-image.yml` there.
2. Wait for every build, content check and manifest job to succeed.
3. Run `python3 scripts/ci/pin_ci_images.py IMAGE_WORKFLOW_SOURCE_SHA`.
4. Commit the catalog with the consumer change and validate its CI before landing.

The pin command requires a full source SHA and checks every advertised platform.
Build tags belong to the run and architecture, so a failed architecture can be
rerun without losing the tags from successful jobs in its earlier attempt.
A missing or malformed catalog entry fails closed. Historical workflows without
a catalog retain the older resolver, which freezes `latest` to a digest once.

VM consumer jobs run their scripts through `scripts/ci/image_shell.py`. Actions
continue orchestrating downloads and services on the VM. The execution container
uses the VM's localhost network, UID/GID, read-only account databases, and the
same absolute workspace and temporary paths. PDO fixtures can use the existing
Docker daemon and published database ports without changing DSNs or TLS names.
The consumer Cargo build guard stays first in PATH, and its audit log is shared.
A consumer execution image is separate from the producer image identity recorded
in artifact manifests; source, checksum and ABI validation remain mandatory.

Historical packaging retains the trusted wrapper before switching its checkout.
Release probes keep their native cache in an explicit shared runner-temp path.
