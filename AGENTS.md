# AGENTS.md

## Repo Rules

- Do not leave legacy code paths, compatibility fallbacks, dead transitional branches, or "old path just in case" logic in this repo.
- When replacing a behavior, remove the old implementation in the same change unless the user explicitly asks to keep both paths.
- If a temporary migration path seems unavoidable, stop and ask first instead of silently leaving legacy behavior behind.
