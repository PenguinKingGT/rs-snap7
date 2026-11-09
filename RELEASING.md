# Release Procedure

## Prepared release

Version `0.1.0` is prepared for the first crates.io upload. No crate, Git tag,
GitHub release, or documentation deployment is created by these instructions.
The crate name was unregistered when checked on 2026-11-09; availability is not
reserved and must be checked again immediately before the first upload.

Rust 1.95 is the supported minimum for every feature combination. This includes
the optional `demo` dependency. The `0.1` release covers the implemented software
scope in [CHANGELOG.md](CHANGELOG.md). Physical PLC, serial PPI, and live DCP
acceptance remain pending; retain those limits in release notes.

## Package contents

Cargo.toml uses an explicit, root-anchored include list. The leading `/` also
prevents a root README/LICENSE pattern from including same-named files in nested
website dependencies. Source, examples, tests, existing
Markdown documentation, READMEs, changelog, release instructions, MIT license,
upstream NOTICE, and the Python peer test helper are included. The local
`python-snap7/` checkout, `website/`, Node modules, build output, CI configuration,
and maintainer-only tools are excluded. Cargo adds its normalized/original
manifest and Git revision metadata automatically.

The archive checker validates the actual `.crate`, including attribution and
payload boundaries. Tests using Python or PLC hardware stay opt-in; the Python
harness needs an external pinned upstream checkout and SNAP7_PYTHON_REFERENCE.

## Validate the candidate

Run from a clean checkout of the intended release commit:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --no-default-features
cargo test --locked --all-targets --features blocking
cargo test --locked --all-targets --all-features
cargo test --locked --doc --all-features
cargo +1.95.0 check --locked --all-targets --all-features
python3 -m unittest discover -s tools/tests
cargo package --locked --list
cargo package --locked --all-features
python3 tools/check_release_package.py target/package/rs-snap7-0.1.0.crate
cargo publish --dry-run --locked --all-features
```

Also test the extracted package, which verifies the archive independently of
files present only in the working checkout:

```sh
cargo test --locked --manifest-path target/package/rs-snap7-0.1.0/Cargo.toml --all-targets --all-features
cargo test --locked --manifest-path target/package/rs-snap7-0.1.0/Cargo.toml --doc --all-features
```

The existing OS/feature CI matrix and added minimum-version/package jobs must
pass on the release commit. Local macOS results do not establish Windows/Linux
or hardware acceptance. `cargo publish --dry-run` packages and verifies without
uploading. During local preparation, `--allow-dirty` can inspect uncommitted
metadata, but the final candidate must be verified again from a clean commit.

## Upload when explicitly approved

1. Confirm the intended crate name/version and that the checked commit is clean.
2. Authenticate as an authorized crates.io owner using `cargo login` on the
   publishing machine; never commit credentials or paste tokens into issues.
3. Run `cargo publish --locked --all-features`.
4. Confirm `0.1.0` is visible on crates.io and docs.rs builds all-feature API docs.
5. Create the `v0.1.0` Git tag at that exact commit and publish release notes from
   CHANGELOG.md; pushing the tag and creating a GitHub release are separate actions.
6. After upload is confirmed, update the README/site installation examples from
   Git dependencies to `rs-snap7 = "0.1.0"` and record the release date.

The manifest permits only the crates.io registry. It does not automate upload.
A published version cannot be overwritten; corrections require a new version.
Follow the [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).

## Local preparation record

Validated on macOS on 2026-11-09 with Rust 1.97.1:

| Check | Result |
| --- | --- |
| Default all-target tests | 101 passed |
| Blocking all-target tests | 152 passed; 2 hardware tests ignored |
| All-feature all-target tests | 190 passed; 4 opt-in tests ignored |
| Extracted all-feature package tests | 190 passed; 4 opt-in tests ignored |
| Workspace and packaged all-feature doctests | 3 passed each |
| Rust 1.95 all-target/all-feature check | Passed |
| Release payload regression tests | 4 passed |
| Explicit Python cross-peer acceptance | 2 passed |
| rustfmt, strict Clippy, warning-free Rustdoc | Passed |
| Crate archive | 114 expected files; about 202 KiB compressed |
| Website production build | Passed |

The four all-feature opt-in tests are two Python cross-peer tests and two hardware
tests. The Python tests were then run explicitly and passed. Physical hardware
and remote CI were not run. Recheck the final clean
commit's package and dry run immediately before uploading.
