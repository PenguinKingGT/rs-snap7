<script setup>
import { withBase } from 'vitepress';
</script>

# LOGO! API

LOGO! adapts validated VM addresses to DB1 operations. Use controller-specific TSAP settings; VW/VD values are signed.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/logo/struct.Logo.html')" target="_blank" rel="noopener">Logo</a> | Async VM reads/writes and explicit TSAP connections |
| <a :href="withBase('/rustdoc/rs_snap7/logo/struct.VmAddress.html')" target="_blank" rel="noopener">VmAddress</a> | Parse V10, VW20, VD30 and V10.3 |
| <a :href="withBase('/rustdoc/rs_snap7/blocking/struct.Logo.html')" target="_blank" rel="noopener">blocking::Logo</a> | Optional synchronous VM operations |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/optional-products.md).
