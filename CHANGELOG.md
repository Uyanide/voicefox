# Changelog

## [0.3.1] - 2026-08-08

### Fixed

- Reserved `1` through `8` for tab navigation on the settings page and moved the new playback/data shortcuts to `F1`-`F10` and `Shift+F1`-`Shift+F8`.
- Migrated legacy settings shortcuts so existing configuration files no longer retain the conflicting bare number keys.
- Made settings rows mouse-operable and added click, wheel, right-click, and drag controls to the JS source, local directory, and status-bar panels.
- Rendered the audio-device and external-playlist input dialogs opened from mouse or keyboard controls.

## [0.3.0] - 2026-08-08

### Added

- Extended libmpv playback controls: speed, output device, ReplayGain, equalizer, channel mode, balance, A-B looping, gapless playback, and fades.
- Configurable settings-page shortcuts for every new playback and data-management action.
- A playback-controls submenu in the song context menu.
- Incremental local-library watching and diagnostics for duplicate, damaged, and missing files.
- Versioned data export/import with atomic writes, automatic backups, migration, and M3U/LX Music/NetEase playlist import.

### Changed

- Local metadata and artwork handling now uses `lofty`; CUE parsing prefers `cue-rw` with a compatibility fallback.
