# Changelog

## [0.3.0] - 2026-08-08

### Added

- Extended libmpv playback controls: speed, output device, ReplayGain, equalizer, channel mode, balance, A-B looping, gapless playback, and fades.
- Configurable settings-page shortcuts for every new playback and data-management action.
- A playback-controls submenu in the song context menu.
- Incremental local-library watching and diagnostics for duplicate, damaged, and missing files.
- Versioned data export/import with atomic writes, automatic backups, migration, and M3U/LX Music/NetEase playlist import.

### Changed

- Local metadata and artwork handling now uses `lofty`; CUE parsing prefers `cue-rw` with a compatibility fallback.
