"""Validate the actual Cargo archive before publishing; never upload artifacts."""

import argparse
from pathlib import Path, PurePosixPath
import tarfile


REQUIRED = {
    "Cargo.toml", "Cargo.lock", "README.md", "README.zh-CN.md", "LICENSE",
    "NOTICE", "CHANGELOG.md", "RELEASING.md", "src/lib.rs",
    "examples/read_db.rs", "tests/common/mod.rs", "tools/python_peer.py",
}
ROOT_FILES = {
    "Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json",
    "README.md", "README.zh-CN.md", "LICENSE", "NOTICE", "CHANGELOG.md",
    "RELEASING.md",
}
DIRECTORIES = {"src", "examples", "tests", "docs"}


def validate_members(members, root):
    """Require expected files and reject unexpected paths or link entries."""
    files = set()
    for member in members:
        path = PurePosixPath(member.name)
        if (not path.parts or path.is_absolute() or ".." in path.parts
                or "\\" in member.name or path.parts[0] != root):
            raise ValueError("unexpected archive path: " + member.name)
        relative = path.relative_to(root).as_posix()
        if not member.isfile():
            raise ValueError("non-file archive entry: " + member.name)
        if relative in files:
            raise ValueError("duplicate archive file: " + relative)
        allowed = relative in ROOT_FILES or relative == "tools/python_peer.py"
        allowed = allowed or (len(path.parts) > 2 and path.parts[1] in DIRECTORIES)
        if not allowed:
            raise ValueError("unexpected package file: " + relative)
        files.add(relative)
    missing = REQUIRED - files
    if missing:
        raise ValueError("missing required package files: " + ", ".join(sorted(missing)))
    return files


def main():
    """Inspect a completed .crate file and report a nonzero status on mismatch."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    args = parser.parse_args()
    if args.archive.suffix != ".crate" or not args.archive.stem.startswith("rs-snap7-"):
        parser.error("expected an rs-snap7-VERSION.crate archive")
    try:
        with tarfile.open(args.archive, "r:gz") as archive:
            files = validate_members(archive.getmembers(), args.archive.stem)
    except (OSError, tarfile.TarError, ValueError) as error:
        parser.exit(1, "package validation failed: " + str(error) + "\n")
    print(f"Validated {len(files)} files; archive size {args.archive.stat().st_size:,} bytes.")


if __name__ == "__main__":
    main()
