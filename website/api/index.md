<script setup>
import { withBase } from 'vitepress';
</script>

# API Reference

The site build generates complete API documentation from the current Rust source:
method signatures, parameters, return values, errors, and source links.

<a :href="withBase('/rustdoc/rs_snap7/index.html')" target="_blank" rel="noopener">Open the complete Rust API</a>

<a :href="withBase('/rustdoc/rs_snap7/client/struct.Client.html')" target="_blank" rel="noopener">Async Client</a> ·
<a :href="withBase('/rustdoc/rs_snap7/blocking/struct.Client.html')" target="_blank" rel="noopener">Blocking Client</a> ·
<a :href="withBase('/rustdoc/rs_snap7/struct.ClientConfig.html')" target="_blank" rel="noopener">ClientConfig</a> ·
<a :href="withBase('/rustdoc/rs_snap7/error/enum.Error.html')" target="_blank" rel="noopener">Error</a>

Version `0.1.0` is published. Generate Rustdoc directly from the checkout to
see every public method, type, parameter, and error contract:

```sh
# Run from the repository root.
cargo doc --no-deps --all-features --open
```

The entry page is `target/doc/rs_snap7/index.html`. Rustdoc remains the complete
API reference; this site provides task-oriented guides and a module map.

## Public API map

| Entry point | Purpose |
| --- | --- |
| `Client`, `ClientConfig` | Async connection, memory, diagnostics, control, blocks, and security |
| `ClientPool` | Bounded reads across independent connections |
| `blocking::{Client, ClientPool, Logo}` | Optional synchronous access to the same async core |
| `Logo`, `logo::VmAddress` | Checked LOGO! VM addresses and I/O |
| `Address`, `Area`, `WordLen` | Validated memory addresses and transport widths |
| `codec` | Checked scalar, string, bit, and date/time conversions |
| `tags::{Tag, Value}` | Typed addresses, values, and metadata |
| `db::{Db, DbConfig, Layout}` | Owned snapshots, layouts, row/field access, and dirty tracking |
| `policy`, `observability` | Recovery, limits, events, hooks, and operation statistics |
| `blocks`, `szl` | Block and diagnostic models |
| `protocol`, `transport` | Advanced S7 PDU codecs and raw ISO-on-TCP access |
| `Error`, `Result` | Structured validation, transport, PLC, and uncertain-outcome errors |

## Common client operations

| Task | Methods |
| --- | --- |
| Lifecycle | `connect`, `disconnect`, `reconnect`, `is_connected`, `pdu_length` |
| Raw DB memory | `db_read`, `db_write`, `db_get`, `db_fill` |
| Typed DB values | `db_read_int`, `db_write_int`, `db_read_real`, `db_write_real` |
| Bit access | `db_read_bool`, `db_write_bool` |
| Batch access | `read_multi_vars`, `write_multi_vars` |
| Typed tags | `read_tag`, `write_tag`, `read_tags` |
| Diagnostics | `get_cpu_state`, `get_cpu_info`, `get_order_code`, `read_szl` |
| Control and clock | `plc_stop`, `plc_hot_start`, `get_plc_datetime`, `set_plc_datetime` |
| Blocks | `list_blocks`, `get_block_info`, `upload`, `full_upload`, `download` |

See the [client source](https://github.com/PenguinKingGT/rs-snap7/blob/main/src/client/mod.rs),
[blocking facade](https://github.com/PenguinKingGT/rs-snap7/blob/main/src/blocking/mod.rs), and
[optional feature list](../guide/optional-products.md) for module-level entry points.

## Browse by Module

- [Client](client.md)
- [Server](server.md)
- [Partner](partner.md)
- [LOGO!](logo.md)
- [PPI](ppi.md)
- [Value Codecs](codec.md)
- [Tags](tags.md)
- [DB Snapshots](db.md)
- [Pools, Policies and Events](optimization.md)
- [Diagnostics and Blocks](diagnostics.md)
- [Discovery](discovery.md)
- [Types and Addresses](types.md)
- [Errors](errors.md)
- [Demo](demo.md)
