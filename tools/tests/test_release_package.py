"""Regression cases for release payload boundaries and required attribution."""

import importlib.util
from pathlib import Path
import tarfile
import unittest

spec = importlib.util.spec_from_file_location(
    "release_package", Path(__file__).parents[1] / "check_release_package.py"
)
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class PackageValidationTests(unittest.TestCase):
    """Exercise archive mistakes independently of Cargo's include rules."""

    ROOT = "rs-snap7-0.1.0"

    def members(self, extra=()):
        """Create an otherwise valid payload with caller-selected extra names."""
        return [tarfile.TarInfo(self.ROOT + "/" + name)
                for name in sorted(release.REQUIRED) + list(extra)]

    def test_complete_archive_preserves_licenses_and_source(self):
        """Accept source/tests/docs together with all required attribution."""
        files = release.validate_members(self.members(["docs/usage.md"]), self.ROOT)
        self.assertIn("LICENSE", files)
        self.assertIn("NOTICE", files)
        self.assertIn("src/lib.rs", files)

    def test_reference_website_build_and_credential_files_are_rejected(self):
        """Never ship local references, generated output, or credential files."""
        for name in ["website/package.json", "website/node_modules/a.js",
                     "website/node_modules/vite/README.md",
                     "website/node_modules/vite/LICENSE",
                     "python-snap7/snap7/client.py", "target/debug/s7",
                     ".cargo/credentials.toml"]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                release.validate_members(self.members([name]), self.ROOT)

    def test_missing_notice_is_rejected(self):
        """An otherwise compilable crate must still preserve attribution."""
        members = [m for m in self.members() if not m.name.endswith("/NOTICE")]
        with self.assertRaisesRegex(ValueError, "NOTICE"):
            release.validate_members(members, self.ROOT)

    def test_paths_links_and_duplicate_entries_are_rejected(self):
        """Reject paths outside the package root and ambiguous member names."""
        for name in ["", ".", "/tmp/file", self.ROOT + "/../secret", "other/src/lib.rs"]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                release.validate_members(self.members() + [tarfile.TarInfo(name)], self.ROOT)
        link = tarfile.TarInfo(self.ROOT + "/src/link.rs")
        link.type = tarfile.SYMTYPE
        with self.assertRaises(ValueError):
            release.validate_members(self.members() + [link], self.ROOT)
        with self.assertRaises(ValueError):
            release.validate_members(self.members(["src/lib.rs"]), self.ROOT)


if __name__ == "__main__":
    unittest.main()
