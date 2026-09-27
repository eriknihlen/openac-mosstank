#!/usr/bin/env python3
"""Observe MossTank's durable counters; never modify plugin storage."""
import argparse
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import re
import shutil
import sys
import uuid
from itertools import combinations

FIELDS = {"SchemaVersion", "SessionId", "Character", "World", "ObjectId", "StartedUtc",
          "UpdatedUtc", "EndedUtc", "CoveredSeconds", "TotalKills", "Closed", "LastSequence"}


def timestamp(value):
    if not isinstance(value, str):
        raise ValueError("timestamp must be a string")
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("timestamp must include UTC offset")
    return parsed.astimezone(timezone.utc)


def validate(value, session_id):
    if not isinstance(value, dict) or set(value) != FIELDS:
        raise ValueError("unexpected snapshot fields")
    if type(value["SchemaVersion"]) is not int or value["SchemaVersion"] != 1:
        raise ValueError("unsupported schema")
    if not re.fullmatch(r"[0-9a-f]{32}", session_id) or value["SessionId"] != session_id:
        raise ValueError("session identity does not match filename")
    for field in ("Character", "World"):
        if not isinstance(value[field], str) or (field == "Character" and not value[field]):
            raise ValueError("invalid character/world")
    for field in ("ObjectId", "TotalKills", "LastSequence"):
        if type(value[field]) is not int or value[field] < 0:
            raise ValueError("negative or noninteger counter")
    if not 0 < value["ObjectId"] <= 0xffffffff or value["LastSequence"] > 0xffffffffffffffff:
        raise ValueError("identity/sequence out of range")
    if value["TotalKills"] > 0x7fffffffffffffff:
        raise ValueError("kill counter out of range")
    seconds = value["CoveredSeconds"]
    if type(seconds) not in (int, float) or not math.isfinite(seconds) or seconds < 0:
        raise ValueError("invalid covered seconds")
    if type(value["Closed"]) is not bool:
        raise ValueError("invalid closed flag")
    timestamp(value["StartedUtc"])
    timestamp(value["UpdatedUtc"])
    if value["Closed"]:
        if timestamp(value["EndedUtc"]) != timestamp(value["UpdatedUtc"]):
            raise ValueError("closed timestamps disagree")
    elif value["EndedUtc"] is not None:
        raise ValueError("open session has end timestamp")
    return value


def identity(snapshot):
    return snapshot["World"], snapshot["ObjectId"], snapshot["Character"]


def validate_progress(old, new):
    if identity(old) != identity(new) or old["StartedUtc"] != new["StartedUtc"]:
        raise ValueError("session identity changed")
    if any(new[key] < old[key] for key in ("TotalKills", "CoveredSeconds", "LastSequence")):
        raise ValueError("counters reversed/reset within session")
    if old["Closed"] and new != old:
        raise ValueError("closed session changed")
    if new["CoveredSeconds"] == old["CoveredSeconds"] and new["TotalKills"] != old["TotalKills"]:
        raise ValueError("kills changed without covered time advancing")


def rate(kills, seconds):
    return 3600 * kills / seconds if seconds > 0 else None


def observe(sessions_dir, previous, now, expected=()):
    """Return report and last valid cumulative state. Missing data remains a gap."""
    prior = previous.get("sessions", {}) if previous else {}
    previous_time = timestamp(previous["collectedUtc"]) if previous else None
    if previous_time and now < previous_time:
        raise ValueError("collection clock moved backwards")
    retained = dict(prior)
    current = {}
    errors = []
    bad_ids = set()
    gapped_ids = set()
    paths = sorted(sessions_dir.glob("*.json"))
    for path in paths:
        try:
            if path.stat().st_size > 65536:
                raise ValueError("snapshot exceeds 64 KiB")
            snapshot = validate(json.loads(path.read_text(encoding="utf-8-sig")), path.stem)
            if path.stem in prior:
                validate_progress(prior[path.stem], snapshot)
            current[path.stem] = snapshot
            retained[path.stem] = snapshot
        except (OSError, ValueError, TypeError, OverflowError):
            # Do not echo arbitrary source JSON or parser excerpts into reports.
            bad_ids.add(path.stem)
            gapped_ids.add(path.stem)
            errors.append({"sessionId": path.stem, "problem": "invalid/unreadable snapshot or reversed counters"})

    rows = {}
    def row_for(snapshot):
        key = identity(snapshot)
        if key not in rows:
            rows[key] = {"world": key[0], "objectId": key[1], "character": key[2],
                         "totalKills": 0, "coveredSeconds": 0, "intervalKills": 0,
                         "intervalCoveredSeconds": 0, "freshOpenSessions": 0,
                         "cumulativeReliable": True,
                         "sessionIds": [], "gaps": [], "historicalGaps": [], "newBaselines": []}
        return rows[key]

    for session_id, snapshot in retained.items():
        row = row_for(snapshot)
        row["sessionIds"].append(session_id)
        row["totalKills"] += snapshot["TotalKills"]
        row["coveredSeconds"] += snapshot["CoveredSeconds"]
        # A dead recorder's last open snapshot stays useful history. Once a
        # replacement was observed at the previous baseline it must not poison
        # all future intervals. Never retire a snapshot that is still advancing.
        historical_open = (not snapshot["Closed"] and previous_time is not None and
            prior.get(session_id) == snapshot and any(
                other_id != session_id and other_id in prior and identity(other) == identity(snapshot)
                and timestamp(other["StartedUtc"]) > timestamp(snapshot["UpdatedUtc"])
                and timestamp(other["StartedUtc"]) <= previous_time
                for other_id, other in retained.items()))
        if historical_open and (session_id not in current or
                (now - timestamp(snapshot["UpdatedUtc"])).total_seconds() > 120):
            row["historicalGaps"].append("incomplete superseded session: " + session_id)
            continue
        if session_id in bad_ids:
            row["gaps"].append("invalid snapshot: " + session_id)
            continue
        if session_id not in current:
            if not snapshot["Closed"]:
                row["gaps"].append("missing open session: " + session_id)
                gapped_ids.add(session_id)
            continue
        age = (now - timestamp(snapshot["UpdatedUtc"])).total_seconds()
        if age < -120:
            row["gaps"].append("future snapshot timestamp: " + session_id)
            gapped_ids.add(session_id)
        elif not snapshot["Closed"]:
            if age > 120:
                row["gaps"].append("stale/future open session: " + session_id)
                gapped_ids.add(session_id)
            else:
                row["freshOpenSessions"] += 1
        if previous_time is None:
            row["newBaselines"].append(session_id)
        elif session_id in previous.get("gappedSessionIds", []):
            row["newBaselines"].append(session_id)
            row["gaps"].append("recovered session requires fresh interval baseline: " + session_id)
        elif session_id in prior:
            row["intervalKills"] += snapshot["TotalKills"] - prior[session_id]["TotalKills"]
            row["intervalCoveredSeconds"] += snapshot["CoveredSeconds"] - prior[session_id]["CoveredSeconds"]
        elif timestamp(snapshot["StartedUtc"]) >= previous_time:
            row["intervalKills"] += snapshot["TotalKills"]
            row["intervalCoveredSeconds"] += snapshot["CoveredSeconds"]
        else:
            row["newBaselines"].append(session_id)
            row["gaps"].append("newly discovered older session requires baseline: " + session_id)

    if previous_time:
        for snapshot in current.values():
            started = timestamp(snapshot["StartedUtc"])
            if not previous_time < started <= now:
                continue
            ended = [timestamp(other["EndedUtc"]) for other in retained.values()
                     if identity(other) == identity(snapshot) and other["Closed"]
                     and timestamp(other["EndedUtc"]) <= started]
            if ended:
                uncovered = (started - max(previous_time, max(ended))).total_seconds()
                if uncovered > 0:
                    row_for(snapshot)["gaps"].append(f"partial interval: {uncovered:.3f}s offline between sessions")

    for first, second in combinations(retained.values(), 2):
        if identity(first) != identity(second):
            continue
        overlap_start = max(timestamp(first["StartedUtc"]), timestamp(second["StartedUtc"]))
        overlap_end = min(timestamp(first["UpdatedUtc"]), timestamp(second["UpdatedUtc"]))
        if overlap_end <= overlap_start:
            continue
        row = row_for(first)
        row["cumulativeReliable"] = False
        problem = "overlapping recorded coverage: " + first["SessionId"] + "/" + second["SessionId"]
        if previous_time is None or min(overlap_end, now) > max(overlap_start, previous_time):
            row["gaps"].append(problem)
        else:
            row["historicalGaps"].append(problem)

    for name in expected:
        matching = [row for row in rows.values() if row["character"] == name]
        if not matching:
            rows[("", 0, name)] = {"world": "", "objectId": None, "character": name,
                "totalKills": None, "coveredSeconds": 0, "intervalKills": None,
                "intervalCoveredSeconds": 0, "freshOpenSessions": 0,
                "cumulativeReliable": True,
                "sessionIds": [], "gaps": ["expected character missing"], "historicalGaps": [], "newBaselines": []}
        elif not any(row["freshOpenSessions"] for row in matching):
            for row in matching:
                row["gaps"].append("expected character has no fresh open session")
    for row in rows.values():
        if row["freshOpenSessions"] > 1:
            row["gaps"].append("overlapping fresh sessions for character")
        row["cumulativeKillsPerHour"] = (rate(row["totalKills"], row["coveredSeconds"])
                                         if row["cumulativeReliable"] else None)
        row["intervalKillsPerHour"] = (rate(row["intervalKills"], row["intervalCoveredSeconds"])
                                             if previous and not row["gaps"] else None)
        row["status"] = ("gap" if row["gaps"] else "baseline" if not previous else
                         "covered" if row["intervalCoveredSeconds"] > 0 else
                         "no-new-coverage" if row["freshOpenSessions"] else "closed")
    report = {"schemaVersion": 1, "collectedUtc": now.isoformat(),
              "previousCollectedUtc": previous_time.isoformat() if previous_time else None,
              "characters": sorted(rows.values(), key=lambda r: (r["world"], r["character"], r["objectId"] or 0)),
              "diagnostics": errors, "hasGaps": bool(errors) or any(r["gaps"] for r in rows.values())}
    state = {"schemaVersion": 1, "collectedUtc": now.isoformat(), "sessions": retained,
             "gappedSessionIds": sorted(gapped_ids)}
    return report, state, current


def markdown(report):
    def clean(value):
        return str(value).replace("|", "\\|").replace("\n", " ").replace("\r", " ")
    def number(value):
        return "—" if value is None else f"{value:.2f}"
    lines = ["# Personal kills/hour", "", "Collected: " + report["collectedUtc"], "",
             "Rates use recorded covered time; gaps are not zero kills. Cumulative rates include retained history.", "",
             "| Character | World | Status | Total kills | Covered hours | Cumulative kills/h | Interval kills | Interval covered minutes | Interval kills/h |",
             "|---|---|---|---:|---:|---:|---:|---:|---:|"]
    gaps = []
    for row in report["characters"]:
        lines.append("| " + " | ".join([clean(row["character"]), clean(row["world"]), row["status"],
            str(row["totalKills"]) if row["totalKills"] is not None else "—",
            number(row["coveredSeconds"] / 3600), number(row["cumulativeKillsPerHour"]),
            str(row["intervalKills"]) if row["intervalKills"] is not None else "—",
            number(row["intervalCoveredSeconds"] / 60), number(row["intervalKillsPerHour"])]) + " |")
        for gap in row["gaps"]:
            gaps.append("Gap for " + clean(row["character"]) + ": " + clean(gap))
        for gap in row["historicalGaps"]:
            gaps.append("Historical gap for " + clean(row["character"]) + ": " + clean(gap))
    if gaps:
        lines += [""] + gaps
    if report["diagnostics"]:
        lines += ["", "Invalid/unreadable source snapshots: " + str(len(report["diagnostics"])) + ". See report.json diagnostics."]
    return "\n".join(lines) + "\n"


def write_json(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())


def collect(sessions_dir, output_dir, expected=(), now=None):
    sessions_dir = Path(sessions_dir).resolve(strict=True)
    output_dir = Path(output_dir).resolve()
    if not sessions_dir.is_dir():
        raise ValueError("sessions directory does not exist")
    if output_dir == sessions_dir or sessions_dir in output_dir.parents or output_dir in sessions_dir.parents:
        raise ValueError("source and output directories must be separate, not nested")
    output_dir.mkdir(parents=True, exist_ok=True)
    lock = output_dir / ".collecting"
    lock.mkdir()  # Exclusive single writer; never remove another invocation's lock.
    pending = None
    pointer_temp = None
    try:
        latest = output_dir / "latest.json"
        previous = None
        if latest.exists():
            previous = json.loads(latest.read_text(encoding="utf-8"))
            if (not isinstance(previous, dict) or type(previous.get("schemaVersion")) is not int
                    or previous["schemaVersion"] != 1 or previous.get("source") != str(sessions_dir)
                    or not isinstance(previous.get("sessions"), dict)
                    or not isinstance(previous.get("gappedSessionIds"), list)
                    or any(not isinstance(item, str) for item in previous["gappedSessionIds"])):
                raise ValueError("collector state schema/source mismatch")
            timestamp(previous["collectedUtc"])
            for session_id, snapshot in previous["sessions"].items():
                validate(snapshot, session_id)
        report, state, snapshots = observe(sessions_dir, previous, now or datetime.now(timezone.utc), expected)
        generation = uuid.uuid4().hex
        pending = output_dir / (".pending-" + generation)
        pending.mkdir()
        write_json(pending / "snapshots.json", snapshots)
        write_json(pending / "report.json", report)
        with (pending / "report.md").open("x", encoding="utf-8", newline="\n") as stream:
            stream.write(markdown(report))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(pending, output_dir / generation)
        pending = None
        state.update(source=str(sessions_dir), generation=generation, report=report)
        pointer_temp = output_dir / (".latest-" + generation + ".tmp")
        write_json(pointer_temp, state)
        os.replace(pointer_temp, latest)
        pointer_temp = None
        return report, output_dir / generation / "report.md"
    finally:
        if pending is not None and pending.exists():
            shutil.rmtree(pending)
        if pointer_temp is not None and pointer_temp.exists():
            pointer_temp.unlink()
        lock.rmdir()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sessions-dir", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--expected", action="append", default=[], metavar="CHARACTER")
    args = parser.parse_args()
    try:
        report, path = collect(args.sessions_dir, args.output_dir, args.expected)
    except (OSError, ValueError, KeyError, TypeError):
        print("Collection failed: invalid/unavailable source, output, or collector state. No successful observation committed.", file=sys.stderr)
        return 1
    print(path)
    print("Measurement gaps present." if report["hasGaps"] else "Observation collected.")
    return 2 if report["hasGaps"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
