#!/usr/bin/env bash
# Install Ubuntu CI dependencies with complete indexes and one signed mirror fallback.
set -euo pipefail

install_options=(-y)
if [[ ${1:-} == --no-install-recommends ]]; then
    install_options+=(--no-install-recommends)
    shift
fi
if [[ $# == 0 ]]; then
    echo 'usage: apt_install.sh [--no-install-recommends] package...' >&2
    exit 2
fi
for package in "$@"; do
    if [[ ! $package =~ ^[a-z0-9][a-z0-9+.:=~_-]*$ ]]; then
        echo "Invalid CI package name: $package" >&2
        exit 2
    fi
done
packages=("$@")
architecture=$(dpkg --print-architecture)
case "$architecture" in
    amd64) mirror_host=mirrors.edge.kernel.org/ubuntu ;;
    arm64) mirror_host=mirrors.dotsrc.org/ubuntu-ports ;;
    *) echo "Unsupported Ubuntu CI architecture: $architecture" >&2; exit 2 ;;
esac
# Minimal Ubuntu images bootstrap ca-certificates through APT's signed metadata.
# Existing VMs use HTTPS; archive signatures remain mandatory in either case.
scheme=https
if [[ ! -r /etc/ssl/certs/ca-certificates.crt ]]; then scheme=http; fi
mirror="$scheme://$mirror_host"
source_root=${ELEPHC_CI_APT_ETC:-/etc/apt}
work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/elephc-apt.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
export DEBIAN_FRONTEND=noninteractive LC_ALL=C
started=$SECONDS
last_status=1
last_phase=update
source_options=()
common_options=(
    -o Acquire::Retries=1
    -o Acquire::http::Timeout=15
    -o Acquire::https::Timeout=15
    -o APT::Get::AllowUnauthenticated=false
    -o Acquire::AllowInsecureRepositories=false
    -o Acquire::AllowDowngradeToInsecureRepositories=false
)

run_apt() {
    local phase=$1 remaining limit
    shift
    last_phase=$phase
    remaining=$((300 - SECONDS + started))
    if ((remaining <= 0)); then
        echo '::error::APT provisioning exceeded its five-minute total budget' >&2
        last_status=124
        return 124
    fi
    limit=90
    if [[ $phase == install ]]; then limit=120; fi
    if ((limit > remaining)); then limit=$remaining; fi
    echo "APT phase=$phase attempt=$attempt/2 architecture=$architecture budget_seconds=$remaining"
    set +e
    timeout --kill-after=5 "$limit" apt-get "${common_options[@]}" ${source_options[@]+"${source_options[@]}"} "$@" \
        2>&1 | tee "$work/apt.log"
    local codes=("${PIPESTATUS[@]}")
    set -e
    last_status=${codes[0]}
    if ((codes[1] != 0)); then last_status=1; fi
    return "$last_status"
}

network_failure() {
    # Signature, hash, permission and package errors must never become a retry.
    if grep -Eiq 'NO_PUBKEY|EXPKEYSIG|BADSIG|not signed|signatures? (could not|couldn.t|were invalid)|Hash Sum mismatch|not valid yet|Release file .*expired|Permission denied|Could not open lock' "$work/apt.log"; then
        return 1
    fi
    if [[ $last_phase == update && $last_status == 124 ]]; then return 0; fi
    grep -Eiq 'Temporary failure resolving|Could not resolve|Connection timed out|Connection failed|Could not connect|Network is unreachable|Connection reset|TLS connection was non-properly terminated|(^|[[:space:]])(429|500|502|503|504)[[:space:]]' "$work/apt.log"
}

prepare_fallback() {
    local file destination changed=0
    mkdir -p "$work/sources.list.d"
    : > "$work/sources.list"
    # Preserve vendor repositories, components, suites, architectures and Signed-By.
    # Rewrite only Ubuntu's archive URLs and the hosted runner's Ubuntu mirror lists.
    for file in "$source_root/sources.list" "$source_root"/sources.list.d/*.list "$source_root"/sources.list.d/*.sources; do
        [[ -f $file ]] || continue
        destination="$work/sources.list.d/${file##*/}"
        if [[ $file == "$source_root/sources.list" ]]; then destination="$work/sources.list"; fi
        sed -E \
            -e "s#https?://([[:alnum:]-]+\.)?(archive|security|ports)\.ubuntu\.com/(ubuntu|ubuntu-ports)/?([[:space:]]|$)#$mirror\4#g" \
            -e "s#mirror\+file:/etc/apt/apt-mirrors(-security)?\.txt([[:space:]]|$)#$mirror\2#g" \
            "$file" > "$destination"
        if ! cmp -s "$file" "$destination"; then changed=$((changed + 1)); fi
    done
    if ((changed == 0)); then
        echo '::error::No supported Ubuntu archive source found for mirror recovery' >&2
        return 1
    fi
    source_options=(-o "Dir::Etc::sourcelist=$work/sources.list" -o "Dir::Etc::sourceparts=$work/sources.list.d")
    echo "::warning::APT transport failed; retrying complete indexes and packages through $mirror"
}

for attempt in 1 2; do
    if ((attempt == 2)); then prepare_fallback; fi
    if run_apt update --error-on=any update && \
       run_apt install install "${install_options[@]}" -- "${packages[@]}"; then
        echo "APT provisioning complete on attempt $attempt/2"
        exit 0
    fi
    if ((attempt == 2)) || ! network_failure; then exit "$last_status"; fi
done
