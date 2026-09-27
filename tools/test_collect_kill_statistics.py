"""Run with python -m unittest discover -s tools -p 'test_*.py' -v."""
from datetime import datetime, timedelta, timezone
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import subprocess
import sys
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("collector", Path(__file__).parent / "collect-kill-statistics.py")
collector = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(collector)


class CollectorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "sessions"
        self.source.mkdir()
        self.output = self.root / "reports"
        self.now = datetime(2026, 9, 27, 12, tzinfo=timezone.utc)
        self.id = "a" * 32

    def snapshot(self, **changes):
        value = dict(SchemaVersion=1, SessionId=self.id, Character="Example", World="Coldeve",
                     ObjectId=1, StartedUtc=(self.now-timedelta(seconds=600)).isoformat(),
                     UpdatedUtc=self.now.isoformat(), EndedUtc=None, CoveredSeconds=600,
                     TotalKills=100, Closed=False, LastSequence=200)
        value.update(changes)
        return value

    def save(self, value):
        path = self.source / (value["SessionId"] + ".json")
        path.write_text(json.dumps(value), encoding="utf-8")
        return path

    def collect(self, now=None, expected=()):
        return collector.collect(self.source, self.output, expected, now or self.now)[0]

    def row(self, report):
        return report["characters"][0]

    def test_baseline_and_normal_delta(self):
        self.save(self.snapshot())
        first = self.row(self.collect())
        self.assertEqual("baseline", first["status"])
        self.assertEqual(600, first["cumulativeKillsPerHour"])
        self.assertIsNone(first["intervalKillsPerHour"])
        later = self.now + timedelta(minutes=20)
        self.save(self.snapshot(TotalKills=300, CoveredSeconds=1800, LastSequence=400, UpdatedUtc=later.isoformat()))
        second = self.row(self.collect(later))
        self.assertEqual(600, second["intervalKillsPerHour"])
        self.assertEqual(200, second["intervalKills"])
        self.assertEqual(1200, second["intervalCoveredSeconds"])
        self.assertEqual(2, len(list(self.output.glob("*/report.json"))))

    def test_new_segment_does_not_drop_final_old_segment_delta(self):
        self.save(self.snapshot())
        self.collect()
        ended = self.now + timedelta(minutes=5)
        self.save(self.snapshot(TotalKills=110, CoveredSeconds=900, UpdatedUtc=ended.isoformat(), EndedUtc=ended.isoformat(), Closed=True))
        later = self.now + timedelta(minutes=20)
        self.save(self.snapshot(SessionId="b"*32, StartedUtc=ended.isoformat(), UpdatedUtc=later.isoformat(), TotalKills=50, CoveredSeconds=900))
        row = self.row(self.collect(later))
        self.assertEqual(60, row["intervalKills"])
        self.assertEqual(1200, row["intervalCoveredSeconds"])
        self.assertEqual(180, row["intervalKillsPerHour"])
        self.assertEqual(160, row["totalKills"])

    def test_stale_and_missing_sources_are_gaps_not_zero(self):
        path = self.save(self.snapshot())
        self.collect()
        later = self.now + timedelta(minutes=20)
        row = self.row(self.collect(later))
        self.assertEqual("gap", row["status"])
        self.assertIsNone(row["intervalKillsPerHour"])
        path.unlink()
        row = self.row(self.collect(later + timedelta(minutes=20)))
        self.assertEqual("gap", row["status"])
        self.assertEqual(100, row["totalKills"])
        self.assertTrue(any("missing" in gap for gap in row["gaps"]))

    def test_closed_is_history_and_missing_expected_is_gap(self):
        self.save(self.snapshot(Closed=True, EndedUtc=self.now.isoformat()))
        self.collect()
        report = self.collect(self.now + timedelta(minutes=20), ["Example", "Absent"])
        rows = {row["character"]: row for row in report["characters"]}
        self.assertEqual("gap", rows["Example"]["status"])
        self.assertEqual("gap", rows["Absent"]["status"])
        self.assertIsNone(rows["Absent"]["totalKills"])
        self.assertIsNone(rows["Absent"]["intervalKillsPerHour"])
        self.assertEqual(600, rows["Example"]["cumulativeKillsPerHour"])

    def test_fresh_zero_kills_has_zero_rate_only_with_advancing_coverage(self):
        self.save(self.snapshot())
        self.collect()
        later = self.now + timedelta(minutes=20)
        self.save(self.snapshot(CoveredSeconds=1800, UpdatedUtc=later.isoformat()))
        row = self.row(self.collect(later))
        self.assertEqual(0, row["intervalKillsPerHour"])
        row = self.row(self.collect(later))
        self.assertIsNone(row["intervalKillsPerHour"])
        self.assertEqual("no-new-coverage", row["status"])

    def test_invalid_or_reversed_counter_retains_last_valid_state(self):
        self.save(self.snapshot())
        self.collect()
        for changes in [dict(TotalKills=-1), dict(CoveredSeconds=500), dict(TotalKills=99),
                        dict(SchemaVersion=2), dict(LastSequence=199), dict(Closed="false"),
                        dict(CoveredSeconds=float("nan")), dict(ObjectId=0), dict(Character="Changed")]:
            with self.subTest(changes=changes):
                self.save(self.snapshot(**changes))
                report = self.collect(self.now + timedelta(minutes=20))
                self.assertTrue(report["diagnostics"])
                self.assertEqual(100, self.row(report)["totalKills"])
                self.assertIsNone(self.row(report)["intervalKillsPerHour"])

    def test_new_older_segment_establishes_baseline_not_interval(self):
        self.collect()
        self.save(self.snapshot())
        row = self.row(self.collect(self.now + timedelta(minutes=1)))
        self.assertEqual(100, row["totalKills"])
        self.assertEqual(0, row["intervalCoveredSeconds"])
        self.assertTrue(row["newBaselines"])
        self.assertEqual("gap", row["status"])

    def test_crashed_old_segment_is_historical_after_replacement_baseline(self):
        self.save(self.snapshot())
        self.collect()
        restart = self.now + timedelta(minutes=10)
        later = self.now + timedelta(minutes=20)
        new_id = "b" * 32
        self.save(self.snapshot(SessionId=new_id, StartedUtc=restart.isoformat(),
                               UpdatedUtc=later.isoformat(), TotalKills=50, CoveredSeconds=600))
        first = self.row(self.collect(later))
        self.assertEqual("gap", first["status"])
        next_time = later + timedelta(minutes=20)
        self.save(self.snapshot(SessionId=new_id, StartedUtc=restart.isoformat(),
                               UpdatedUtc=next_time.isoformat(), TotalKills=150, CoveredSeconds=1800))
        second = self.row(self.collect(next_time))
        self.assertEqual("covered", second["status"])
        self.assertEqual(300, second["intervalKillsPerHour"])
        self.assertEqual(250, second["totalKills"])
        self.assertTrue(second["historicalGaps"])

    def test_offline_gap_between_closed_and_new_segment_is_disclosed(self):
        self.save(self.snapshot())
        self.collect()
        ended = self.now + timedelta(minutes=5)
        self.save(self.snapshot(Closed=True, EndedUtc=ended.isoformat(),
                               UpdatedUtc=ended.isoformat(), CoveredSeconds=900, TotalKills=120))
        started = self.now + timedelta(minutes=10)
        later = self.now + timedelta(minutes=20)
        self.save(self.snapshot(SessionId="b"*32, StartedUtc=started.isoformat(),
                               UpdatedUtc=later.isoformat(), TotalKills=50, CoveredSeconds=600))
        row = self.row(self.collect(later))
        self.assertEqual(900, row["intervalCoveredSeconds"])
        self.assertTrue(any("300.000s offline" in gap for gap in row["gaps"]))

    def test_recovery_after_missing_observation_establishes_new_baseline(self):
        path = self.save(self.snapshot())
        self.collect()
        path.unlink()
        self.collect(self.now + timedelta(minutes=20))
        later = self.now + timedelta(minutes=40)
        self.save(self.snapshot(UpdatedUtc=later.isoformat(), CoveredSeconds=3000, TotalKills=500))
        row = self.row(self.collect(later))
        self.assertEqual("gap", row["status"])
        self.assertEqual(500, row["totalKills"])
        self.assertIsNone(row["intervalKillsPerHour"])

    def test_overlapping_closed_segments_do_not_create_a_fabricated_rate(self):
        self.save(self.snapshot(Closed=True, EndedUtc=self.now.isoformat()))
        self.save(self.snapshot(SessionId="b"*32, Closed=True, EndedUtc=self.now.isoformat()))
        row = self.row(self.collect())
        self.assertIsNone(row["cumulativeKillsPerHour"])
        self.assertTrue(any("overlapping recorded" in gap for gap in row["gaps"]))

    def test_closed_and_open_overlap_invalidates_only_affected_intervals(self):
        self.save(self.snapshot())
        self.collect()
        ended = self.now + timedelta(minutes=5)
        later = self.now + timedelta(minutes=20)
        started = self.now + timedelta(minutes=2)
        self.save(self.snapshot(Closed=True, EndedUtc=ended.isoformat(), UpdatedUtc=ended.isoformat(), CoveredSeconds=900))
        self.save(self.snapshot(SessionId="b"*32, StartedUtc=started.isoformat(), UpdatedUtc=later.isoformat(), CoveredSeconds=1080))
        row = self.row(self.collect(later))
        self.assertIsNone(row["intervalKillsPerHour"])
        self.assertIsNone(row["cumulativeKillsPerHour"])
        self.assertTrue(any("overlapping recorded" in gap for gap in row["gaps"]))
        next_time = later + timedelta(minutes=20)
        self.save(self.snapshot(SessionId="b"*32, StartedUtc=started.isoformat(), UpdatedUtc=next_time.isoformat(), CoveredSeconds=2280, TotalKills=200))
        row = self.row(self.collect(next_time))
        self.assertEqual(300, row["intervalKillsPerHour"])
        self.assertIsNone(row["cumulativeKillsPerHour"])
        self.assertTrue(any("overlapping recorded" in gap for gap in row["historicalGaps"]))

    def test_atomic_generation_and_state_failure_preserves_previous_commit(self):
        source_path = self.save(self.snapshot())
        original_source = source_path.read_bytes()
        self.collect()
        original_state = (self.output / "latest.json").read_bytes()
        original_replace = collector.os.replace
        def fail_pointer(source, destination):
            if Path(destination).name == "latest.json":
                raise OSError("injected pointer failure")
            return original_replace(source, destination)
        with patch.object(collector.os, "replace", side_effect=fail_pointer):
            with self.assertRaises(OSError):
                self.collect(self.now + timedelta(seconds=30))
        self.assertEqual(original_state, (self.output / "latest.json").read_bytes())
        self.assertEqual(original_source, source_path.read_bytes())
        self.assertFalse((self.output / ".collecting").exists())
        self.assertFalse(list(self.output.glob(".pending-*")))
        self.assertFalse(list(self.output.glob(".latest-*")))
        state = json.loads(original_state)
        directory = self.output / state["generation"]
        self.assertEqual({"report.md", "report.json", "snapshots.json"}, {p.name for p in directory.iterdir()})

    def test_concurrent_lock_is_not_removed_and_nested_output_is_rejected(self):
        self.output.mkdir()
        lock = self.output / ".collecting"
        lock.mkdir()
        with self.assertRaises(FileExistsError):
            self.collect()
        self.assertTrue(lock.is_dir())
        with self.assertRaises(ValueError):
            collector.collect(self.source, self.source / "output", now=self.now)

    def test_source_change_or_damaged_state_fails_without_reset(self):
        self.save(self.snapshot())
        self.collect()
        latest = self.output / "latest.json"
        state = json.loads(latest.read_text())
        state["source"] = "different directory"
        latest.write_text(json.dumps(state))
        with self.assertRaises(ValueError):
            self.collect()
        self.assertEqual("different directory", json.loads(latest.read_text())["source"])

    def test_cli_commits_gap_report_without_copying_unknown_sensitive_fields(self):
        self.save(self.snapshot(Password="secret-never-copy"))
        result = subprocess.run([sys.executable, str(Path(collector.__file__)),
            "--sessions-dir", str(self.source), "--output-dir", str(self.output),
            "--expected", "Example"], capture_output=True, text=True, check=False)
        self.assertEqual(2, result.returncode)
        state = (self.output / "latest.json").read_text()
        self.assertNotIn("secret-never-copy", state + result.stdout + result.stderr)
        self.assertTrue(json.loads(state)["report"]["hasGaps"])

    def test_invalid_state_shape_is_rejected_without_replacing_it(self):
        self.save(self.snapshot())
        self.collect()
        path = self.output / "latest.json"
        original = json.loads(path.read_text())
        for damaged in ([], dict(original, sessions=[]), dict(original, gappedSessionIds=42)):
            with self.subTest(damaged=damaged):
                content = json.dumps(damaged)
                path.write_text(content)
                with self.assertRaises(ValueError):
                    self.collect()
                self.assertEqual(content, path.read_text())


if __name__ == "__main__":
    unittest.main()
