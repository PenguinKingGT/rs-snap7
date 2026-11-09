<script setup>
import { withBase } from 'vitepress';
</script>

# Diagnostics and Blocks API

Client service methods return these structured models. CPU support for individual SZLs, control services and block transfers must be checked on the target controller.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/szl/index.html')" target="_blank" rel="noopener">szl</a> | CPU/CP/order/protection and diagnostic parsers |
| <a :href="withBase('/rustdoc/rs_snap7/types/enum.CpuState.html')" target="_blank" rel="noopener">CpuState</a> | Decoded CPU state |
| <a :href="withBase('/rustdoc/rs_snap7/blocks/struct.BlockInfo.html')" target="_blank" rel="noopener">BlockInfo</a> | Validated remote block metadata |
| <a :href="withBase('/rustdoc/rs_snap7/blocks/struct.CompactBlockInfo.html')" target="_blank" rel="noopener">CompactBlockInfo</a> | Offline compact header decoding |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/diagnostics.md).
