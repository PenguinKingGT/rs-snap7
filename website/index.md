---
layout: home
hero:
  name: rs-snap7
  text: Siemens S7 communication in Rust
  tagline: An async-first client, an optional blocking API, and tools for classic S7 workflows.
  image:
    src: /logo.svg
    alt: rs-snap7
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: API reference
      link: /api/
features:
  - title: One async core
    details: Native Tokio I/O with serialized sessions, cancellation handling, recovery, and optional blocking adapters.
  - title: Checked data access
    details: Raw memory, typed values, bit updates, tags, DB layouts, and multi-variable operations.
  - title: Tools for development
    details: A local emulator, CLI, diagnostic services, and independent software tests without PLC hardware.
---

## Project status

rs-snap7 is a Rust port of python-snap7. All 28 baseline migration features are
implemented. The package is still in development and is not published to crates.io.
Real PLC, serial PPI, and live DCP acceptance remain pending.

Start with the [local emulator](guide/getting-started.md), choose
[async or blocking access](guide/async-and-blocking.md), and review the
[scope and limits](guide/errors-and-limits.md) before hardware integration.

For hardware, start with [PLC connections](guide/plc-connections.md). For local development, use the [server tutorial](guide/server.md); diagnose failures with [troubleshooting](guide/troubleshooting.md).
