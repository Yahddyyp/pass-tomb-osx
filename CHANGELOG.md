# Changelog

All notable changes to this project are documented here.

## [1.5.0] - 2026-10-22

### Summary
This release inplements a lot of fixes as this was initally vibe coded so now i have to go along and fix up the bugs in the code.

### Added
- Per tomb persistent timer storage keyed to the canonical path of each tomb image.
- Timer worker token verification and automatic cancellation of pending timers on manual close.
- Post detach inventory verification confirming disk image detachment.
- Post mount cleanup that unmounts newly opened tombs if ownership or store initialization fails.
- Diagnostic logging for background auto-close timer workers to `worker.log`.
- Fallback lookup in `PATH` for the `pass-tomb` worker executable when not located beside `pass-open`.
- Multi-image `hdiutil info` regression test suite.

### Changed
- Restructured `cmd_close` to safely resolve targets by canonical tomb path, active password store mount directory, or volume name without falling back to unrelated volumes.
- Tightened GPG trust level validation to require trusted keys (`m`, `f`, `u`), rejecting undefined/unknown trust (`w`) and special internal keys (`s`).
- Validated tomb size values across CLI options and environment variables, replacing silent fallbacks with descriptive errors.

### Fixed
- Fixed critical atomicity bug in `cmd_change` and `cmd_chkey` by changing the disk image password first and using atomic key replacement to prevent irreversible lockouts.
- Fixed `parse_info_plist` XML parsing bug where having any other disk image attached on macOS hid open tombs from `list_open_tombs`.
- Fixed orphaned key files remaining on disk when `hdiutil create` fails during tomb initialization.
- Fixed `cmd_resize` checking directory existence (`~/.password-store`) instead of querying the actual tomb mount status.
- Fixed `cmd_open` attempting to double-mount tombs that are already open.
- Fixed `cmd_timer` displaying internal hash filenames instead of human-readable tomb paths.

## [1.0.0] - 2026-07-18
- Initial release
