# Diagnostics and Blocks

Examples assume a connected async `client` in a function returning
`rs_snap7::Result<()>`. Use the local emulator for software exploration; real
PLC service availability depends on its model, settings, and permissions.

## Read PLC information

```rust
let state = client.get_cpu_state().await?;
let cpu = client.get_cpu_info().await?;
let order = client.get_order_code().await?;
let clock = client.get_plc_datetime().await?;
println!("{state:?}: {cpu:?}, {order:?}, {clock:?}");
```

`read_szl` supports bounded continuation handling. `get_cp_info`,
`get_protection`, and `read_diagnostic_buffer` decode additional service models.

## List and upload blocks

```rust
use rs_snap7::BlockType;

let blocks = client.list_blocks().await?;
let info = client.get_block_info(BlockType::Db, 1).await?;
let payload = client.upload(1).await?;
let complete = client.full_upload(BlockType::Db, 1).await?;
```

`upload` returns DB bytes after removing the compact header; `full_upload`
returns the complete block representation. `download(data, Some(number))`
accepts a **DB representation**, not an arbitrary memory-write buffer. `None`
extracts its DB number from bytes 6–7; short data requires an explicit number.
Use `db_write` for ordinary byte writes.

## Control and clock operations

`plc_stop`, `plc_hot_start`, `plc_cold_start`, `compress`, `copy_ram_to_rom`,
`set_plc_datetime`, and block deletion/download modify remote state. Invoke them
only when your application intends that change. They are never silently replayed
after a lost acknowledgement.

`DateTime` has no timezone. `set_plc_system_datetime()` sends UTC; pass an explicit
`DateTime` when the PLC should follow a different local-time convention.

`force_bit`/`cancel_force` modify process-image bits and do not establish persistent
CPU forces. See the [complete API](../api/index.md) for signatures and error contracts.
