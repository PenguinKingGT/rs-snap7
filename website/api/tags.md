<script setup>
import { withBase } from 'vitepress';
</script>

# Tags API

Tags keep address syntax, data type and array count together. Dynamic values preserve signedness and width. Classic S7 I/O still requires absolute addresses.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/tags/struct.Tag.html')" target="_blank" rel="noopener">Tag</a> | Parse addresses and encode/decode checked values |
| <a :href="withBase('/rustdoc/rs_snap7/tags/enum.Value.html')" target="_blank" rel="noopener">Value</a> | Typed dynamic values and arrays |
| <a :href="withBase('/rustdoc/rs_snap7/tags/enum.DataType.html')" target="_blank" rel="noopener">DataType</a> | Supported tag data types |
| <a :href="withBase('/rustdoc/rs_snap7/tags/import/index.html')" target="_blank" rel="noopener">import</a> | CSV/JSON/TIA XML text; requires tag-import |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/data-tools.md).
