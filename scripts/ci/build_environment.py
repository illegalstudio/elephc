"""Record actual compiler/SDK inputs and validate the consumer's executable ABI."""

import os
import platform
import re
import shutil
import subprocess


def command(*args):
    """Collect a stable tool identity without assuming every host has the tool."""
    if not shutil.which(args[0]):
        return ""
    result = subprocess.run(args, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode:
        raise ValueError(f"Cannot identify build environment: {args[0]}")
    return result.stdout.strip()


def snapshot(target):
    """Preserve immutable image identity plus the actual native and Apple toolchains."""
    image = os.environ.get("ELEPHC_CI_IMAGE", "")
    if image and not re.fullmatch(r"ghcr.io/illegalstudio/elephc-ci@sha256:[0-9a-f]{64}", image):
        raise ValueError("CI container identity must be an immutable manifest digest")
    sdk = {"ios-arm64": "iphoneos", "ios-sim-arm64": "iphonesimulator",
           "macos-aarch64": "macosx"}.get(target)
    return {"system": platform.system(), "machine": platform.machine(),
            "glibc": platform.libc_ver()[1] if platform.system() == "Linux" else "",
            "container": image, "runner_image": os.environ.get("ImageVersion", ""),
            "rustc": command("rustc", "-vV"), "cc": command("cc", "--version"),
            "xcode": command("xcodebuild", "-version") if sdk else "",
            "sdk": command("xcrun", "--sdk", sdk, "--show-sdk-version") if sdk else "",
            "flags": {name: value for name, value in os.environ.items()
                      if name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "MACOSX_DEPLOYMENT_TARGET",
                                  "IPHONEOS_DEPLOYMENT_TARGET", "SDKROOT", "CARGO_BUILD_TARGET")}}


def validate(producer, target):
    """Reject a different architecture, older glibc, or a drifting pinned container."""
    host = platform.system()
    machine = {"arm64": "aarch64", "x86_64": "x86_64", "aarch64": "aarch64"}.get(platform.machine())
    wanted = "x86_64" if target == "linux-x86_64" else "aarch64"
    expected_host = "Linux" if target.startswith("linux-") else "Darwin"
    if host != expected_host or machine != wanted:
        raise ValueError("Artifact cannot execute or link on this host architecture")
    if producer.get("system") != host:
        raise ValueError("Artifact producer operating system mismatch")
    current_image = os.environ.get("ELEPHC_CI_IMAGE")
    if current_image and producer.get("container") != current_image:
        raise ValueError("Artifact CI container digest mismatch")
    if host == "Linux":
        built = tuple(map(int, producer.get("glibc", "0").split(".")))
        current = tuple(map(int, platform.libc_ver()[1].split(".")))
        if not built or current < built:
            raise ValueError("Consumer glibc is older than the artifact build environment")
    if target.startswith("ios-"):
        sdk = "iphoneos" if target == "ios-arm64" else "iphonesimulator"
        if producer.get("sdk") != command("xcrun", "--sdk", sdk, "--show-sdk-version"):
            raise ValueError("iOS artifact SDK differs from the link-test SDK")
