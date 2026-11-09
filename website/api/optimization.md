<script setup>
import { withBase } from 'vitepress';
</script>

# Pools, Policies and Events API

Use independent sessions for bounded parallel reads. Configure wire request limits and recovery before connecting; inspect logical operation metrics without network I/O.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/client/struct.ClientPool.html')" target="_blank" rel="noopener">ClientPool</a> | Bounded parallel reads; 1–32 independent sessions |
| <a :href="withBase('/rustdoc/rs_snap7/policy/struct.RateLimit.html')" target="_blank" rel="noopener">RateLimit</a> | Fixed spacing or token-bucket limits |
| <a :href="withBase('/rustdoc/rs_snap7/policy/struct.Recovery.html')" target="_blank" rel="noopener">Recovery</a> | Read recovery, backoff and heartbeat |
| <a :href="withBase('/rustdoc/rs_snap7/observability/struct.OperationHook.html')" target="_blank" rel="noopener">OperationHook</a> | Logical operation callbacks |
| <a :href="withBase('/rustdoc/rs_snap7/observability/struct.OperationStats.html')" target="_blank" rel="noopener">OperationStats</a> | Completion, failure and callback statistics |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/multi-variable.md).
