# Scanner image for Anthropic's OSS Scanner (https://github.com/anthropics/oss-scanner, projects/elephc).
#
# The scan runs offline, so this build fetches every crate in Cargo.lock, then builds in the dev profile (debug info,
# debug assertions and overflow checks on): the compiler, every bridge staticlib that compiled programs link, and every
# test binary. It also builds the managed native packages (libxml2, curl with libssh2/nghttp2/openssl/zlib, PCRE2,
# Oniguruma) from their pinned, checksum-verified sources, so compiled programs that need them link without the
# network. threat_model.md, next to this file, says how to drive all of it.
#
# Keep the base image, apt list, Rust release and nextest version aligned with .github/docker/ci.Dockerfile, which
# builds the Linux CI environment.
FROM ubuntu:24.04

ARG RUST_VERSION=1.98
ARG NEXTEST_VERSION=0.9.140

# The CI apt list, plus gdb. The compiler shells out to the host `as` and `ld`/`gcc` (binutils, build-essential); the
# PDO bridge links FreeTDS and unixODBC; the codegen tests compile Elephc's PCRE2 shim against the system headers. gdb
# is how threat_model.md suggests debugging compiled programs; the scanner's own layer installs it too, but the image
# should not depend on that.
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        binutils \
        build-essential \
        ca-certificates \
        curl \
        file \
        freetds-dev \
        gdb \
        git \
        libbz2-dev \
        libpcre2-dev \
        libssl-dev \
        netbase \
        pkg-config \
        python3 \
        tzdata \
        unixodbc-dev \
        zlib1g-dev \
        zstd \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" \
    && rustc --version

RUN case "$(dpkg --print-architecture)" in \
        amd64) nextest_platform=linux ;; \
        arm64) nextest_platform=linux-arm ;; \
        *) echo "unsupported architecture: $(dpkg --print-architecture)" >&2; exit 1 ;; \
    esac \
    && curl -LsSf "https://get.nexte.st/${NEXTEST_VERSION}/${nextest_platform}" | tar -xzf - -C "${CARGO_HOME}/bin" \
    && cargo nextest --version

# scanner contract: the checkout lives inside the image, at /src
COPY . /src
WORKDIR /src

# RUST_MIN_STACK matches CI: deep parser and lowering fixtures overflow the default test thread stack.
# CARGO_INCREMENTAL=0 keeps about 6 GB of incremental state out of the image and shortens the build, which the scanner
# limits to 45 minutes. ELEPHC_NATIVE_CACHE pins the managed native package cache to one path, whatever HOME the
# scanner uses later.
ENV RUST_BACKTRACE=1 \
    RUST_MIN_STACK=33554432 \
    CARGO_TERM_COLOR=never \
    CARGO_INCREMENTAL=0 \
    RUSTFLAGS="-C force-frame-pointers=yes" \
    ELEPHC_NATIVE_CACHE=/opt/elephc-native

# Every crate in Cargo.lock, so later builds (including the bridge rebuilds the compiler starts by itself when a
# bridge's sources change) stay offline.
RUN cargo fetch --locked

# The compiler, then everything else at once:
# - in the background, the managed native packages that the examples' committed locks pin, plus the offline Oniguruma
#   bundle the codegen tests use. One install at a time: installs of a shared package wait on its cache lock, and only
#   for 30 seconds.
# - in the foreground, the bridge staticlibs, as CI's "Build & Archive Tests (linux-x86_64)" job builds them, and every
#   test binary, without running it. The magician (eval) staticlib with curl support goes to its own file, because
#   Cargo does not vary a staticlib's name by feature.
# A step that fails fails the build, and primary_contact gets the log.
RUN set -e; \
    cargo build --locked; \
    ( set -e; \
      for lock in examples/*/elephc.lock; do \
          ./target/debug/elephc native install --locked --target linux-x86_64 \
              --manifest-path "$(dirname "$lock")/elephc.toml"; \
      done; \
      python3 scripts/ci/prepare_oniguruma_tests.py target/debug/elephc target/debug/elephc-oniguruma \
          --target linux-x86_64 \
    ) > /tmp/native-install.log 2>&1 & native=$!; \
    cargo build --locked \
        -p elephc-tls -p elephc-pdo -p elephc-crypto -p elephc-bcmath -p elephc-mbstring -p elephc-iconv \
        -p elephc-phar -p elephc-tz -p elephc-image -p elephc-pcntl -p elephc-xml -p elephc-magician \
        -p elephc-web -p elephc-instr -p elephc-probe -p elephc-curl; \
    cargo build --locked -p elephc-magician --features curl --target-dir target/elephc-magician-curl-test-build; \
    cp target/elephc-magician-curl-test-build/debug/libelephc_magician.a target/debug/libelephc_magician_curl.a; \
    cargo nextest run --locked --workspace --no-run; \
    status=0; wait "$native" || status=$?; \
    cat /tmp/native-install.log; rm /tmp/native-install.log; \
    exit "$status"

# Compile and run a few programs so the log shows the image works, without letting a failure abort the build.
RUN (./target/debug/elephc examples/fizzbuzz/main.php && ./examples/fizzbuzz/main | tail -n 3 \
        && ./target/debug/elephc examples/hello-preg/main.php && ./examples/hello-preg/main \
        && ./target/debug/elephc examples/xml/main.php && ./examples/xml/main | tail -n 3) \
    || echo "WARNING: an example failed to compile or run"

# The cheap tests: the lexer, parser and error-reporting suites and the bridge crates' unit tests. The compiler's
# own unit tests (thousands, each lowering for every target) and the end-to-end codegen suites (codegen_tests and the
# other compile-and-run binaries) take hours on one machine, so they are left to the agent.
RUN timeout 1800 cargo nextest run --locked --workspace --no-fail-fast \
        -E '(kind(lib) & not package(elephc)) | (binary_id(elephc::integration) & test(/^(lexer_tests|parser_tests|error_tests)::/))' \
    || echo "WARNING: cargo nextest reported failures"

# Everything is fetched. From here on cargo must not try to reach a registry.
ENV CARGO_NET_OFFLINE=true
