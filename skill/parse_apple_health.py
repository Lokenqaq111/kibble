#!/usr/bin/env python3
"""Apple Health export → per-day aggregated CSV + per-workout CSV + summary MD.

Streaming parser (handles 700 MB+ XML on a laptop). Excludes location data
(workout-routes/*.gpx) by design — never read, never copied. Copies ECG CSVs
verbatim because they are small and clinically useful.

Usage:
    python3 parse_apple_health.py <export_dir> <repo_dir>

Outputs into <repo_dir>/health/:
    daily.csv               wide table, one row per day
    workouts.csv            one row per workout
    ecg/<file>.csv          copies of electrocardiograms/*.csv
    raw_record_counts.md    audit: what was found and counts
    README.md               schema documentation
"""

import csv
import os
import shutil
import sys
import xml.etree.ElementTree as ET
from collections import defaultdict
from datetime import datetime

# HK record type → (csv column name, aggregation strategy)
DAILY_METRICS = [
    ("HKQuantityTypeIdentifierStepCount",               "steps",         "sum"),
    ("HKQuantityTypeIdentifierActiveEnergyBurned",      "active_kcal",   "sum"),
    ("HKQuantityTypeIdentifierBasalEnergyBurned",       "basal_kcal",    "sum"),
    ("HKQuantityTypeIdentifierAppleExerciseTime",       "exercise_min",  "sum"),
    ("HKQuantityTypeIdentifierAppleStandTime",          "stand_min",     "sum"),
    ("HKQuantityTypeIdentifierDistanceWalkingRunning",  "walk_run_km",   "sum"),
    ("HKQuantityTypeIdentifierDistanceCycling",         "cycle_km",      "sum"),
    ("HKQuantityTypeIdentifierFlightsClimbed",          "flights",       "sum"),
    ("HKQuantityTypeIdentifierRestingHeartRate",        "resting_hr",    "avg"),
    ("HKQuantityTypeIdentifierHeartRate",               "avg_hr",        "avg"),
    ("HKQuantityTypeIdentifierHeartRateVariabilitySDNN","hrv_ms",        "avg"),
    ("HKQuantityTypeIdentifierOxygenSaturation",        "spo2_pct",      "avg"),
    ("HKQuantityTypeIdentifierRespiratoryRate",         "resp_rate",     "avg"),
    ("HKQuantityTypeIdentifierBodyMass",                "weight_kg",     "latest"),
    ("HKQuantityTypeIdentifierVO2Max",                  "vo2_max",       "avg"),
    ("HKQuantityTypeIdentifierTimeInDaylight",          "daylight_min",  "sum"),
    ("HKCategoryTypeIdentifierSleepAnalysis",           "sleep_hours",   "sleep"),
]
METRIC_BY_TYPE = {t: (c, a) for (t, c, a) in DAILY_METRICS}
COLUMNS = ["date"] + [c for (_, c, _) in DAILY_METRICS]


def parse_dt(s):
    if not s:
        return None
    try:
        return datetime.strptime(s[:19], "%Y-%m-%d %H:%M:%S")
    except ValueError:
        return None


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: parse_apple_health.py <export_dir> <repo_dir>")
    export_dir = sys.argv[1]
    repo = sys.argv[2]
    out = os.path.join(repo, "health")
    os.makedirs(out, exist_ok=True)

    xml_path = None
    for name in ("export.xml", "导出.xml"):
        p = os.path.join(export_dir, name)
        if os.path.isfile(p):
            xml_path = p
            break
    if not xml_path:
        sys.exit(f"no export.xml or 导出.xml in {export_dir}")

    daily = defaultdict(lambda: defaultdict(list))
    sleep_minutes = defaultdict(float)
    workouts = []
    type_counts = defaultdict(int)
    date_min = None
    date_max = None

    ctx = ET.iterparse(xml_path, events=("end",))
    for _, el in ctx:
        if el.tag == "Record":
            t = el.get("type", "")
            type_counts[t] += 1
            sd = parse_dt(el.get("startDate", ""))
            ed = parse_dt(el.get("endDate", ""))
            if sd:
                if date_min is None or sd.date() < date_min:
                    date_min = sd.date()
                if date_max is None or sd.date() > date_max:
                    date_max = sd.date()
            spec = METRIC_BY_TYPE.get(t)
            if spec and sd:
                col, agg = spec
                if agg == "sleep":
                    val = el.get("value", "")
                    if val == "HKCategoryValueSleepAnalysisAsleepUnspecified" \
                       or val.startswith("HKCategoryValueSleepAnalysisAsleep"):
                        if ed:
                            sleep_minutes[sd.date().isoformat()] += (ed - sd).total_seconds() / 60.0
                else:
                    v = el.get("value", "")
                    try:
                        v = float(v)
                    except ValueError:
                        v = None
                    if v is not None:
                        unit = el.get("unit", "")
                        if unit == "mi":
                            v *= 1.609344
                        if unit == "lb":
                            v *= 0.45359237
                        daily[sd.date().isoformat()][col].append(v)
            el.clear()
        elif el.tag == "Workout":
            wt = el.get("workoutActivityType", "").replace("HKWorkoutActivityType", "")
            sd = el.get("startDate", "")
            ed = el.get("endDate", "")
            dur = el.get("duration", "")
            dist = el.get("totalDistance", "")
            dist_u = el.get("totalDistanceUnit", "")
            kcal = el.get("totalEnergyBurned", "")
            kcal_u = el.get("totalEnergyBurnedUnit", "")
            workouts.append([sd, ed, wt, dur, dist, dist_u, kcal, kcal_u])
            el.clear()

    with open(os.path.join(out, "daily.csv"), "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(COLUMNS)
        all_dates = set(daily.keys()) | set(sleep_minutes.keys())
        for d in sorted(all_dates):
            row = [d]
            for (_, col, agg) in DAILY_METRICS:
                if agg == "sleep":
                    mins = sleep_minutes.get(d, 0.0)
                    row.append(round(mins / 60.0, 2) if mins else "")
                    continue
                vals = daily[d].get(col, [])
                if not vals:
                    row.append("")
                elif agg == "sum":
                    row.append(round(sum(vals), 2))
                elif agg == "avg":
                    row.append(round(sum(vals) / len(vals), 2))
                elif agg == "latest":
                    row.append(round(vals[-1], 2))
            w.writerow(row)

    with open(os.path.join(out, "workouts.csv"), "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["start", "end", "type", "duration_min", "distance",
                    "dist_unit", "kcal", "kcal_unit"])
        w.writerows(workouts)

    ecg_src = os.path.join(export_dir, "electrocardiograms")
    ecg_count = 0
    if os.path.isdir(ecg_src):
        ecg_dst = os.path.join(out, "ecg")
        os.makedirs(ecg_dst, exist_ok=True)
        for name in os.listdir(ecg_src):
            if name.endswith(".csv"):
                shutil.copy2(os.path.join(ecg_src, name), os.path.join(ecg_dst, name))
                ecg_count += 1

    md = [
        "# Apple Health Export — Audit",
        "",
        f"- Source: `{export_dir}`",
        f"- Date range: **{date_min} → {date_max}**",
        f"- Days with any record: **{len(set(daily.keys()) | set(sleep_minutes.keys())):,}**",
        f"- Workouts: **{len(workouts):,}**",
        f"- ECG files copied: **{ecg_count}**",
        f"- Distinct record types: **{len(type_counts)}**",
        "",
        "## Top 30 record types (by count)",
        "",
        "| Type | Count |",
        "|---|---:|",
    ]
    for t, c in sorted(type_counts.items(), key=lambda x: -x[1])[:30]:
        short = (t.replace("HKQuantityTypeIdentifier", "Q:")
                  .replace("HKCategoryTypeIdentifier", "C:"))
        md.append(f"| {short} | {c:,} |")
    md.append("")
    md.append("## Excluded by design")
    md.append("")
    md.append("- `workout-routes/*.gpx` — GPS location data, not extracted "
              "(sensitive, low signal for training-effect analysis).")
    md.append("- `export_cda.xml` — Clinical Document Architecture format, "
              "duplicates `导出.xml`/`export.xml`.")
    with open(os.path.join(out, "raw_record_counts.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(md) + "\n")

    readme = [
        "# Health subset of the kibble repo",
        "",
        "Generated by the `kibble-health` skill from an Apple Health export.",
        "",
        "## Files",
        "",
        "- `daily.csv` — one row per day, columns are aggregated metrics.",
        "  Empty cell means no data was recorded that day for that metric.",
        "  Aggregations: `steps`, `*_kcal`, `*_min`, `*_km`, `flights`, "
        "`daylight_min`, `sleep_hours` are **sums**;",
        "  `resting_hr`, `avg_hr`, `hrv_ms`, `spo2_pct`, `resp_rate`, "
        "`vo2_max` are **daily averages**;",
        "  `weight_kg` is the **latest** measurement on that day.",
        "- `workouts.csv` — one row per workout. `duration_min` is in minutes; "
        "distance unit and kcal unit are per-row (HealthKit exports each as-set).",
        "- `ecg/*.csv` — Apple Watch ECG exports copied verbatim.",
        "- `raw_record_counts.md` — audit of what came in.",
        "",
        "## Not stored",
        "",
        "- GPS routes (`workout-routes/*.gpx`) — deliberately not synced.",
        "- Raw `export.xml` / `导出.xml` — too large for git; re-export from "
        "iOS Health when re-syncing.",
    ]
    with open(os.path.join(out, "README.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(readme) + "\n")

    print(f"daily.csv: {len(set(daily.keys()) | set(sleep_minutes.keys()))} days")
    print(f"workouts.csv: {len(workouts)} workouts")
    print(f"ecg: {ecg_count} files")
    print(f"date range: {date_min} → {date_max}")


if __name__ == "__main__":
    main()
