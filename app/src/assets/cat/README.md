# Cat sprites

The SVG files here are throwaway placeholders so the state machine has something to render. They use coarse 64×64 viewBoxes to feel pixel-ish.

When you have real pixel-art PNGs (e.g. exported from Aseprite), drop them in here with the same base names and update the import map in `../../main.ts`:

- `idle` — neutral closed mouth
- `mouth_open` — mouth open, ready to receive
- `chewing` — mouth half-open with a crumb
- `swallow` — eyes closed, mouth squeezed shut
- `happy` — content (^_^) face
- `confused` — X_X face, optional `?` glyph
