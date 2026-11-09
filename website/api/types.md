<script setup>
import { withBase } from 'vitepress';
</script>

# Types and Addresses API

Construct validated addresses instead of assembling numeric protocol fields. Sizes and units depend on the selected area and word length.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/types/struct.Address.html')" target="_blank" rel="noopener">Address</a> | Checked absolute area/DB/start/count address |
| <a :href="withBase('/rustdoc/rs_snap7/types/enum.Area.html')" target="_blank" rel="noopener">Area</a> | DB, input, output, marker, timer and counter areas |
| <a :href="withBase('/rustdoc/rs_snap7/types/enum.WordLen.html')" target="_blank" rel="noopener">WordLen</a> | Element representation and byte width |
| <a :href="withBase('/rustdoc/rs_snap7/types/enum.BlockType.html')" target="_blank" rel="noopener">BlockType</a> | S7 block identifiers |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/read-write.md).
