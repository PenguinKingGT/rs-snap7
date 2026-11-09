<script setup>
import { withBase } from 'vitepress';
</script>

# Discovery API

PROFINET DCP identifies stations over Ethernet independently of S7 TCP sessions. Built-in raw Ethernet supports Linux/macOS; applications can supply FrameIo.

**Cargo feature:** `discovery`.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/discovery/index.html')" target="_blank" rel="noopener">discovery</a> | Identify all stations or a selected MAC |
| <a :href="withBase('/rustdoc/rs_snap7/discovery/struct.Device.html')" target="_blank" rel="noopener">Device</a> | Advertised name, addresses and vendor metadata |
| <a :href="withBase('/rustdoc/rs_snap7/discovery/struct.DiscoveryConfig.html')" target="_blank" rel="noopener">DiscoveryConfig</a> | Bound collection time, frames and device count |
| <a :href="withBase('/rustdoc/rs_snap7/discovery/trait.FrameIo.html')" target="_blank" rel="noopener">FrameIo</a> | Application-owned Ethernet adapter seam |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/optional-products.md).
