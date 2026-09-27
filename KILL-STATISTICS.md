# Personal kill statistics

Recording is **off by default**. In MossTank's plugin storage (`files` beneath
its plugin directory), create `kill-statistics/enabled.txt` containing `true`,
then restart the client through its launcher. Delete the flag or set it to
`false` and restart to disable recording. The flag is read only at plugin
startup; this is independent of macro profiles and macro enable/disable.

The recorder counts delivered personal killing-blow chat notices in the default
log-text class, using the existing combat-result classifier. Cleaves and attacks
that kill multiple creatures count each delivered notice, regardless of selected
target. It does not infer kills from nearby deaths, pet damage, shared fellowship
experience, or experience totals. Chat suppressed before delivery is not counted.
Identical or older chat sequence numbers are ignored. There is no transcript
polling or per-kill disk write.

## Snapshot schema (version 1)

Each observed in-world character session writes a unique
`kill-statistics/sessions/<SessionId>.json` through atomic plugin storage.
A plugin restart always starts a new GUID segment; it never overwrites another
process's counters. A character/world/object ID change or observed logout closes
the current segment. The recorder checks identity on ticks and delivered chat.
An unobserved disconnect/reconnect between those callbacks cannot be distinguished.

| Field | Meaning |
|---|---|
| SchemaVersion | `1` |
| SessionId | Unique segment identifier, also the filename |
| Character, World, ObjectId | Character identity; no account credentials |
| StartedUtc | Time recording this segment began, not account login time |
| UpdatedUtc | Last successful snapshot's recording time |
| EndedUtc | End time when closed, otherwise null |
| CoveredSeconds | Monotonic elapsed time recorded in this segment |
| TotalKills | Cumulative personal kill notices in this segment |
| Closed | True after observed logout, identity change or plugin disposal |
| LastSequence | Highest delivered chat sequence observed by this recorder |

A snapshot is written at start, at most once per minute during normal operation,
and on lifecycle closure. Idle but connected time counts in CoveredSeconds; this
measures kills per hour online, not only seconds spent attacking. UTC is for
correlation; elapsed coverage uses a monotonic clock. Graceful shutdown flushes
the final counter and removes the chat subscription. Abrupt termination can lose
up to a minute of observations. Recording/storage failure logs an explicit error
and disables the recorder for that process, leaving the last durable snapshot.
It never retries disk writes on every tick or interferes with macro actions.

## Collecting every 20 minutes

An external collector should save the snapshot observations every 20 minutes,
keyed by SessionId and grouped by World/ObjectId/Character. For each segment,
subtract previous TotalKills and CoveredSeconds. Report interval kills/hour as
`3600 * sum(kill deltas) / sum(covered-second deltas)`; report cumulative rates
from cumulative counters and coverage. Keep new/closed segments separate until
aggregation. A first observation establishes a baseline for interval reports;
its cumulative values remain useful for the session-to-date rate.

Check UpdatedUtc and the live client session before interpreting a snapshot.
A still-open snapshot older than two minutes is stale, not evidence of zero
kills. Report missing/stale/failed recording as a measurement gap, not zero
activity; show covered seconds alongside each rate. Do not extrapolate a stale
snapshot to the collection wall-clock time. After a crash or storage failure,
only coverage up to the last durable snapshot is supported. Closed snapshots
remain historical evidence rather than a currently running session. A zero
rate is meaningful only when fresh covered time advanced and TotalKills did not.
