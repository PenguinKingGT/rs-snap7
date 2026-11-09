<script setup>
import { withBase } from 'vitepress';
</script>

# Partner API

Partner is an independent bidirectional PBC session, with its own transfer IDs, receive queue and lifecycle. Its async adapters use the Tokio blocking pool.

**Cargo feature:** `partner`.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/partner/struct.Partner.html')" target="_blank" rel="noopener">Partner</a> | Connect, send, receive, inspect statistics and stop |
| <a :href="withBase('/rustdoc/rs_snap7/partner/struct.PartnerListener.html')" target="_blank" rel="noopener">PartnerListener</a> | Bind and accept passive sessions |
| <a :href="withBase('/rustdoc/rs_snap7/partner/struct.PartnerConfig.html')" target="_blank" rel="noopener">PartnerConfig</a> | Transport, queue and callback settings |
| <a :href="withBase('/rustdoc/rs_snap7/partner/struct.Transfer.html')" target="_blank" rel="noopener">Transfer</a> | Received R-ID and payload |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/optional-products.md).
