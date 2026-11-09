<script setup>
import { withBase } from 'vitepress';
</script>

# PPI API

Experimental serial S7-200 access uses a mutable single-master client and element-aware area operations. It is separate from Ethernet rack/slot connections.

**Cargo feature:** `ppi`.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/ppi/struct.PpiClient.html')" target="_blank" rel="noopener">PpiClient</a> | Open/connect, read/write areas and V memory |
| <a :href="withBase('/rustdoc/rs_snap7/ppi/struct.PpiConfig.html')" target="_blank" rel="noopener">PpiConfig</a> | Station, timeouts and retry settings |
| <a :href="withBase('/rustdoc/rs_snap7/ppi/struct.PpiTransport.html')" target="_blank" rel="noopener">PpiTransport</a> | Checked serial exchanges over an injected port |
| <a :href="withBase('/rustdoc/rs_snap7/ppi/enum.PpiArea.html')" target="_blank" rel="noopener">PpiArea</a> | Supported serial memory areas |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/optional-products.md).
