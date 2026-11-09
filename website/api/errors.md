<script setup>
import { withBase } from 'vitepress';
</script>

# Errors API

Match structured errors to preserve PLC codes and acknowledged write progress. Error is non-exhaustive; keep a fallback match arm.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/error/enum.Error.html')" target="_blank" rel="noopener">Error</a> | Validation, communication, PLC and uncertain outcomes |
| <a :href="withBase('/rustdoc/rs_snap7/error/type.Result.html')" target="_blank" rel="noopener">Result</a> | Library result alias |
| <a :href="withBase('/rustdoc/rs_snap7/error/fn.item_error_text.html')" target="_blank" rel="noopener">item_error_text</a> | Describe a PLC item return code |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/errors-and-limits.md).
