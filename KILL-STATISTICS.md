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
the current segment. The recorder also listens to login/logoff lifecycle events,
so reconnects of the same character create separate segments even between ticks.
After logoff it stays closed while teardown still exposes the old character;
login completion or an observed out-of-world/in-world transition permits reopening.
Identity is checked on ticks and delivered chat as a fallback for hosts with inert
lifecycle events. Such hosts cannot distinguish a reconnect between callbacks.

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

## Collector command

`tools/collect-kill-statistics.py` uses Python 3.10+ and only the standard library.
It performs one observation per invocation; scheduling remains external. For example:

```powershell
python tools/collect-kill-statistics.py `
  --sessions-dir "$env:LOCALAPPDATA\OpenAC\plugins\acdream.mosstank\files\kill-statistics\sessions" `
  --output-dir "$env:LOCALAPPDATA\OpenAC\diagnostics\kill-history" `
  --expected "Character One" --expected "Character Two"
```

Use the actual plugin storage location reported by your installation. Source and
output directories must be separate and not nested. The collector never changes
source files. Repeat `--expected` with exact character names to report missing
characters, including an expected character whose only evidence is a closed
session. Identity grouping also includes world and object ID.

Each invocation creates a unique history directory with `snapshots.json` (only
validated source fields), `report.json`, and a human-readable `report.md`.
The entire directory is staged before publication; `latest.json` is atomically
replaced last and identifies the committed generation and durable collector
state. Follow its `generation` property to the latest Markdown report. A failed
publication leaves the previous `latest.json` intact; an unreferenced complete
generation is not a committed observation. Files are flushed before replacement.
The collector does not promise survival against every filesystem/power failure.

Exit codes: **0** = observation committed, **2** = observation committed with
measurement gaps, **1** = collection failed without committing new state.
Invalid or unreadable snapshots produce diagnostics without copying their raw
content. Invalid schema, negative/nonfinite counters, identity changes, reversed
counters, and mutation of a closed session are rejected; the last valid counter
is retained. A recovered previously missing/stale/invalid segment establishes a
new interval baseline. Its cumulative rate remains available.

A new segment beginning after the previous collection contributes its recorded
coverage and kills; a newly discovered older segment establishes a baseline.
Known offline gaps between closed and replacement segments are disclosed as
partial intervals. A crashed, superseded open segment remains cumulative
history with a historical warning. After the replacement has established a
baseline, that old segment no longer prevents valid subsequent interval rates.
Fresh zero-kill intervals report zero only when covered time advanced. Empty
coverage, missing characters, and uncertain intervals display an unavailable
interval rate. Historical rates describe recorded coverage, not continuous
wall-clock uptime.

Overlapping recorded segments are flagged even when both have already closed.
The collector does not guess which kills or elapsed seconds to deduplicate:
cumulative rates are unavailable for overlapping history, and interval rates
are unavailable while the overlap affects that interval. Later clean intervals
can still report a rate, with the historical overlap warning retained.

Only one collector may write an output directory at a time. The `.collecting`
directory is an exclusive lock. A crash can leave it behind: verify no collector
is running before removing that directory and running again. The tool never
automatically removes another invocation's lock. A source-directory change or
damaged state requires a separate output directory or explicit operator repair;
it never silently resets prior measurements.

Tests: `python -m unittest discover -s tools -p 'test_*.py' -v`.
