#!/usr/bin/env bash

set -Eeuo pipefail

export LC_ALL=C
export TZ=UTC
umask 022

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
DEFAULT_INVENTORY="$REPO_ROOT/docs/specs/wasm-inventory.json"
DEFAULT_SPECIFICATION="$REPO_ROOT/docs/specs/wasm-compliance.md"
PHP_SRC_REPOSITORY="https://github.com/php/php-src.git"
EXPECTED_PROFILES=("8.2" "8.3" "8.4" "8.5")
OPTIONAL_BUILD_ENV_VARS=("CC" "CFLAGS" "CPPFLAGS" "LDFLAGS" "PKG_CONFIG_PATH")
EXPLICITLY_CLEARED_ENV_VARS=(
    "BASH_ENV"
    "CONFIG_SITE"
    "ENV"
    "GIT_ASKPASS"
    "GIT_CONFIG_COUNT"
    "GIT_SSH"
    "GIT_SSH_COMMAND"
    "HTTP_PROXY"
    "HTTPS_PROXY"
    "LD_LIBRARY_PATH"
    "LIBRARY_PATH"
    "MAKEFLAGS"
    "MAKELEVEL"
    "MFLAGS"
    "NO_PROXY"
    "PHP_INI_SCAN_DIR"
    "all_proxy"
    "http_proxy"
    "https_proxy"
    "no_proxy"
)
CONFIGURE_ARGS=(
    "--prefix=/install"
    "--disable-all"
    "--enable-cli"
    "--disable-cgi"
    "--disable-phpdbg"
    "--without-pear"
)

STAGING_DIR=""
BUILD_PATH=""
BUILD_ENV_READY=0
BUILD_ENV_ASSIGNMENTS=()

usage() {
    cat <<'EOF'
Build php-src revisions pinned by docs/specs/wasm-inventory.json.

Usage:
  scripts/build-pinned-php-src.sh --output-dir PATH [--profile PROFILE] [--jobs N]
  scripts/build-pinned-php-src.sh --verify-pins-only [--profile PROFILE] [--inventory PATH]

Options:
  --output-dir PATH   New directory that will receive the selected build(s).
  --inventory PATH    Inventory to validate and read (defaults to the repository inventory).
  --profile PROFILE   One of 8.2, 8.3, 8.4, 8.5, or all (default: all).
  --jobs N            Positive make parallelism (defaults to the detected CPU count).
  --verify-pins-only  Print profile/tag/tag-object/tag-commit TSV; do not fetch or build.
  -h, --help          Show this help.

The output directory must not already exist. Builds are staged in a neighboring
temporary directory and published with a no-clobber rename only after every
provenance and hash check passes.
EOF
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

cleanup() {
    if [[ -z "$STAGING_DIR" || ! -d "$STAGING_DIR" ]]; then
        return
    fi
    case "$(basename -- "$STAGING_DIR")" in
        .php-src-build.*) rm -rf -- "$STAGING_DIR" ;;
        *) printf 'error: refusing to clean unexpected staging path: %s\n' "$STAGING_DIR" >&2 ;;
    esac
}

detected_jobs() {
    local jobs=""
    if command -v getconf >/dev/null 2>&1; then
        jobs="$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)"
    fi
    if [[ ! "$jobs" =~ ^[1-9][0-9]*$ ]] && command -v sysctl >/dev/null 2>&1; then
        jobs="$(sysctl -n hw.ncpu 2>/dev/null || true)"
    fi
    if [[ ! "$jobs" =~ ^[1-9][0-9]*$ ]]; then
        jobs=1
    fi
    printf '%s\n' "$jobs"
}

prepare_build_environment() {
    local candidate=""
    local directory=""
    local variable=""

    BUILD_PATH="${PATH:-/usr/bin:/bin:/usr/sbin:/sbin}"
    for candidate in \
        /opt/homebrew/opt/bison/bin/bison \
        /usr/local/opt/bison/bin/bison \
        /opt/local/bin/bison; do
        if [[ -x "$candidate" ]]; then
            directory="$(dirname -- "$candidate")"
            case ":$BUILD_PATH:" in
                *":$directory:"*) ;;
                *) BUILD_PATH="$directory:$BUILD_PATH" ;;
            esac
        fi
    done
    [[ -n "$BUILD_PATH" ]] || die "the sanitized build PATH must not be empty"
    BUILD_ENV_ASSIGNMENTS=(
        "PATH=$BUILD_PATH"
        "LC_ALL=C"
        "TZ=UTC"
        "GIT_CONFIG_NOSYSTEM=1"
        "GIT_CONFIG_GLOBAL=/dev/null"
        "GIT_TERMINAL_PROMPT=0"
    )
    for variable in "${OPTIONAL_BUILD_ENV_VARS[@]}"; do
        if [[ -v "$variable" ]]; then
            BUILD_ENV_ASSIGNMENTS+=("$variable=${!variable}")
        fi
    done
    BUILD_ENV_READY=1
}

verify_build_tool_versions() {
    local bison_version=""

    bison_version="$(first_version_line run_build_env bison --version)"
    if [[ ! "$bison_version" =~ ([0-9]+)\.([0-9]+) ]]; then
        die "cannot parse bison version: $bison_version"
    fi
    ((10#${BASH_REMATCH[1]} >= 3)) \
        || die "bison 3.0 or later is required, found: $bison_version"
}

run_build_env() {
    if ((BUILD_ENV_READY == 0)); then
        prepare_build_environment
    fi
    env -i "${BUILD_ENV_ASSIGNMENTS[@]}" "$@"
}

run_reproducible_build_env() {
    local source_date_epoch="$1"
    shift
    [[ "$source_date_epoch" =~ ^[0-9]+$ ]] \
        || die "invalid SOURCE_DATE_EPOCH: $source_date_epoch"
    if ((BUILD_ENV_READY == 0)); then
        prepare_build_environment
    fi
    env -i "${BUILD_ENV_ASSIGNMENTS[@]}" \
        "SOURCE_DATE_EPOCH=$source_date_epoch" "$@"
}

run_metadata_env() {
    local source_date_epoch="$1"
    shift
    [[ "$source_date_epoch" =~ ^[0-9]+$ ]] \
        || die "invalid metadata SOURCE_DATE_EPOCH: $source_date_epoch"
    env -i \
        "LC_ALL=C" \
        "TZ=UTC" \
        "SOURCE_DATE_EPOCH=$source_date_epoch" \
        "$@"
}

write_build_environment_manifest() {
    local destination="$1"
    local arguments=()
    local observed_environment=""
    local resolved_tools=()
    local tool=""
    local variable=""

    if ((BUILD_ENV_READY == 0)); then
        prepare_build_environment
    fi
    for variable in "${OPTIONAL_BUILD_ENV_VARS[@]}"; do
        if [[ -v "$variable" ]]; then
            arguments+=("$variable" "set" "${!variable}")
        else
            arguments+=("$variable" "unset" "")
        fi
    done
    for tool in git tar autoconf bison re2c make cc; do
        resolved_tools+=(
            "$tool"
            "$(PATH="$BUILD_PATH" command -v "$tool" 2>/dev/null \
                || die "cannot resolve sanitized build tool: $tool")"
        )
    done
    observed_environment="$(
        run_build_env python3 -c \
            'import json, os; print(json.dumps(dict(os.environ), sort_keys=True))'
    )"

    python3 - "$destination" "$BUILD_PATH" \
        "${EXPLICITLY_CLEARED_ENV_VARS[*]}" \
        "$observed_environment" \
        "${#arguments[@]}" "${arguments[@]}" "${resolved_tools[@]}" <<'PY'
import json
import sys
from pathlib import Path

destination = Path(sys.argv[1])
build_path = sys.argv[2]
cleared = sys.argv[3].split()
observed = json.loads(sys.argv[4])
optional_count = int(sys.argv[5])
optional = sys.argv[6:6 + optional_count]
resolved = sys.argv[6 + optional_count:]
if len(optional) % 3:
    raise SystemExit("malformed optional build environment records")
if len(resolved) % 2:
    raise SystemExit("malformed resolved build tool records")

overrides = {}
for index in range(0, len(optional), 3):
    name, state, value = optional[index:index + 3]
    if state not in {"set", "unset"}:
        raise SystemExit(f"invalid state for {name}: {state}")
    overrides[name] = {
        "set": state == "set",
        "value": value if state == "set" else None,
    }
resolved_tools = {
    resolved[index]: resolved[index + 1]
    for index in range(0, len(resolved), 2)
}
expected_observed = {
    "PATH": build_path,
    "LC_ALL": "C",
    "TZ": "UTC",
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_GLOBAL": "/dev/null",
    "GIT_TERMINAL_PROMPT": "0",
}
expected_observed.update(
    {
        name: details["value"]
        for name, details in overrides.items()
        if details["set"]
    }
)
platform_injected = {"__CF_USER_TEXT_ENCODING"}
if not isinstance(observed, dict):
    raise SystemExit("observed build environment is not an object")
for name, value in expected_observed.items():
    if observed.get(name) != value:
        raise SystemExit(f"observed build environment has unexpected {name}")
unexpected = set(observed) - set(expected_observed) - platform_injected
if unexpected:
    raise SystemExit(
        "observed build environment exceeds the allowlist: "
        + ", ".join(sorted(unexpected))
    )

document = {
    "schema": "elephc.pinned-php-src-build-environment.v1",
    "passed": {
        "PATH": build_path,
        "LC_ALL": "C",
        "TZ": "UTC",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_TERMINAL_PROMPT": "0",
    },
    "toolchain_overrides": overrides,
    "resolved_tools": resolved_tools,
    "observed": observed,
    "platform_injected_allowed": sorted(platform_injected & set(observed)),
    "explicitly_cleared": sorted(cleared),
    "source_date_epoch": "recorded per profile",
}
destination.write_text(
    json.dumps(document, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

sha256_file() {
    local path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -- "$path" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 -- "$path" | awk '{print $1}'
    else
        die "sha256sum or shasum is required"
    fi
}

provenance_path() {
    local path="$1"
    local directory=""
    local basename=""
    local absolute=""

    directory="$(dirname -- "$path")"
    basename="$(basename -- "$path")"
    absolute="$(cd -- "$directory" && pwd -P)/$basename"
    case "$absolute" in
        "$REPO_ROOT"/*) printf '%s\n' "${absolute#"$REPO_ROOT"/}" ;;
        *) printf '%s\n' "$absolute" ;;
    esac
}

first_version_line() {
    "$@" 2>/dev/null | sed -n '1p'
}

extract_pins() {
    local inventory="$1"
    [[ -f "$inventory" ]] || die "inventory does not exist: $inventory"
    python3 - "$inventory" <<'PY'
import json
import re
import sys
from pathlib import Path

inventory_path = Path(sys.argv[1])
expected_profiles = ["8.2", "8.3", "8.4", "8.5"]


def reject_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key!r}")
        result[key] = value
    return result


try:
    document = json.loads(
        inventory_path.read_text(encoding="utf-8"),
        object_pairs_hook=reject_duplicate_keys,
    )
    schema = document["metadata"]["schema"]
    if schema != "elephc.wasm-inventory.v4":
        raise ValueError(
            "metadata.schema must be 'elephc.wasm-inventory.v4' for explicit "
            "tag_object/tag_commit pins"
        )
    pins = document["metadata"]["pins"]["php_src"]
    if not isinstance(pins, list):
        raise ValueError("metadata.pins.php_src must be an array")

    by_profile = {}
    for index, pin in enumerate(pins):
        if not isinstance(pin, dict):
            raise ValueError(f"metadata.pins.php_src[{index}] must be an object")
        profile = pin.get("profile")
        tag = pin.get("tag")
        tag_object = pin.get("tag_object")
        tag_commit = pin.get("tag_commit")
        if profile in by_profile:
            raise ValueError(f"duplicate php-src profile: {profile!r}")
        if profile not in expected_profiles:
            raise ValueError(f"unexpected php-src profile: {profile!r}")
        if not isinstance(tag, str) or not re.fullmatch(
            rf"php-{re.escape(profile)}\.[0-9]+", tag
        ):
            raise ValueError(f"profile {profile}: invalid exact php-src tag {tag!r}")
        if not isinstance(tag_object, str) or not re.fullmatch(
            r"[0-9a-f]{40}", tag_object
        ):
            raise ValueError(
                f"profile {profile}: tag_object must be 40 lowercase hex characters"
            )
        if not isinstance(tag_commit, str) or not re.fullmatch(
            r"[0-9a-f]{40}", tag_commit
        ):
            raise ValueError(
                f"profile {profile}: tag_commit must be 40 lowercase hex characters"
            )
        if tag_object == tag_commit:
            raise ValueError(
                f"profile {profile}: expected an annotated tag object distinct from tag_commit"
            )
        by_profile[profile] = (tag, tag_object, tag_commit)

    actual_profiles = sorted(by_profile)
    if actual_profiles != expected_profiles:
        raise ValueError(
            "expected exactly php-src profiles "
            + ", ".join(expected_profiles)
            + "; got "
            + (", ".join(actual_profiles) if actual_profiles else "none")
        )

    tag_objects = [tag_object for _, tag_object, _ in by_profile.values()]
    if len(tag_objects) != len(set(tag_objects)):
        raise ValueError("php-src tag objects must be unique across profiles")
    tag_commits = [tag_commit for _, _, tag_commit in by_profile.values()]
    if len(tag_commits) != len(set(tag_commits)):
        raise ValueError("php-src tag commits must be unique across profiles")

    for profile in expected_profiles:
        tag, tag_object, tag_commit = by_profile[profile]
        print(f"{profile}\t{tag}\t{tag_object}\t{tag_commit}")
except (OSError, UnicodeError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
    print(f"error: invalid php-src pin inventory: {error}", file=sys.stderr)
    raise SystemExit(2)
PY
}

extract_specification_pin() {
    local inventory="$1"
    python3 - "$inventory" <<'PY'
import json
import re
import sys
from pathlib import Path


def reject_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key!r}")
        result[key] = value
    return result


try:
    document = json.loads(
        Path(sys.argv[1]).read_text(encoding="utf-8"),
        object_pairs_hook=reject_duplicate_keys,
    )
    digest = document["metadata"]["pins"]["wasm_compliance_sha256"]
    if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise ValueError(
            "metadata.pins.wasm_compliance_sha256 must be 64 lowercase hex characters"
        )
    print(digest)
except (OSError, UnicodeError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
    print(f"error: invalid WASM specification pin: {error}", file=sys.stderr)
    raise SystemExit(2)
PY
}

filter_pins() {
    local pins="$1"
    local selection="$2"
    local profile=""
    local tag=""
    local tag_object=""
    local tag_commit=""
    local matched=0

    case "$selection" in
        all|8.2|8.3|8.4|8.5) ;;
        *) die "--profile must be one of 8.2, 8.3, 8.4, 8.5, or all" ;;
    esac

    while IFS=$'\t' read -r profile tag tag_object tag_commit; do
        if [[ "$selection" == "all" || "$selection" == "$profile" ]]; then
            printf '%s\t%s\t%s\t%s\n' \
                "$profile" "$tag" "$tag_object" "$tag_commit"
            matched=$((matched + 1))
        fi
    done <<<"$pins"
    if [[ "$selection" == "all" ]]; then
        [[ "$matched" -eq "${#EXPECTED_PROFILES[@]}" ]] \
            || die "selected $matched profiles, expected ${#EXPECTED_PROFILES[@]}"
    else
        [[ "$matched" -eq 1 ]] \
            || die "selected $matched records for profile $selection, expected one"
    fi
}

verify_specification_pin() {
    local inventory="$1"
    local specification="$2"
    local expected=""
    local actual=""

    [[ -f "$specification" ]] || die "WASM specification does not exist: $specification"
    expected="$(extract_specification_pin "$inventory")"
    actual="$(sha256_file "$specification")"
    [[ "$actual" == "$expected" ]] \
        || die "WASM specification SHA-256 is $actual, expected inventory pin $expected"
    printf '%s\n' "$actual"
}

capture_elephc_state() {
    local destination="$1"
    local repository_root=""
    local head=""
    local status_sha=""
    local dirty="false"

    repository_root="$(run_build_env git -C "$REPO_ROOT" rev-parse --show-toplevel 2>/dev/null)" \
        || die "cannot resolve Elephc repository root"
    [[ "$repository_root" == "$REPO_ROOT" ]] \
        || die "script repository root mismatch: $repository_root"
    head="$(run_build_env git -C "$REPO_ROOT" rev-parse --verify HEAD 2>/dev/null)" \
        || die "cannot resolve Elephc HEAD"
    [[ "$head" =~ ^[0-9a-f]{40}$ ]] || die "malformed Elephc HEAD: $head"
    run_build_env git -C "$REPO_ROOT" \
        status --porcelain=v1 --untracked-files=all >"$destination" \
        || die "cannot capture Elephc dirty state"
    if [[ -s "$destination" ]]; then
        dirty="true"
    fi
    status_sha="$(sha256_file "$destination")"
    printf '%s\t%s\t%s\n' "$head" "$dirty" "$status_sha"
}

verify_checkout() {
    local checkout="$1"
    local tag="$2"
    local expected_tag_object="$3"
    local expected_tag_commit="$4"
    local tag_object=""
    local tag_commit=""
    local head=""
    local status=""

    [[ -d "$checkout/.git" ]] || die "not a standalone Git checkout: $checkout"
    tag_object="$(run_build_env git -C "$checkout" rev-parse --verify "refs/tags/$tag" 2>/dev/null)" \
        || die "cannot resolve tag object for $tag"
    tag_commit="$(run_build_env git -C "$checkout" rev-parse --verify "${tag}^{commit}" 2>/dev/null)" \
        || die "cannot resolve ${tag}^{commit}"
    head="$(run_build_env git -C "$checkout" rev-parse --verify HEAD 2>/dev/null)" \
        || die "cannot resolve checkout HEAD"
    [[ "$tag_object" == "$expected_tag_object" ]] \
        || die "tag $tag object is $tag_object, expected inventory object $expected_tag_object"
    [[ "$tag_commit" == "$expected_tag_commit" ]] \
        || die "tag $tag peels to $tag_commit, expected inventory commit $expected_tag_commit"
    [[ "$head" == "$expected_tag_commit" ]] \
        || die "checkout HEAD is $head, expected peeled tag commit $expected_tag_commit"
    if run_build_env git -C "$checkout" symbolic-ref -q HEAD >/dev/null 2>&1; then
        die "checkout HEAD is attached; a detached checkout is required"
    fi
    status="$(run_build_env git -C "$checkout" status --porcelain=v1 --untracked-files=all)"
    [[ -z "$status" ]] || die "checkout is not clean: $checkout"

    printf '%s\t%s\t%s\n' "$tag_object" "$tag_commit" "$head"
}

write_hash_line() {
    local base="$1"
    local relative="$2"
    local manifest="$3"
    [[ "$relative" != /* && "$relative" != *".."* ]] \
        || die "unsafe relative hash path: $relative"
    [[ -f "$base/$relative" ]] || die "cannot hash missing file: $base/$relative"
    printf '%s  %s\n' "$(sha256_file "$base/$relative")" "$relative" >>"$manifest"
}

verify_hash_manifest() {
    local base="$1"
    local manifest="$2"
    local expected=""
    local relative=""
    local actual=""

    [[ -s "$manifest" ]] || die "hash manifest is empty: $manifest"
    while IFS=' ' read -r expected relative; do
        relative="${relative# }"
        [[ "$expected" =~ ^[0-9a-f]{64}$ ]] || die "invalid SHA-256 in $manifest"
        [[ -n "$relative" && "$relative" != /* && "$relative" != *".."* ]] \
            || die "unsafe path in $manifest: $relative"
        [[ -f "$base/$relative" ]] || die "hashed file is missing: $base/$relative"
        actual="$(sha256_file "$base/$relative")"
        [[ "$actual" == "$expected" ]] \
            || die "SHA-256 mismatch for $base/$relative"
    done <"$manifest"
}

install_tree_manifest() {
    local action="$1"
    local install_root="$2"
    local manifest="$3"

    [[ "$action" == "write" || "$action" == "verify" ]] \
        || die "install_tree_manifest action must be write or verify"
    [[ -d "$install_root" && ! -L "$install_root" ]] \
        || die "install tree is not a real directory: $install_root"

    python3 - "$action" "$install_root" "$manifest" <<'PY'
import hashlib
import json
import os
import stat
import sys
from pathlib import Path

action = sys.argv[1]
root = Path(sys.argv[2])
manifest = Path(sys.argv[3])


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def entry(path: Path) -> dict:
    relative = path.relative_to(root).as_posix()
    details = path.lstat()
    mode = f"{stat.S_IMODE(details.st_mode):04o}"
    if stat.S_ISDIR(details.st_mode):
        return {"path": relative, "type": "directory", "mode": mode}
    if stat.S_ISREG(details.st_mode):
        return {
            "path": relative,
            "type": "file",
            "mode": mode,
            "size": details.st_size,
            "sha256": sha256_file(path),
        }
    if stat.S_ISLNK(details.st_mode):
        return {
            "path": relative,
            "type": "symlink",
            "mode": mode,
            "target": os.readlink(path),
        }
    raise SystemExit(f"unsupported special file in install tree: {relative}")


paths = sorted(
    root.rglob("*"),
    key=lambda item: item.relative_to(root).as_posix(),
)
entries = [entry(root), *(entry(path) for path in paths)]
document = {
    "schema": "elephc.pinned-php-src-install-tree.v1",
    "root": "install",
    "entries": entries,
}
serialized = json.dumps(document, indent=2, sort_keys=True) + "\n"

if action == "write":
    manifest.write_text(serialized, encoding="utf-8")
elif action == "verify":
    try:
        existing = manifest.read_text(encoding="utf-8")
    except OSError as error:
        raise SystemExit(f"cannot read install tree manifest: {error}")
    if existing != serialized:
        raise SystemExit("install tree manifest does not match the installed tree")
else:
    raise SystemExit(f"unsupported manifest action: {action}")
PY
}

write_dynamic_dependencies() {
    local php_binary="$1"
    local destination="$2"
    local raw="$destination.raw"
    local platform=""
    local tool=""

    platform="$(uname -s)"
    case "$platform" in
        Darwin)
            require_command otool
            tool="otool -L"
            run_build_env otool -L "$php_binary" >"$raw"
            ;;
        Linux)
            require_command ldd
            tool="ldd"
            run_build_env ldd "$php_binary" >"$raw"
            ;;
        *)
            die "dynamic dependency capture is unsupported on $platform"
            ;;
    esac

    python3 - "$raw" "$destination" "$php_binary" "$platform" "$tool" <<'PY'
import re
import sys
from pathlib import Path

raw_path = Path(sys.argv[1])
destination = Path(sys.argv[2])
binary = sys.argv[3]
platform = sys.argv[4]
tool = sys.argv[5]
raw = raw_path.read_text(encoding="utf-8", errors="strict")
if platform == "Linux" and re.search(r"=>\s+not found(?:\s|$)", raw):
    raise SystemExit("one or more PHP dynamic dependencies are unresolved")
normalized = raw.replace(binary, "install/bin/php")
if platform == "Linux":
    normalized = re.sub(r"\(0x[0-9A-Fa-f]+\)", "(address omitted)", normalized)
if not normalized.strip():
    raise SystemExit("dynamic dependency output is empty")
destination.write_text(
    f"platform: {platform}\n"
    f"tool: {tool}\n"
    "binary: install/bin/php\n"
    "output:\n"
    f"{normalized.rstrip()}\n",
    encoding="utf-8",
)
raw_path.unlink()
PY
    [[ -s "$destination" ]] || die "dynamic dependency manifest is empty"
}

write_php_metadata() {
    local php_binary="$1"
    local source_date_epoch="$2"
    local expected_version="$3"
    local destination="$4"
    local probe='
$environment = getenv();
ksort($environment);
$extensions = get_loaded_extensions(false);
$zendExtensions = get_loaded_extensions(true);
sort($extensions, SORT_STRING);
sort($zendExtensions, SORT_STRING);
$document = [
    "schema" => "elephc.pinned-php-src-runtime-metadata.v1",
    "environment" => $environment,
    "php" => [
        "version" => PHP_VERSION,
        "sapi" => PHP_SAPI,
        "ini" => [
            "loaded_file" => php_ini_loaded_file() ?: null,
            "scanned_files" => php_ini_scanned_files() ?: null,
            "config_file_path" => get_cfg_var("cfg_file_path"),
            "config_file_scan_dir" => get_cfg_var("cfg_file_scan_dir"),
        ],
        "extensions" => $extensions,
        "zend_extensions" => $zendExtensions,
    ],
];
$encoded = json_encode($document, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);
if ($encoded === false) {
    fwrite(STDERR, "cannot encode runtime metadata\n");
    exit(70);
}
fwrite(STDOUT, $encoded . "\n");
'

    run_metadata_env "$source_date_epoch" "$php_binary" -n -r "$probe" >"$destination"
    [[ -s "$destination" ]] || die "PHP runtime metadata is empty"

    python3 - "$destination" "$expected_version" "$source_date_epoch" \
        "${CONFIGURE_ARGS[@]}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
expected_version = sys.argv[2]
source_date_epoch = sys.argv[3]
configure_args = sys.argv[4:]
document = json.loads(path.read_text(encoding="utf-8"))

if document.get("schema") != "elephc.pinned-php-src-runtime-metadata.v1":
    raise SystemExit("unexpected PHP runtime metadata schema")
environment = document.get("environment")
expected_environment = {
    "LC_ALL": "C",
    "SOURCE_DATE_EPOCH": source_date_epoch,
    "TZ": "UTC",
}
if not isinstance(environment, dict):
    raise SystemExit("PHP metadata environment is not an object")
for name, value in expected_environment.items():
    if environment.get(name) != value:
        raise SystemExit(
            f"PHP metadata environment has unexpected {name}: {environment.get(name)!r}"
        )
platform_injected = {"__CF_USER_TEXT_ENCODING"}
unexpected_environment = set(environment) - set(expected_environment) - platform_injected
if unexpected_environment:
    raise SystemExit(
        "PHP metadata environment exceeds the allowlist: "
        + ", ".join(sorted(unexpected_environment))
    )
document["environment_policy"] = {
    "passed": expected_environment,
    "platform_injected_allowed": sorted(platform_injected & set(environment)),
}
php = document.get("php", {})
if php.get("version") != expected_version:
    raise SystemExit(
        f"built PHP version is {php.get('version')!r}, expected {expected_version!r}"
    )
if php.get("sapi") != "cli":
    raise SystemExit(f"built PHP SAPI is {php.get('sapi')!r}, expected 'cli'")
ini = php.get("ini", {})
if ini.get("loaded_file") is not None or ini.get("scanned_files") is not None:
    raise SystemExit("PHP -n unexpectedly loaded or scanned an ini file")
extensions = php.get("extensions")
zend_extensions = php.get("zend_extensions")
if not isinstance(extensions, list) or extensions != sorted(set(extensions)):
    raise SystemExit("PHP extension list is not sorted and unique")
if not isinstance(zend_extensions, list) or zend_extensions != sorted(set(zend_extensions)):
    raise SystemExit("PHP Zend extension list is not sorted and unique")
document["configure"] = {
    "args": configure_args,
    "command": ["configure", *configure_args],
}
path.write_text(
    json.dumps(document, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

publish_no_clobber() {
    local source="$1"
    local destination="$2"

    [[ -d "$source" && ! -L "$source" ]] \
        || die "publish source is not a real directory: $source"
    [[ ! -e "$destination" && ! -L "$destination" ]] \
        || die "publish destination already exists: $destination"

    python3 - "$source" "$destination" <<'PY'
import ctypes
import errno
import os
import platform
import sys
from pathlib import Path

source = Path(sys.argv[1])
destination = Path(sys.argv[2])
if not source.is_dir() or source.is_symlink():
    raise SystemExit(f"publish source is not a real directory: {source}")
if os.path.lexists(destination):
    raise SystemExit(f"publish destination already exists: {destination}")
if source.stat().st_dev != destination.parent.stat().st_dev:
    raise SystemExit("publish source and destination are not on the same filesystem")

libc = ctypes.CDLL(None, use_errno=True)
system = platform.system()
source_bytes = os.fsencode(source)
destination_bytes = os.fsencode(destination)
at_fdcwd = -2

if system == "Linux":
    try:
        rename = libc.renameat2
    except AttributeError:
        raise SystemExit("renameat2(RENAME_NOREPLACE) is unavailable")
    rename.argtypes = [
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_uint,
    ]
    rename.restype = ctypes.c_int
    result = rename(at_fdcwd, source_bytes, at_fdcwd, destination_bytes, 1)
elif system == "Darwin":
    try:
        rename = libc.renameatx_np
    except AttributeError:
        raise SystemExit("renameatx_np(RENAME_EXCL) is unavailable")
    rename.argtypes = [
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_uint,
    ]
    rename.restype = ctypes.c_int
    result = rename(at_fdcwd, source_bytes, at_fdcwd, destination_bytes, 0x00000004)
else:
    raise SystemExit(f"atomic no-clobber publish is unsupported on {system}")

if result != 0:
    error = ctypes.get_errno()
    if error in {errno.EEXIST, errno.ENOTEMPTY}:
        raise SystemExit(f"publish destination already exists: {destination}")
    raise SystemExit(f"atomic no-clobber publish failed: {os.strerror(error)}")
PY
}

write_profile_provenance() {
    local destination="$1"
    local profile="$2"
    local tag="$3"
    local inventory_tag_object="$4"
    local inventory_tag_commit="$5"
    local tag_object="$6"
    local tag_commit="$7"
    local head="$8"
    local inventory_sha="$9"
    local specification_sha="${10}"
    local script_sha="${11}"
    local build_environment_sha="${12}"
    local php_version="${13}"
    local source_date_epoch="${14}"
    local php_sha="${15}"
    local metadata_sha="${16}"
    local install_tree_sha="${17}"
    local dependencies_sha="${18}"
    local git_version="${19}"
    local autoconf_version="${20}"
    local bison_version="${21}"
    local re2c_version="${22}"
    local make_version="${23}"
    local cc_version="${24}"
    shift 24

    python3 - "$destination" "$profile" "$tag" "$inventory_tag_object" \
        "$inventory_tag_commit" "$tag_object" "$tag_commit" "$head" \
        "$inventory_sha" "$specification_sha" "$script_sha" "$build_environment_sha" \
        "$php_version" "$source_date_epoch" "$php_sha" "$metadata_sha" \
        "$install_tree_sha" "$dependencies_sha" \
        "$git_version" "$autoconf_version" "$bison_version" "$re2c_version" \
        "$make_version" "$cc_version" "$PHP_SRC_REPOSITORY" "$@" <<'PY'
import json
import re
import sys
from pathlib import Path

(
    destination,
    profile,
    tag,
    inventory_tag_object,
    inventory_tag_commit,
    tag_object,
    tag_commit,
    head,
    inventory_sha,
    specification_sha,
    script_sha,
    build_environment_sha,
    php_version,
    source_date_epoch,
    php_sha,
    metadata_sha,
    install_tree_sha,
    dependencies_sha,
    git_version,
    autoconf_version,
    bison_version,
    re2c_version,
    make_version,
    cc_version,
    repository,
    *configure_args,
) = sys.argv[1:]

if inventory_tag_object != tag_object:
    raise SystemExit("inventory tag object does not match fetched tag object")
if inventory_tag_commit != tag_commit or tag_commit != head:
    raise SystemExit("inventory tag commit, fetched tag commit, and HEAD do not match")
digests = {
    "inventory": inventory_sha,
    "specification": specification_sha,
    "script": script_sha,
    "build environment": build_environment_sha,
    "PHP binary": php_sha,
    "runtime metadata": metadata_sha,
    "install tree": install_tree_sha,
    "dynamic dependencies": dependencies_sha,
}
for label, digest in digests.items():
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise SystemExit(f"{label} SHA-256 is malformed")
if not all(
    [git_version, autoconf_version, bison_version, re2c_version, make_version, cc_version]
):
    raise SystemExit("one or more build tool versions are empty")

profile_root = Path(destination).parent
runtime_metadata = json.loads(
    (profile_root / "php-metadata.json").read_text(encoding="utf-8")
)
build_environment = json.loads(
    (profile_root.parent / "build-environment.json").read_text(encoding="utf-8")
)
if runtime_metadata.get("php", {}).get("version") != php_version:
    raise SystemExit("runtime metadata PHP version does not match provenance")
if runtime_metadata.get("configure", {}).get("args") != configure_args:
    raise SystemExit("runtime metadata configure args do not match provenance")
toolchain_overrides = build_environment.get("toolchain_overrides", {})
expected_override_names = {"CC", "CFLAGS", "CPPFLAGS", "LDFLAGS", "PKG_CONFIG_PATH"}
if set(toolchain_overrides) != expected_override_names:
    raise SystemExit("build environment toolchain override set is incomplete")
build_flags = {
    name: details.get("value") if details.get("set") else None
    for name, details in sorted(toolchain_overrides.items())
}
runtime_php = runtime_metadata.get("php", {})
if not isinstance(runtime_php.get("ini"), dict):
    raise SystemExit("runtime metadata INI configuration is missing")
for extension_key in ("extensions", "zend_extensions"):
    extension_values = runtime_php.get(extension_key)
    if (
        not isinstance(extension_values, list)
        or extension_values != sorted(set(extension_values))
    ):
        raise SystemExit(f"runtime metadata {extension_key} is not sorted and unique")

document = {
    "schema": "elephc.pinned-php-src-build.v2",
    "profile": profile,
    "repository": repository,
    "inputs": {
        "inventory_sha256": inventory_sha,
        "wasm_specification_sha256": specification_sha,
        "builder_script_sha256": script_sha,
        "build_environment": "../build-environment.json",
        "build_environment_sha256": build_environment_sha,
    },
    "source": {
        "tag": tag,
        "inventory_tag_object": inventory_tag_object,
        "inventory_tag_commit": inventory_tag_commit,
        "tag_object": tag_object,
        "tag_commit": tag_commit,
        "peeled_commit": tag_commit,
        "head": head,
        "detached": True,
        "dirty": False,
        "materialization": "git archive of verified HEAD",
        "source_date_epoch": int(source_date_epoch),
    },
    "build": {
        "configure_args": configure_args,
        "configure_command": ["configure", *configure_args],
        "build_flags": build_flags,
        "environment": "../build-environment.json",
        "ini_mode": "-n",
        "tools": {
            "git": git_version,
            "autoconf": autoconf_version,
            "bison": bison_version,
            "re2c": re2c_version,
            "make": make_version,
            "cc": cc_version,
        },
    },
    "runtime": {
        "ini_mode": "-n",
        "ini": runtime_php.get("ini"),
        "extensions": runtime_php.get("extensions"),
        "zend_extensions": runtime_php.get("zend_extensions"),
    },
    "artifact": {
        "php_binary": "install/bin/php",
        "php_version": php_version,
        "php_sha256": php_sha,
        "runtime_metadata": "php-metadata.json",
        "runtime_metadata_sha256": metadata_sha,
        "install_tree": "install-tree.json",
        "install_tree_sha256": install_tree_sha,
        "dynamic_dependencies": "dynamic-dependencies.txt",
        "dynamic_dependencies_sha256": dependencies_sha,
    },
}
Path(destination).write_text(
    json.dumps(document, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

build_profile() {
    local work_root="$1"
    local result_root="$2"
    local profile="$3"
    local tag="$4"
    local inventory_tag_object="$5"
    local inventory_tag_commit="$6"
    local inventory_sha="$7"
    local specification_sha="$8"
    local script_sha="$9"
    local build_environment_sha="${10}"
    local jobs="${11}"
    local checkout="$work_root/checkouts/$profile"
    local source="$work_root/sources/$profile"
    local build="$work_root/build/$profile"
    local profile_root="$result_root/$profile"
    local expected_version="${tag#php-}"
    local verification=""
    local tag_object=""
    local tag_commit=""
    local head=""
    local source_date_epoch=""
    local php_binary=""
    local php_version="$expected_version"
    local php_sha=""
    local metadata_sha=""
    local install_tree_sha=""
    local dependencies_sha=""
    local hash_manifest=""

    printf '==> PHP %s: fetching pinned tag object %s\n' "$profile" "$tag"
    mkdir -p -- "$checkout" "$source" "$build" "$profile_root"
    run_build_env git -C "$checkout" init --quiet
    run_build_env git -C "$checkout" remote add origin "$PHP_SRC_REPOSITORY"
    run_build_env git -C "$checkout" fetch --quiet --no-tags --depth=1 origin \
        "refs/tags/$tag:refs/tags/$tag"
    tag_object="$(run_build_env git -C "$checkout" rev-parse --verify "refs/tags/$tag")"
    [[ "$tag_object" == "$inventory_tag_object" ]] \
        || die "tag $tag object is $tag_object, expected inventory object $inventory_tag_object"
    tag_commit="$(run_build_env git -C "$checkout" rev-parse --verify "${tag}^{commit}")"
    [[ "$tag_commit" == "$inventory_tag_commit" ]] \
        || die "tag $tag peels to $tag_commit, expected inventory commit $inventory_tag_commit"
    run_build_env git -C "$checkout" fetch --quiet --no-tags --depth=1 origin \
        "$inventory_tag_commit"
    run_build_env git -C "$checkout" checkout --quiet --detach "$inventory_tag_commit"

    verification="$(
        verify_checkout \
            "$checkout" "$tag" "$inventory_tag_object" "$inventory_tag_commit"
    )"
    IFS=$'\t' read -r tag_object tag_commit head <<<"$verification"
    [[ -n "$tag_object" && -n "$tag_commit" && -n "$head" ]] \
        || die "incomplete checkout verification for $profile"
    source_date_epoch="$(run_build_env git -C "$checkout" show -s --format=%ct HEAD)"
    [[ "$source_date_epoch" =~ ^[0-9]+$ ]] \
        || die "invalid source timestamp for $profile: $source_date_epoch"

    run_build_env git -C "$checkout" archive --format=tar "$head" \
        | run_build_env tar -xf - -C "$source"
    [[ -x "$source/buildconf" ]] || die "php-src $profile has no executable buildconf"

    printf '==> PHP %s: buildconf and minimal CLI configure\n' "$profile"
    (
        cd -- "$source"
        run_reproducible_build_env "$source_date_epoch" ./buildconf --force
    )
    [[ -x "$source/configure" ]] || die "buildconf did not create configure for $profile"
    (
        cd -- "$build"
        run_reproducible_build_env \
            "$source_date_epoch" "$source/configure" "${CONFIGURE_ARGS[@]}"
    )

    printf '==> PHP %s: make -j%s and transactional install\n' "$profile" "$jobs"
    run_reproducible_build_env "$source_date_epoch" make -C "$build" -j"$jobs"
    run_reproducible_build_env "$source_date_epoch" make -C "$build" \
        INSTALL_ROOT="$profile_root" install

    php_binary="$profile_root/install/bin/php"
    [[ -x "$php_binary" ]] || die "installed PHP CLI is missing for $profile"
    write_php_metadata \
        "$php_binary" "$source_date_epoch" "$expected_version" \
        "$profile_root/php-metadata.json"
    write_dynamic_dependencies \
        "$php_binary" "$profile_root/dynamic-dependencies.txt"
    install_tree_manifest \
        write "$profile_root/install" "$profile_root/install-tree.json"
    install_tree_manifest \
        verify "$profile_root/install" "$profile_root/install-tree.json"

    verification="$(
        verify_checkout \
            "$checkout" "$tag" "$inventory_tag_object" "$inventory_tag_commit"
    )"
    IFS=$'\t' read -r tag_object tag_commit head <<<"$verification"
    [[ "$tag_object" == "$inventory_tag_object" \
        && "$tag_commit" == "$inventory_tag_commit" \
        && "$tag_commit" == "$head" ]] \
        || die "checkout provenance changed while building $profile"

    php_sha="$(sha256_file "$php_binary")"
    metadata_sha="$(sha256_file "$profile_root/php-metadata.json")"
    install_tree_sha="$(sha256_file "$profile_root/install-tree.json")"
    dependencies_sha="$(sha256_file "$profile_root/dynamic-dependencies.txt")"
    write_profile_provenance \
        "$profile_root/provenance.json" \
        "$profile" "$tag" "$inventory_tag_object" "$inventory_tag_commit" \
        "$tag_object" "$tag_commit" "$head" "$inventory_sha" \
        "$specification_sha" "$script_sha" "$build_environment_sha" \
        "$php_version" "$source_date_epoch" "$php_sha" "$metadata_sha" \
        "$install_tree_sha" "$dependencies_sha" \
        "$(first_version_line run_build_env git --version)" \
        "$(first_version_line run_build_env autoconf --version)" \
        "$(first_version_line run_build_env bison --version)" \
        "$(first_version_line run_build_env re2c --version)" \
        "$(first_version_line run_build_env make --version)" \
        "$(first_version_line run_build_env cc --version)" \
        "${CONFIGURE_ARGS[@]}"

    hash_manifest="$profile_root/hashes.sha256"
    : >"$hash_manifest"
    write_hash_line "$profile_root" "install/bin/php" "$hash_manifest"
    write_hash_line "$profile_root" "install-tree.json" "$hash_manifest"
    write_hash_line "$profile_root" "php-metadata.json" "$hash_manifest"
    write_hash_line "$profile_root" "dynamic-dependencies.txt" "$hash_manifest"
    write_hash_line "$profile_root" "provenance.json" "$hash_manifest"
    install_tree_manifest \
        verify "$profile_root/install" "$profile_root/install-tree.json"
    verify_hash_manifest "$profile_root" "$hash_manifest"
}

write_root_provenance() {
    local result_root="$1"
    local inventory_sha="$2"
    local specification_sha="$3"
    local specification_pin="$4"
    local script_sha="$5"
    local build_environment_sha="$6"
    local elephc_head="$7"
    local elephc_dirty="$8"
    local elephc_status_sha="$9"
    local inventory_path="${10}"
    local specification_path="${11}"
    shift 11

    python3 - "$result_root" "$inventory_sha" "$specification_sha" \
        "$specification_pin" "$script_sha" "$build_environment_sha" \
        "$elephc_head" "$elephc_dirty" "$elephc_status_sha" \
        "$inventory_path" "$specification_path" "$PHP_SRC_REPOSITORY" "$@" <<'PY'
import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
inventory_sha = sys.argv[2]
specification_sha = sys.argv[3]
specification_pin = sys.argv[4]
script_sha = sys.argv[5]
build_environment_sha = sys.argv[6]
elephc_head = sys.argv[7]
elephc_dirty_text = sys.argv[8]
elephc_status_sha = sys.argv[9]
inventory_path = sys.argv[10]
specification_path = sys.argv[11]
repository = sys.argv[12]
expected_profiles = sys.argv[13:]
if not expected_profiles:
    raise SystemExit("root provenance requires at least one selected profile")
if any(profile not in {"8.2", "8.3", "8.4", "8.5"} for profile in expected_profiles):
    raise SystemExit("root provenance contains an invalid selected profile")
if len(expected_profiles) != len(set(expected_profiles)):
    raise SystemExit("root provenance contains duplicate selected profiles")
if specification_sha != specification_pin:
    raise SystemExit("WASM specification file does not match its inventory pin")
for label, digest in {
    "inventory": inventory_sha,
    "specification": specification_sha,
    "builder script": script_sha,
    "build environment": build_environment_sha,
    "Elephc status": elephc_status_sha,
}.items():
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise SystemExit(f"{label} SHA-256 is malformed")
if not re.fullmatch(r"[0-9a-f]{40}", elephc_head):
    raise SystemExit("Elephc HEAD is malformed")
if elephc_dirty_text not in {"true", "false"}:
    raise SystemExit("Elephc dirty state must be true or false")

profiles = []
for profile in expected_profiles:
    path = root / profile / "provenance.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    if document.get("schema") != "elephc.pinned-php-src-build.v2":
        raise SystemExit(f"profile provenance schema mismatch in {path}")
    if document.get("profile") != profile:
        raise SystemExit(f"profile provenance mismatch in {path}")
    profile_inputs = document.get("inputs", {})
    if profile_inputs.get("inventory_sha256") != inventory_sha:
        raise SystemExit(f"profile inventory hash mismatch in {path}")
    if profile_inputs.get("wasm_specification_sha256") != specification_sha:
        raise SystemExit(f"profile specification hash mismatch in {path}")
    if profile_inputs.get("builder_script_sha256") != script_sha:
        raise SystemExit(f"profile builder script hash mismatch in {path}")
    if profile_inputs.get("build_environment_sha256") != build_environment_sha:
        raise SystemExit(f"profile build environment hash mismatch in {path}")
    profiles.append(
        {
            "profile": profile,
            "tag": document["source"]["tag"],
            "tag_object": document["source"]["tag_object"],
            "tag_commit": document["source"]["tag_commit"],
            "peeled_commit": document["source"]["peeled_commit"],
            "php_binary": f"{profile}/{document['artifact']['php_binary']}",
            "php_version": document["artifact"]["php_version"],
            "php_sha256": document["artifact"]["php_sha256"],
            "configure_command": document["build"]["configure_command"],
            "configure_args": document["build"]["configure_args"],
            "build_flags": document["build"]["build_flags"],
            "ini_mode": document["runtime"]["ini_mode"],
            "ini": document["runtime"]["ini"],
            "extensions": document["runtime"]["extensions"],
            "zend_extensions": document["runtime"]["zend_extensions"],
            "install_tree_sha256": document["artifact"]["install_tree_sha256"],
            "runtime_metadata_sha256": document["artifact"]["runtime_metadata_sha256"],
            "dynamic_dependencies_sha256": document["artifact"]["dynamic_dependencies_sha256"],
            "provenance": f"{profile}/provenance.json",
            "hashes": f"{profile}/hashes.sha256",
        }
    )

document = {
    "schema": "elephc.pinned-php-src-build-set.v2",
    "repository": repository,
    "selection": expected_profiles,
    "inputs": {
        "inventory": {
            "path": inventory_path,
            "sha256": inventory_sha,
        },
        "wasm_specification": {
            "path": specification_path,
            "sha256": specification_sha,
            "inventory_pin_sha256": specification_pin,
        },
        "builder_script": {
            "path": "scripts/build-pinned-php-src.sh",
            "sha256": script_sha,
        },
        "build_environment": {
            "path": "build-environment.json",
            "sha256": build_environment_sha,
        },
        "elephc": {
            "head": elephc_head,
            "dirty": elephc_dirty_text == "true",
            "status": "elephc-git-status.txt",
            "status_sha256": elephc_status_sha,
        },
    },
    "profiles": profiles,
}
(root / "provenance.json").write_text(
    json.dumps(document, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

main() {
    local inventory="$DEFAULT_INVENTORY"
    local specification="$DEFAULT_SPECIFICATION"
    local output_dir=""
    local selected_profile="all"
    local jobs="$(detected_jobs)"
    local verify_pins_only=0
    local pins=""
    local selected_pins=""
    local parent=""
    local name=""
    local parent_abs=""
    local output_abs=""
    local work_root=""
    local result_root=""
    local inventory_sha=""
    local inventory_provenance_path=""
    local specification_sha=""
    local specification_pin=""
    local specification_provenance_path=""
    local script_sha=""
    local build_environment_sha=""
    local elephc_state=""
    local elephc_head=""
    local elephc_dirty=""
    local elephc_status_sha=""
    local final_elephc_state=""
    local final_elephc_head=""
    local final_elephc_dirty=""
    local final_elephc_status_sha=""
    local profile=""
    local tag=""
    local tag_object=""
    local tag_commit=""
    local profile_count=0
    local expected_profile_count=0
    local root_hashes=""
    local built_profiles=()
    local profile_option_seen=0

    while (($#)); do
        case "$1" in
            --output-dir)
                (($# >= 2)) || die "--output-dir requires a value"
                output_dir="$2"
                shift 2
                ;;
            --inventory)
                (($# >= 2)) || die "--inventory requires a value"
                inventory="$2"
                shift 2
                ;;
            --profile)
                (($# >= 2)) || die "--profile requires a value"
                ((profile_option_seen == 0)) || die "--profile may only be specified once"
                selected_profile="$2"
                profile_option_seen=1
                shift 2
                ;;
            --jobs)
                (($# >= 2)) || die "--jobs requires a value"
                jobs="$2"
                shift 2
                ;;
            --verify-pins-only)
                verify_pins_only=1
                shift
                ;;
            -h|--help)
                usage
                return 0
                ;;
            *)
                die "unknown argument: $1"
                ;;
        esac
    done

    require_command python3
    pins="$(extract_pins "$inventory")"
    selected_pins="$(filter_pins "$pins" "$selected_profile")"
    if ((verify_pins_only)); then
        [[ -z "$output_dir" ]] || die "--output-dir is incompatible with --verify-pins-only"
        printf '%s\n' "$selected_pins"
        return 0
    fi

    [[ -n "$output_dir" ]] || die "--output-dir is required"
    [[ "$jobs" =~ ^[1-9][0-9]*$ ]] || die "--jobs must be a positive integer"
    for command in env git tar autoconf bison re2c make cc sed awk uname; do
        require_command "$command"
    done
    if ! command -v sha256sum >/dev/null 2>&1 \
        && ! command -v shasum >/dev/null 2>&1; then
        die "sha256sum or shasum is required"
    fi
    prepare_build_environment
    verify_build_tool_versions
    specification_pin="$(extract_specification_pin "$inventory")"
    specification_sha="$(verify_specification_pin "$inventory" "$specification")"
    inventory_sha="$(sha256_file "$inventory")"
    inventory_provenance_path="$(provenance_path "$inventory")"
    specification_provenance_path="$(provenance_path "$specification")"
    script_sha="$(sha256_file "$SCRIPT_DIR/build-pinned-php-src.sh")"

    [[ "$output_dir" != "/" ]] || die "refusing to use / as the output directory"
    [[ ! -e "$output_dir" && ! -L "$output_dir" ]] \
        || die "output directory already exists: $output_dir"
    parent="$(dirname -- "$output_dir")"
    name="$(basename -- "$output_dir")"
    [[ -n "$name" && "$name" != "." && "$name" != ".." ]] \
        || die "invalid output directory: $output_dir"
    mkdir -p -- "$parent"
    parent_abs="$(cd -- "$parent" && pwd -P)"
    output_abs="$parent_abs/$name"
    [[ ! -e "$output_abs" && ! -L "$output_abs" ]] \
        || die "output directory already exists: $output_abs"

    STAGING_DIR="$(mktemp -d "$parent_abs/.php-src-build.XXXXXX")"
    trap cleanup EXIT
    trap 'exit 130' HUP INT TERM
    work_root="$STAGING_DIR/work"
    result_root="$STAGING_DIR/result"
    mkdir -p -- "$work_root" "$result_root"

    write_build_environment_manifest "$result_root/build-environment.json"
    build_environment_sha="$(sha256_file "$result_root/build-environment.json")"
    elephc_state="$(capture_elephc_state "$result_root/elephc-git-status.txt")"
    IFS=$'\t' read -r elephc_head elephc_dirty elephc_status_sha <<<"$elephc_state"
    [[ -n "$elephc_head" && -n "$elephc_dirty" && -n "$elephc_status_sha" ]] \
        || die "incomplete Elephc repository state"

    while IFS=$'\t' read -r profile tag tag_object tag_commit; do
        [[ -n "$profile" && -n "$tag" && -n "$tag_object" && -n "$tag_commit" ]] \
            || die "incomplete canonical pin record"
        build_profile \
            "$work_root" "$result_root" "$profile" "$tag" "$tag_object" \
            "$tag_commit" "$inventory_sha" "$specification_sha" "$script_sha" \
            "$build_environment_sha" "$jobs"
        built_profiles+=("$profile")
        profile_count=$((profile_count + 1))
    done <<<"$selected_pins"
    if [[ "$selected_profile" == "all" ]]; then
        expected_profile_count="${#EXPECTED_PROFILES[@]}"
    else
        expected_profile_count=1
    fi
    [[ "$profile_count" -eq "$expected_profile_count" ]] \
        || die "built $profile_count profiles, expected $expected_profile_count"

    write_root_provenance \
        "$result_root" "$inventory_sha" "$specification_sha" "$specification_pin" \
        "$script_sha" "$build_environment_sha" "$elephc_head" "$elephc_dirty" \
        "$elephc_status_sha" "$inventory_provenance_path" \
        "$specification_provenance_path" "${built_profiles[@]}"
    root_hashes="$result_root/hashes.sha256"
    : >"$root_hashes"
    write_hash_line "$result_root" "build-environment.json" "$root_hashes"
    write_hash_line "$result_root" "elephc-git-status.txt" "$root_hashes"
    write_hash_line "$result_root" "provenance.json" "$root_hashes"
    for profile in "${built_profiles[@]}"; do
        write_hash_line "$result_root" "$profile/install/bin/php" "$root_hashes"
        write_hash_line "$result_root" "$profile/install-tree.json" "$root_hashes"
        write_hash_line "$result_root" "$profile/php-metadata.json" "$root_hashes"
        write_hash_line "$result_root" "$profile/dynamic-dependencies.txt" "$root_hashes"
        write_hash_line "$result_root" "$profile/provenance.json" "$root_hashes"
        write_hash_line "$result_root" "$profile/hashes.sha256" "$root_hashes"
        install_tree_manifest \
            verify "$result_root/$profile/install" "$result_root/$profile/install-tree.json"
        verify_hash_manifest \
            "$result_root/$profile" "$result_root/$profile/hashes.sha256"
    done
    verify_hash_manifest "$result_root" "$root_hashes"

    [[ "$(sha256_file "$inventory")" == "$inventory_sha" ]] \
        || die "inventory changed while builds were running"
    [[ "$(sha256_file "$specification")" == "$specification_sha" ]] \
        || die "WASM specification changed while builds were running"
    [[ "$(sha256_file "$SCRIPT_DIR/build-pinned-php-src.sh")" == "$script_sha" ]] \
        || die "builder script changed while builds were running"
    [[ "$(sha256_file "$result_root/build-environment.json")" == "$build_environment_sha" ]] \
        || die "build environment manifest changed while builds were running"

    final_elephc_state="$(capture_elephc_state "$work_root/elephc-git-status.final.txt")"
    IFS=$'\t' read -r \
        final_elephc_head final_elephc_dirty final_elephc_status_sha \
        <<<"$final_elephc_state"
    [[ "$final_elephc_head" == "$elephc_head" \
        && "$final_elephc_dirty" == "$elephc_dirty" \
        && "$final_elephc_status_sha" == "$elephc_status_sha" ]] \
        || die "Elephc HEAD or dirty state changed while builds were running"

    [[ ! -e "$output_abs" && ! -L "$output_abs" ]] \
        || die "output directory appeared before publication: $output_abs"
    publish_no_clobber "$result_root" "$output_abs"
    printf 'Built and verified pinned php-src profile(s) %s in %s\n' \
        "${built_profiles[*]}" "$output_abs"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
