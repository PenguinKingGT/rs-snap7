<script setup>
import { withBase } from 'vitepress';
</script>

# Server API

Create registered memory and exercise clients locally. The emulator models supported services and does not execute PLC programs.

**Cargo feature:** `server`.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/server/struct.Server.html')" target="_blank" rel="noopener">Server</a> | Start, register areas, inspect events and stop |
| <a :href="withBase('/rustdoc/rs_snap7/server/struct.ServerConfig.html')" target="_blank" rel="noopener">ServerConfig</a> | Bind address and bounded session/PDU settings |
| <a :href="withBase('/rustdoc/rs_snap7/server/struct.Memory.html')" target="_blank" rel="noopener">Memory</a> | Snapshot, mutate and protect registered memory |
| <a :href="withBase('/rustdoc/rs_snap7/server/struct.Event.html')" target="_blank" rel="noopener">Event</a> | Structured emulator events |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/server.md).
