# Optional Dependencies

The default async client depends on Tokio. Optional products add the dependencies
below. Rust 1.95 or newer is required for all features. Cargo.lock pins versions
for repository and release checks. Source reuse attribution remains in LICENSE/NOTICE.

| Dependency | Feature | Declared SPDX/license expression |
| --- | --- | --- |
| tokio | Core; blocking adds multi-thread runtime | MIT |
| csv | tag-import | Unlicense / MIT |
| serde, serde_json | tag-import; serde_json also cli | MIT OR Apache-2.0 |
| roxmltree | tag-import | MIT OR Apache-2.0 |
| serialport | ppi | MPL-2.0 |
| pnet_datalink | discovery, Linux/macOS target dependency | MIT OR Apache-2.0 |
| clap | cli | MIT OR Apache-2.0 |
| ctrlc | cli | MIT / Apache-2.0 |
| sysinfo | demo | MIT |

These expressions are recorded from the resolved packages' Cargo manifests.
Transitive packages retain their own licenses; shipping an application should
include the licenses required by the selected dependency graph. No dependency
source files have been modified or vendored here. serialport uses the library
under its MPL-2.0 terms; libudev defaults are disabled. sysinfo enables only
system, disk, network and component features used by the demo.

See [usage.md](usage.md) for supported platforms and runtime requirements. Building an
optional adapter does not constitute live hardware or network acceptance.
