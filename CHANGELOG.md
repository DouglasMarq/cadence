# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Back and forward arrows in the sidebar, available on every page.
- Update prompt: Cadence checks the latest GitHub release after launch and, on request, downloads, verifies and installs it on the macOS app and the AppImage, then restarts. Other installs get a link to the release page.
- Release pipeline: pushing a `v*` tag publishes the macOS app, the AppImage and `SHA256SUMS`.

### Changed

- Views show their last saved content at once, also after a restart, and refresh it in place.
- Opening a view stops the work of the previous one, and cover downloads are limited to six at a time, newest first.
- The artist page shows each section as it arrives instead of waiting for all of them and for the image.
