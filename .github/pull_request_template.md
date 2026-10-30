## What changes for the player (or contributor)

## How it was tested

- [ ] `cargo test --workspace`
- [ ] `cd app/src-tauri && cargo test`
- [ ] `cd app && npx tsc --noEmit -p . && npx vitest run`
- [ ] `cd pipeline-ingest && uv run pytest` (and `-m slow` if the pipeline changed)
- [ ] New interface text has its English in `app/src/locales/en/`
