# Clock

[![Sponsor](https://img.shields.io/badge/sponsor-FreddyFunk-ea4aaa?logo=github-sponsors)](https://github.com/sponsors/FreddyFunk)
[![CI](https://github.com/cosmic-utils/cosmic-ext-clock/actions/workflows/ci.yml/badge.svg)](https://github.com/cosmic-utils/cosmic-ext-clock/actions/workflows/ci.yml)
[![Release](https://github.com/cosmic-utils/cosmic-ext-clock/actions/workflows/release.yml/badge.svg)](https://github.com/cosmic-utils/cosmic-ext-clock/actions/workflows/release.yml)
[![Translation status](https://hosted.weblate.org/widget/cosmic-utils/clock/svg-badge.svg)](https://hosted.weblate.org/engage/cosmic-utils/)

A responsive clock application for COSMIC desktops and Linux phones, built with [libcosmic](https://github.com/pop-os/libcosmic).

<p>
  <img src="resources/icons/hicolor/scalable/apps/org.cosmic_utils.clock.svg" alt="Clock app icon" width="80">
</p>

![Clock preview](preview/preview-001.png)

[View more screenshots](preview/README.md)

## Features

Alarms are evaluated while Clock is running. They are not yet backed by a system service, so closing the application also stops alarm delivery.

## Run

```bash
just run
just demo
```

`just demo` opens the deterministic world-clock page used as the basis for preview automation.

## Build and install

Requirements: Rust 1.98+, `just`, pkg-config, Wayland and XKB development libraries.

```bash
just check
just build-release
sudo just install
```

The Flatpak needs Wayland, fallback X11, DRI, PulseAudio-compatible playback, desktop notifications, read-only COSMIC configuration, and COSMIC settings-daemon access. It does not require network, location, or sensor permissions.

## Packaging and automation

The repository retains the blueprint's Flatpak, Alpine APK, cross-compilation, metadata generation, deterministic preview, CI, and release workflows. Run `just generate` after dependency or translation changes to refresh localized metadata and Flatpak Cargo sources.

## License

Licensed under GPL-3.0-only. Contributions intentionally submitted for inclusion are licensed under the same terms. Source files should carry `SPDX-License-Identifier: GPL-3.0-only`.