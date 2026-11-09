<script setup>
import { withBase } from 'vitepress';
</script>

# DB Snapshots API

An owned snapshot retains editable bytes and an acknowledged baseline. Row/field I/O updates the same snapshot and checkpoints successful write ranges.

## Entry Points

| API | Purpose |
| --- | --- |
| <a :href="withBase('/rustdoc/rs_snap7/db/struct.Db.html')" target="_blank" rel="noopener">Db</a> | Rows, fields, export and dirty-state I/O |
| <a :href="withBase('/rustdoc/rs_snap7/db/struct.DbConfig.html')" target="_blank" rel="noopener">DbConfig</a> | Physical row placement, gaps and writable prefix |
| <a :href="withBase('/rustdoc/rs_snap7/db/struct.Layout.html')" target="_blank" rel="noopener">Layout</a> | Parse exact byte/bit layout specifications |

Open an entry for signatures, parameters, error contracts and source generated from the current implementation.

Tutorial: [Read the related guide](../guide/data-tools.md).
