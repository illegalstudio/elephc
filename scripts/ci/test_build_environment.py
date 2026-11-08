"""Reject artifacts which cannot execute in the receiving ABI environment."""

import unittest
from unittest.mock import patch

import build_environment as subject


class EnvironmentTests(unittest.TestCase):
    def test_linux_rejects_older_glibc_other_architecture_and_image_drift(self):
        producer = {"system": "Linux", "glibc": "2.39", "container": "image-a"}
        with patch.object(subject.platform, "system", return_value="Linux"), \
             patch.object(subject.platform, "machine", return_value="x86_64"), \
             patch.object(subject.platform, "libc_ver", return_value=("glibc", "2.39")), \
             patch.dict(subject.os.environ, {"ELEPHC_CI_IMAGE": "image-a"}):
            subject.validate(producer, "linux-x86_64")
            with self.assertRaises(ValueError):
                subject.validate(producer, "linux-aarch64")
            with self.assertRaises(ValueError):
                subject.validate({**producer, "container": "image-b"}, "linux-x86_64")
            with self.assertRaises(ValueError):
                subject.validate({**producer, "glibc": "2.40"}, "linux-x86_64")

    def test_snapshot_rejects_mutable_container_tag(self):
        with patch.dict(subject.os.environ, {"ELEPHC_CI_IMAGE": "ghcr.io/illegalstudio/elephc-ci:latest"}):
            with self.assertRaises(ValueError):
                subject.snapshot("linux-x86_64")

    def test_ios_requires_the_same_sdk_as_the_link_fixture(self):
        with patch.object(subject.platform, "system", return_value="Darwin"), \
             patch.object(subject.platform, "machine", return_value="arm64"), \
             patch.dict(subject.os.environ, {"ELEPHC_CI_IMAGE": ""}), \
             patch.object(subject, "command", return_value="19.0"):
            subject.validate({"system": "Darwin", "sdk": "19.0"}, "ios-sim-arm64")
            with self.assertRaises(ValueError):
                subject.validate({"system": "Darwin", "sdk": "18.0"}, "ios-sim-arm64")


if __name__ == "__main__":
    unittest.main()
