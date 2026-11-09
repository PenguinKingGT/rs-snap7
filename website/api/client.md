<script setup>
import { withBase } from 'vitepress';
</script>

# Client API

Connect once, share clones for serialized operations, and use independent sessions for concurrency. Network methods on the default Client are awaited.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/client/struct.Client.html')" target="_blank" rel="noopener">Client</a> | Async lifecycle, memory, tags, diagnostics and control |
| <a :href="withBase('/rustdoc/rs_snap7/struct.ClientConfig.html')" target="_blank" rel="noopener">ClientConfig</a> | Configure endpoint, PDU, recovery, limits and hooks |
| <a :href="withBase('/rustdoc/rs_snap7/blocking/struct.Client.html')" target="_blank" rel="noopener">blocking::Client</a> | Synchronous facade; requires blocking |
| <a :href="withBase('/rustdoc/rs_snap7/transport/struct.TransportConfig.html')" target="_blank" rel="noopener">TransportConfig</a> | Timeouts, explicit TSAPs and gateway routing |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/configuration.md).
