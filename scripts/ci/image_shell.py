#!/usr/bin/env python3
"""Execute consumer scripts in pinned images with the runner's service network and paths."""

import json
import os
from pathlib import Path
import re
import shutil
import shlex
import tempfile
import subprocess
import sys

IMAGE_PATTERN = r"ghcr\.io/illegalstudio/elephc-ci@sha256:[0-9a-f]{64}"
IMAGE_PATH = "/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"


def image_ref(value):
    """Accept only immutable images from this repository's CI registry."""
    if not re.fullmatch(IMAGE_PATTERN, value):
        raise ValueError("Expected an immutable repository CI image digest")
    return value


def install():
    """Expose a trusted shell on PATH without expressions or a working-directory dependency."""
    temporary = Path(os.environ["RUNNER_TEMP"])
    target = temporary / "elephc-image-shell.py"
    shutil.copyfile(__file__, target)
    target.chmod(0o755)
    directory = temporary / "elephc-image-bin"
    directory.mkdir(exist_ok=True)
    launcher = directory / "elephc-ci-shell"
    if not launcher.exists():
        launcher.symlink_to(target)
    with open(os.environ["GITHUB_PATH"], "a") as output:
        output.write(f"{directory}\n")


def prepare(role, catalog):
    """Pull once and retain trusted tooling before a possible historical source checkout."""
    images = json.loads(Path(catalog).read_text())
    image = image_ref(images[role])
    subprocess.run(["docker", "pull", image], check=True)
    if role == "pdo-core":
        subprocess.run(["docker", "pull", image_ref(images["kdc"])], check=True)
    directory = Path(os.environ["RUNNER_TEMP"])
    shutil.copyfile(__file__, directory / "elephc-image-shell.py")
    home = directory / "elephc-image-home"
    home.mkdir(exist_ok=True)
    (home / ".cargo").mkdir(exist_ok=True)
    with open(os.environ["GITHUB_ENV"], "a") as output:
        output.write(f"ELEPHC_CI_EXEC_IMAGE={image}\nELEPHC_CI_EXEC_ROLE={role}\n")
        if role == "pdo-core":
            output.write(f"ELEPHC_GSS_KDC_IMAGE={image_ref(images['kdc'])}\n")


def invocation(script, environment):
    """Preserve local services, same-path mounts, fresh step env, and the Cargo build guard."""
    image = image_ref(environment["ELEPHC_CI_EXEC_IMAGE"])
    root = Path(environment["GITHUB_WORKSPACE"]).resolve()
    temporary = Path(environment["RUNNER_TEMP"]).resolve()
    script = Path(script).resolve()
    if not script.is_relative_to(temporary):
        raise ValueError("The generated step script must be inside RUNNER_TEMP")
    home = temporary / "elephc-image-home"
    command = ["docker", "run", "--rm", "--init", "--network=host",
               "--user", f"{os.getuid()}:{os.getgid()}",
               "--workdir", str(Path.cwd()),
               "--volume", f"{root}:{root}", "--volume", f"{temporary}:{temporary}",
               "--volume", "/etc/passwd:/etc/passwd:ro", "--volume", "/etc/group:/etc/group:ro"]
    # Only PDO fixtures need the host Docker daemon. Same absolute paths make
    # their nested fixture bind mounts valid on both sides of the socket.
    if environment.get("ELEPHC_CI_EXEC_ROLE") in ("pdo-core", "pdo"):
        docker = shutil.which("docker")
        socket = Path("/var/run/docker.sock")
        if not docker or not socket.exists():
            raise ValueError("PDO execution requires the runner Docker client and socket")
        command += ["--volume", f"{docker}:/usr/bin/docker:ro",
                    "--volume", f"{socket}:{socket}", "--group-add", str(socket.stat().st_gid)]
    overrides = {"HOME": str(home), "TMPDIR": str(temporary),
                 "CARGO_HOME": str(home / ".cargo"), "RUSTUP_HOME": "/usr/local/rustup",
                 "PATH": IMAGE_PATH}
    if environment.get("ELEPHC_CI_EXEC_ROLE", "").startswith(("php-", "reference")):
        overrides["PATH"] = "/opt/elephc/php/bin:" + overrides["PATH"]
    if environment.get("ELEPHC_CI_BUILD_GUARD_LOG"):
        overrides["PATH"] = str(temporary / "elephc-consumer-bin") + ":" + overrides["PATH"]
        overrides["ELEPHC_CI_REAL_CARGO"] = "/usr/local/cargo/bin/cargo"
    # Pass names, never secret values, in argv. Docker reads the current env,
    # including multiline values and updates from each previous GITHUB_ENV file.
    for key in sorted(environment):
        if key not in overrides and key not in ("ELEPHC_CI_IMAGE", "LD_LIBRARY_PATH"):
            command += ["--env", key]
    for key, value in overrides.items():
        command += ["--env", f"{key}={value}"]
    # Preserve vendor image defaults unless an earlier step supplied a path.
    if environment.get("LD_LIBRARY_PATH"):
        command += ["--env", "LD_LIBRARY_PATH"]
    command += [image, "bash", "--noprofile", "--norc", "-e", "-o", "pipefail", str(script)]
    return command


def main():
    """Prepare a pinned environment or execute one generated Actions step."""
    if sys.argv[1] == "install":
        install()
    elif sys.argv[1] == "prepare":
        prepare(sys.argv[2], sys.argv[3])
    elif sys.argv[1] == "command":
        if sys.platform == "darwin":
            os.execvp(sys.argv[2], sys.argv[2:])
        with tempfile.NamedTemporaryFile(mode="w", suffix=".sh", dir=os.environ["RUNNER_TEMP"]) as step:
            step.write(shlex.join(sys.argv[2:]) + "\n")
            step.flush()
            raise SystemExit(subprocess.run(invocation(step.name, os.environ)).returncode)
    elif sys.platform == "darwin":
        os.execvp("bash", ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", sys.argv[1]])
    else:
        os.execvp("docker", invocation(sys.argv[1], os.environ))


if __name__ == "__main__":
    main()
