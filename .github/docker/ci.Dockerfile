# Prebuilt environment for the Linux CI jobs in .github/workflows/ci.yml.
#
# Every Linux job (build archives, non-codegen tests, 2x16 codegen/eval shards
# per arch) used to run the same apt-get install; on ubuntu-24.04-arm runners
# that step alone cost ~150s per job (~80 minutes of runner time per CI run).
# Baking the dependencies, a pinned Rust toolchain, and cargo-nextest into one
# multi-arch image removes that cost and keeps the toolchain deterministic.
#
# Published as ghcr.io/illegalstudio/elephc-ci:latest (amd64 + arm64) by
# .github/workflows/ci-image.yml whenever this file or that workflow changes
# on main.
#
# Bump RUST_VERSION deliberately, in its own PR: the CI "no warnings" gate
# greps cargo build output, so a floating `stable` toolchain would let a new
# Rust release break every open PR unannounced. Keep the version aligned with
# the local test images (Dockerfile.test-linux-x86_64 / -arm64).
FROM ubuntu:24.04

ARG RUST_VERSION=1.95.0
ARG NEXTEST_VERSION=0.9.140

# The apt list mirrors what the CI jobs previously installed per job, plus:
# ca-certificates/curl (rustup and nextest downloads below), git (without it
# actions/checkout inside a container falls back to a tarball download), zstd
# (lets actions/cache use zstd instead of the slower gzip fallback), and
# netbase (/etc/protocols + /etc/services — present on the runner VMs but not
# in the ubuntu base image; the compiled runtime and the magician interpreter
# read them for getprotobyname()/getservbyname() and friends).
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        binutils \
        build-essential \
        ca-certificates \
        curl \
        file \
        freetds-dev \
        git \
        libbz2-dev \
        libpcre2-dev \
        libssl-dev \
        netbase \
        pkg-config \
        tzdata \
        unixodbc-dev \
        zlib1g-dev \
        zstd \
    && rm -rf /var/lib/apt/lists/*

# Install the toolchain outside $HOME: GitHub job containers run steps with
# HOME=/github/home (a runner-mounted volume), so anything under /root would
# be unreachable from the job steps.
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" \
    && rustc --version \
    && cargo --version

RUN case "$(dpkg --print-architecture)" in \
        amd64) nextest_platform=linux ;; \
        arm64) nextest_platform=linux-arm ;; \
        *) echo "unsupported architecture: $(dpkg --print-architecture)" >&2; exit 1 ;; \
    esac \
    && curl -LsSf "https://get.nexte.st/${NEXTEST_VERSION}/${nextest_platform}" \
        | tar -xzf - -C /usr/local/cargo/bin \
    && cargo nextest --version
