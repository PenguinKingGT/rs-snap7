<script setup>
import { withBase } from 'vitepress';
</script>

# Value Codecs API

Decode local byte buffers without opening a PLC connection. Offsets and lengths are checked, and multibyte values use PLC byte order.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/codec/index.html')" target="_blank" rel="noopener">codec</a> | Integer, float, bit, character and string helpers |
| <a :href="withBase('/rustdoc/rs_snap7/codec/trait.Scalar.html')" target="_blank" rel="noopener">Scalar</a> | Fixed-width checked scalar conversion |
| <a :href="withBase('/rustdoc/rs_snap7/codec/fn.read.html')" target="_blank" rel="noopener">read / write</a> | Generic typed access to a byte slice |
| <a :href="withBase('/rustdoc/rs_snap7/codec/datetime/index.html')" target="_blank" rel="noopener">datetime</a> | S7 date/time models and conversions |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/read-write.md).
