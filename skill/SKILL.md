# Kibble Skill (placeholder)

This file is a placeholder. The downstream Skill that consumes the `kibble-data` repository lives here. It should:

- Read new files from `inbox/`
- Parse EXIF timestamps and classify each item into `breakfast` / `lunch` / `snack` / `dinner` / `late_night` using the `[meal_times]` thresholds in `~/.config/kibble/config.toml`
- Identify food (vision model)
- Move processed items out of `inbox/` into a dated structure (e.g. `2026/05/20/lunch/IMG_1234.jpg`)
- Read any sibling `.note.txt` files as user-provided context
- Generate summaries on demand

None of this is implemented yet. Kibble's only job is to get the photos and notes into the repo.
