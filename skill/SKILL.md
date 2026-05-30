---
name: kibble-health
description: Use when the user wants to process the Kibble health-log inbox, sync an Apple Health export, or generate a diet evaluation report. Reads `~/Library/Application Support/kibble/config.toml`, sorts new photos in `inbox/` into `YYYY/MM/DD/<meal_type>/`, identifies image type (food / receipt / menu), looks up calories and macros via WebSearch, writes a `.nutrition.json` next to each image, updates `index.csv`, then commits + pushes. Apple Health sync mode streams `~/Desktop/apple_health_export/导出.xml` (or `export.xml`) into `health/daily.csv` + `health/workouts.csv` + `health/ecg/*.csv` inside the repo, deliberately excluding GPS routes (`workout-routes/*.gpx`). Diet report mode aggregates `index.csv` into a Word report on the Desktop with a per-day macro table and trend line-charts (calories/fat/protein) carrying red threshold lines. At the end of every Process run, the skill MUST ask the user whether to sync Apple Health too. Triggered by phrases like "process kibble inbox", "整理 kibble", "process my food log", "sync apple health", "同步 apple health", "diet report", "饮食报告", "评价我的饮食".
metadata:
  short-description: Sort Kibble inbox photos, classify food vs receipts, look up nutrition via web search, sync Apple Health export aggregates, generate diet evaluation report
---

# Kibble Inbox Processor

This skill is the consumer end of the Kibble pipeline. Kibble (the desktop app) dumps photos and notes into `inbox/`. This skill does everything downstream: timestamp parsing, meal-type classification, **image-type detection (food / receipt / menu)**, **nutrition lookup via WebSearch**, folder organisation, and the git commit that records the work.

## Modes

1. **Process** (default) — drain `inbox/` into dated folders, annotate, commit.
2. **Apple Health sync** — parse `~/Desktop/apple_health_export/` into `<repo>/health/`. See [Apple Health sync](#apple-health-sync).
3. **Diet report** — aggregate `index.csv` into a Word report on the Desktop (table + trend charts + data-driven signals). See [Diet report](#diet-report). Read-only on the repo; never commits.

Decide which mode based on the user's phrasing. After Process finishes (and before printing the summary table), the skill **must ask the user**: `also sync Apple Health export? (y/n)`. If yes, run Apple Health sync and fold it into the same commit.

Diet report is a standalone, read-only mode — triggered by phrases like "diet report", "饮食报告", "评价/看看我的饮食", "evaluate my eating". It does not touch `inbox/`, does not commit, and is never auto-offered after Process (unlike Apple Health sync).

## Inputs

None from the user. Everything is read from disk:

- `~/Library/Application Support/kibble/config.toml` — for `repo_path` and `[meal_times]`
- `<repo_path>/inbox/` — Process-mode items
- Each image's macOS metadata (`sips -g creation`)
- Each image's pixels (Read into Claude for type detection + identification)
- Sibling `.note.txt` if it exists

If the user explicitly passes a different repo path, honour it; otherwise use config.

## Core Rules

1. **Do not invent food labels, capture times, calories, macros, or vendors.** Every value must trace either to (a) on-disk data, (b) what you can see in the image, or (c) a WebSearch result with the source URL recorded. If you cannot determine a value, write `null` and add a `notes` entry explaining why. Do not guess.
2. **Preserve original bytes during processing.** Never re-encode HEIC or JPG. For HEIC vision, write a transient JPG to `/tmp/kibble-view-<random>.jpg` via `sips -s format jpeg`, Read it, then delete it. The original image is read once for vision, then deleted after `.nutrition.json` is written (see Step 6). Storage minimisation is preferred over keeping originals; everything needed for retrospective review must live in `.nutrition.json` and `.note.txt`.
3. **`.note.txt` and `.nutrition.json` follow the image's logical slot** when filing (Process mode). The image itself is deleted; only the text artefacts persist.
4. **Idempotent on collisions.** Destination filename collision → append `_1`, `_2`, …; apply the same suffix to all sibling files (note + nutrition). Never overwrite.
5. **One commit per run.** Message: `skill: process N item(s)`.
6. **Empty work is success.** No images to process → print `nothing to do` and exit without a commit.
7. **Honest confidence.** Every nutrition.json must carry a `confidence: "high" | "med" | "low"` field per the [confidence rubric](#confidence-rubric). Do not promote `low` to `med` because the number "looks reasonable."

## Workflow

### Step 1 — Load config

Read `~/Library/Application Support/kibble/config.toml`. Extract:

- `repo_path` (string, required)
- `meal_times.breakfast`, `lunch`, `snack`, `dinner` (each `["HH:MM", "HH:MM"]`)
- Anything outside the four windows is `late_night`

Abort with a clear error if `repo_path` is empty or missing on disk.

### Step 2 — Collect work

`ls -1 "$REPO/inbox/"`, filter to image extensions: `.jpg .jpeg .png .gif .webp .heic .heif .tif .tiff .bmp`. Note files (`*.note.txt`) are followers.

If the list is empty, print `nothing to do` and exit.

### Step 3 — Per-image: type detection + identification

For each image, in order:

#### 3a — Capture time
`sips -g creation "<path>"` → parse `creation: YYYY:MM:DD HH:MM:SS` (local clock). Fallback: `stat -f %Sm -t '%Y:%m:%d %H:%M:%S' "<path>"`. If both fail: `capture_time = "unknown"`, `meal_type = "unknown"`.

#### 3b — Meal type
Compare `HH:MM` against config windows. Defaults:
- breakfast `06:00–09:59`
- lunch `10:00–13:59`
- snack `14:00–16:59`
- dinner `17:00–19:59`
- else `late_night`

#### 3c — Read note
If `<path>.note.txt` exists, read + trim. Empty file → no note.

#### 3d — Image type detection (NEW)
Convert HEIC → temp JPG if needed, Read the image. Classify into exactly one of:

| `image_type` | Meaning | Sources for nutrition |
|---|---|---|
| `food` | The image shows the food/drink itself | Vision portion estimate + WebSearch macros per 100g |
| `receipt` | The image shows a printed/digital order receipt with itemised line items | Item names from receipt + WebSearch each item from the vendor |
| `menu` | The image shows a menu (chosen item not yet eaten) | Same as receipt if it shows what was ordered, else skip nutrition (`confidence: low`, items: []) |
| `non_food` | Anything that isn't food, receipt, or menu (random object, blurry, dark) | `unidentified`, no nutrition |

Set `food_label`:
- `food`: 1–4 word lowercase English label (e.g. `beef noodles`, `mixed salad`, `iced latte`). Multiple visible items joined with `+` (`noodles+egg`).
- `receipt` / `menu`: `<vendor-slug> receipt` or `<vendor-slug> menu` (e.g. `mcdonalds receipt`, `kfc menu`). Vendor slug is lowercase, words joined with hyphens.
- `non_food`: `unidentified`.

### Step 4 — Nutrition lookup

The exact procedure depends on `image_type`.

#### 4a — `food`

1. **Identify each distinct food/drink** visible. For each, write a short canonical English name (e.g. `beef noodle soup`, not `牛肉面`).
2. **Estimate portion** from visual cues. Use known references when available (bowl ~400 ml standard, plate ~25 cm, can ~330 ml, chopsticks for scale, etc.). If no reliable reference is visible, record `estimated_g: null` and downgrade confidence.
3. **WebSearch** `"<food name> calories per 100g"` or `"<food name> nutrition per 100g"`. Prefer authoritative sources (USDA FoodData Central, official restaurant nutrition pages, fatsecret, myfitnesspal). Record the source URL in the item's `source_url`.
4. **Compute** `calories = (per_100g.calories × estimated_g) / 100` and same for `protein_g`, `carb_g`, `fat_g`. Round to integers.
5. Set [confidence](#confidence-rubric) using the rubric.

#### 4b — `receipt`

1. **OCR the line items** from the receipt image. Capture: vendor name, each item's name (in source language), quantity, price if visible. Many Chinese chain receipts will have the brand logo + items in Chinese.
2. **For each line item**, WebSearch the official nutrition: `"<vendor> <item name> calories"`. Prefer the vendor's own nutrition page (e.g. mcdonalds.com, kfc.com.cn, starbucks.com). Record `source_url`.
3. **If the receipt total doesn't match the sum of identified items** (e.g. you missed a line, OCR errors), add a `notes` entry — do not silently force consistency.
4. **Multi-quantity items** (e.g. `麦乐鸡5块 ×1` = 5-piece nuggets, qty 1): expand quantity properly. `5块` is a size, not a count multiplier — only multiply by the line's order quantity.
5. Confidence is typically `high` for chain restaurants with published nutrition, `med` if you had to estimate item identity, `low` if the receipt is partially unreadable.

#### 4c — `menu`

If the menu image shows a *specific* item the user chose (rare — usually only if they wrote the chosen item in `.note.txt`), treat like a receipt for that item only. Otherwise: `items: []`, `confidence: low`, `notes: ["menu image without selected item"]`.

#### 4d — `non_food`

Skip nutrition entirely. Write a minimal `.nutrition.json` so it doesn't get picked up again:
```json
{
  "image_type": "non_food",
  "food_label": "unidentified",
  "confidence": "high",
  "items": [],
  "totals": {"calories": null, "protein_g": null, "carb_g": null, "fat_g": null},
  "notes": ["image is not food, receipt, or menu"]
}
```

### Step 5 — Write `.nutrition.json`

Path: same dir as the image, basename = `<image filename>.nutrition.json` (e.g. `IMG_1728.HEIC.nutrition.json`).

#### Schema (canonical)

```json
{
  "image_type": "food | receipt | menu | non_food",
  "food_label": "string",
  "vendor": "string | null",
  "confidence": "high | med | low",
  "items": [
    {
      "name": "string (canonical English)",
      "name_source": "string | null (the original-language string from the image/receipt)",
      "quantity": 1,
      "estimated_g": 250,
      "calories": 420,
      "protein_g": 18,
      "carb_g": 55,
      "fat_g": 12,
      "source_url": "https://..."
    }
  ],
  "totals": {
    "calories": 420,
    "protein_g": 18,
    "carb_g": 55,
    "fat_g": 12
  },
  "notes": ["free-form caveats, units, exclusions"]
}
```

Rules:
- `totals.*` must equal the sum of `items[*].*` for the four macros (integers). Recompute, don't transcribe.
- `vendor` is null for `food` and `non_food`. Required for `receipt` and `menu`.
- `estimated_g` is null for receipt items where the vendor publishes calories per *serving* directly.
- `source_url` is null only if you used your own training knowledge AND no WebSearch was performed (last resort, downgrade confidence to `low`).
- File must be UTF-8, pretty-printed (2-space indent), with a trailing newline.

### Step 6 — File text artefacts, delete image (Process mode only)

After `.nutrition.json` is written (in inbox/ temporarily):

```
inbox/IMG_xxx.HEIC.note.txt        → YYYY/MM/DD/<meal>/IMG_xxx.HEIC.note.txt
inbox/IMG_xxx.HEIC.nutrition.json  → YYYY/MM/DD/<meal>/IMG_xxx.HEIC.nutrition.json
inbox/IMG_xxx.HEIC                 → rm (deleted)
```

`mv` the text artefacts, `rm` the image. Inbox should be empty (except `.gitkeep`) when Process mode finishes. The image filename is preserved as the basename of the text artefacts so the logical identity (and `filename`/`path` columns in `index.csv`) remains intact even though the image bytes are gone.

**Exception — vision read failure**: if `image_type == "unknown"` because `sips` conversion or the vision Read failed, **do not delete** the image. Move it to `<repo>/unreadable/IMG_xxx.HEIC` along with its `.note.txt` so the user can manually re-process. Write a stub `.nutrition.json` in the dated folder marking `confidence: low` and `notes: ["image unreadable, kept in unreadable/ for manual review"]`.

### Step 7 — Update `index.csv`

Schema (new columns):

```
date,time,meal_type,image_type,food_label,calories,protein_g,carb_g,fat_g,confidence,filename,note,path
```

Header is written only if file doesn't exist. Numeric fields are integers or empty string for `null`. Quote any field containing commas (RFC 4180). The `path` is relative to `<repo>`, the `note` is the user's original text (do NOT append receipt items here — they live in nutrition.json).

Use a temp file + rename to avoid partial writes.

### Step 8 — Commit + push

```bash
cd "$REPO"
git add -A
if ! git diff --cached --quiet; then
  git commit -m "<message>"
  git push
fi
```

Message format: `skill: process N item(s)`.

Singular/plural: `1 item` / `2 items`.

Push failures: print git's stderr verbatim, do not roll back. The work is still on disk.

### Step 9 — Summary table

Print exactly this shape (columns aligned, no trailing whitespace):

```
processed N items, K errors

12:25 lunch    food     beef noodles       ~650 kcal (med)    IMG_1728.HEIC
18:22 dinner   receipt  mcdonalds receipt  ~1115 kcal (high)  IMG_1737.HEIC
                          ↳ filet-o-fish, hash brown, mcnuggets 5pc, cola zero (M)

→ pushed as commit <sha>
```

Layout details:
- Columns: `HH:MM`, `meal_type`, `image_type`, `food_label` (truncate to 22 chars), `~<cal> kcal (<conf>)`, `filename`. Single-space separator, padded to align.
- For receipts, add a wrapped `↳` line listing identified items (truncate the whole line to 80 chars; append `…` if truncated).
- For `non_food`: `<filename>: skipped (non-food)`.
- If `confidence: low`, prefix calories with `≈`: `≈650 kcal (low)`.
- If `image_type` was `unknown` due to a vision read failure, show `???` in place of all derived columns.

## Apple Health sync

This mode imports an iOS Health export into `<repo>/health/`. It is offered as a follow-up question at the end of every Process run, and can also be invoked directly ("sync apple health", "同步 apple health").

### What gets synced

- `daily.csv` — one row per day with sum/avg/latest aggregates of: steps, active/basal energy burned, exercise time, stand time, distance (walk/run + cycle), flights climbed, resting HR, average HR, HRV (SDNN), SpO₂, respiratory rate, body mass, VO₂ max, time in daylight, sleep hours.
- `workouts.csv` — one row per workout: start, end, activity type, duration in minutes, distance + unit, kcal + unit.
- `ecg/*.csv` — Apple Watch ECG exports copied verbatim.
- `raw_record_counts.md` — audit of record types found and how many of each.
- `README.md` — schema notes for the directory.

### What does NOT get synced (deliberate)

- **`workout-routes/*.gpx`** — GPS route data. Excluded because (a) location history is sensitive, (b) low signal for training-effect/nutrition analysis. Do not be tempted to "just include a few" — the script never opens these files.
- **`export_cda.xml`** — Clinical Document Architecture; duplicates `导出.xml`/`export.xml`.
- **`导出.xml` / `export.xml` itself** — too large for git (often >500 MB). Re-export from iOS Health when re-syncing.

### Locating the export

Default path: `~/Desktop/apple_health_export/`. Inside, look for `导出.xml` first, fall back to `export.xml` (locale dependent). If neither exists, ask the user where the export is. If the directory itself doesn't exist, tell the user how to export from iOS:

> Health app → profile icon → Export All Health Data → AirDrop/save → unzip on this Mac.

### Run

```bash
python3 ~/.claude/skills/kibble-health/parse_apple_health.py \
  ~/Desktop/apple_health_export \
  <repo_path>
```

Expect ~15–60 seconds for a multi-year export. The script is streaming so memory stays low.

### Folding into the commit

If Apple Health sync is part of a Process run, fold the new files into the same commit. Commit message becomes:

- Process + health: `skill: process N item(s) + sync apple health`
- Health only: `skill: sync apple health (<date_range>)` where `<date_range>` is `YYYY-MM-DD..YYYY-MM-DD` taken from the script's stdout.

### Re-sync semantics

The script overwrites `daily.csv`, `workouts.csv`, `raw_record_counts.md`, `README.md`, and any `ecg/*.csv` that match by name. It does not delete files that no longer exist in the export — if the user truncates or re-exports, leftover ECG files from prior syncs stay put. Tell the user this if they ask.

### Report generation

**Always** run the report generator immediately after `parse_apple_health.py` succeeds:

```bash
python3 ~/.claude/skills/kibble-health/generate_report.py <repo_path>
```

This writes a single **Word document** to the user's Desktop, NOT into the repo:

- `~/Desktop/kibble-health-report-<YYYY-MM-DD>.docx`

`.docx` is used (instead of `.md`) so the report opens natively in Word / Pages / Google Docs / LibreOffice / macOS Quick Look without needing a markdown viewer. Typical size: ~300 KB; the 5 charts (RHR/HRV trend, weekly volume, sleep↔HRV scatter, VO₂ max, steps+kcal) are embedded as PNGs inside the .docx package.

The report includes an **Observations & recommendations** section. The script computes these deterministically — each bullet must reference a specific number from the data (sample size, threshold value, %-change). If a signal is in the noise band or the sample is too thin, **no recommendation is generated for that area** rather than a generic one. Nutrition recommendations require ≥14 days of logged meals; below that, the report says how many more days are needed.

The recommendation logic is in `build_recommendations()` inside `generate_report.py`. To add a new rec, append to that function with a clear (signal threshold → observation string → action string) mapping. Do not add platitudes ("stay hydrated", "eat more vegetables") — only actions tied to a data signal.

**Critical separation rule:** the repo (`~/Desktop/health-log/`) stores only *source* data — CSV aggregates from Apple Health and the meal log. Reports, charts, and any other derived/regenerable artefact must NEVER be written into the repo. Re-running the skill should produce a fresh report on Desktop without churning git history.

If a previous report with the same date exists on Desktop, overwrite it (one report per day is enough). If the user wants to keep history, they can rename it before re-running.

After generation, surface the report's path to the user in the summary table (Step 9) so they can `open` it.

Report generation depends on `matplotlib`, `pandas`, `numpy`, `python-docx`. On macOS these are typically at `/Library/Frameworks/Python.framework/Versions/3.13/bin/python3`. If `python3 -c "import docx, matplotlib"` fails, tell the user (`pip3 install python-docx matplotlib pandas` fixes it) and skip the report step — the source CSV sync still completes and gets committed.

### Failure modes

- **No XML found** — abort, tell the user which path was checked, suggest re-export.
- **Python missing** — should not happen on macOS; if it does, fall back to telling the user `which python3` returned nothing.
- **XML truncated / parse error** — script raises; do not partial-write. Print the line number from the error.
- **Repo's `health/` folder doesn't exist** — script creates it.

## Diet report

A standalone, read-only mode that turns the accumulated meal log (`<repo>/index.csv`) into a Word evaluation report on the Desktop. It does **not** read `inbox/`, does **not** sync Apple Health, and does **not** commit anything — it only reads `index.csv`.

Triggered by phrases like "diet report", "饮食报告", "评价/看看我的饮食", "evaluate my eating". Never auto-offered after Process (that question is reserved for Apple Health sync).

### Run

```bash
python3 ~/.claude/skills/kibble-health/generate_diet_report.py <repo_path>
```

Writes `~/Desktop/kibble-diet-report-<YYYY-MM-DD>.docx` (overwrites same-day). Surface the path so the user can `open` it.

### What the report contains

The script (`generate_diet_report.py`) produces everything **deterministically** from `index.csv`:

- **Overview** — days logged, complete vs partial days, breakfast coverage, fast-food and sweet/fried counts.
- **Per-day macro table** — calories/protein/carb/fat per day, plus auto notes (`无早餐`, `仅单餐·未记全`, `脂肪超100g`) and a complete-days average row.
- **Trend line-charts** — one PNG per entry in the `THRESHOLDS` dict (default: calories, fat, protein), each with a **red dashed threshold line**. Each threshold has a `kind`: `ceiling` (a day **over** the line is marked red — calories, fat) or `floor` (a day **under** the line is marked red — protein minimum). Crossing days get a red marker + value annotation, and a caption lists their dates.
- **Data-driven signals** — `build_signals()` emits "做得好" and "需要改进" bullets. **Every bullet references a real number** (e.g. "11 天里仅 1 天记录早餐"); a signal in the noise band emits **no bullet** rather than a platitude — same philosophy as `build_recommendations()` in `generate_report.py`.

### Charts excluded partial days

Charts plot **only "complete" days** (≥2 meal slots or ≥2 items) so a single-meal day doesn't masquerade as a calorie crash and skew the trend. The per-day table still lists every day (partial ones flagged in the notes column).

### Fonts

Chart axis/title labels are **English on purpose** — matplotlib renders CJK as tofu boxes without a bundled CJK font. The Word body text is full Chinese; only the embedded chart images use English labels. Do not "fix" this by forcing a Chinese font unless one is known-present.

### Tuning

Thresholds and label keyword buckets (`THRESHOLDS`, `FAST`, `SWEET`, `FRIED`, `VEG`, `FAT_NOTE_G`, `HIGH_CAL_DAY`) live at the top of `generate_diet_report.py`. To add a chart, add an entry to `THRESHOLDS` with a `label`, `line`, and `kind`. To add a signal, append to `build_signals()` with a clear (number-bearing threshold → bullet string) mapping. Do not add platitudes — only observations tied to a data signal.

### Layering qualitative interpretation

The script intentionally does **not** hardcode prose narrative (which would be specific to one dataset). When the user wants a richer read, layer the qualitative interpretation in the conversation on top of the deterministic report — do not bake dataset-specific commentary into the script.

### Separation rule (same as Apple Health)

The report and its chart PNGs are derived artefacts → **Desktop only, never into the repo**. The repo holds only source data (`index.csv` + nutrition JSON). Re-running must not churn git history.

### Dependencies / failure modes

- Needs `python-docx` + `matplotlib` (`pip3 install python-docx matplotlib`). If `python3 -c "import docx, matplotlib"` fails, tell the user and skip.
- **No `index.csv`** — abort with a message to run Process mode first.
- **Empty `index.csv`** — abort ("nothing to report").

## Confidence rubric

| Source | Confidence |
|---|---|
| Chain restaurant receipt, all items found on vendor's official nutrition page | `high` |
| Chain restaurant receipt, some items had to be looked up on third-party DB | `med` |
| Home-cooked or generic food, USDA FoodData Central per-100g + reliable visual reference (bowl/plate of known size, hand for scale) | `med` |
| Home-cooked food, no clear size reference, portion guess from "looks like a typical serving" | `low` |
| Multi-component dish where individual ingredients can't be cleanly separated (e.g. stir-fry, hotpot) | `low` regardless of source |
| Used your own knowledge without WebSearch | `low` |

If unsure between two levels, pick the lower one.

## Edge cases

- **`inbox/.gitkeep` present** — ignore.
- **Image but no note** — leave `note` column empty in csv; do not create a fake note file.
- **Note but no image** — orphan. Move the orphan `.note.txt` to `<repo>/orphans/`. Do not delete. Skip nutrition.
- **HEIC `sips` conversion fails** — `image_type: "unknown"`, no nutrition, `confidence: low`. Move the image to `<repo>/unreadable/` (do NOT delete) so it can be manually re-processed; still write a stub `.nutrition.json` in the dated folder.
- **Cross-day photos** — file by *capture* date.
- **WebSearch unavailable / rate-limited** — record `confidence: low`, `source_url: null`, add note `"WebSearch unavailable"`. Do not block the whole run.
- **Repo path differs across machines** — config is the source of truth; do not hardcode any path.
- **Beverages with no calories** (water, cola zero) — calories may legitimately be 0. Do not downgrade confidence for zeroes; they're a real value.

## What this skill does NOT do

- Re-encode or compress images.
- Re-classify already-filed items into different meal folders (capture time is canonical).
- Mutate `.note.txt` after it's filed (notes are user content, append-only via re-upload).
- Estimate calories without a WebSearch result for chain-restaurant items (low confidence, no `source_url` → flag for review, don't fabricate).
