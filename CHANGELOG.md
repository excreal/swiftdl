# Changelog

All notable changes to SwiftDL are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.3] - 2026-11-05

### Added
- yt-dlp quality selection for XVideos, XNXX and HQPorner, including browser-extension routing.

### Fixed
- Pausing yt-dlp downloads now stops the whole process tree and preserves partial files for resume.
- Opening downloaded files on Linux uses the host application's environment, reports startup failures and does not wait for the application to close.
- Downloaded AppImages can be launched directly after granting owner execute permission.
- Dailymotion embedded-player links containing a video ID are converted to regular video links; missing IDs show an actionable error.
- Browser-preview media-site detection now matches supported hosts rather than domain names inside unrelated URLs.

## [0.3.1] - 2026-11-04

### Fixed
- **Magnet links and .torrent files did nothing** when you clicked Start
  download or Download later: adding a download only accepted web links, and
  the window didn't show the error. Magnet links and .torrent files on your
  computer are now accepted, pasting a magnet link into New download works,
  and any error adding a download is shown in the window.

### Added
- **Firefox add-on.** The extension now runs in Firefox 140 and later, built
  from the same code with Firefox's manifest and shipped with the app and as
  `swiftdl-firefox.xpi`. SwiftDL registers its native messaging host for
  Firefox, and Settings → Browser lists Firefox.

## [0.3.0] - 2026-11-01

### Added
- **Torrents and magnet links.** SwiftDL downloads torrents with its own
  BitTorrent engine (librqbit): the New download window lists the files to
  pick from, and progress shows peers and seeds.
- **Opens magnet links and .torrent files.** SwiftDL registers as the
  computer's handler for both on Windows, macOS and Linux, and the browser
  extension sends magnet links and .torrent downloads to it.
- **Seeding choice.** Settings → General → After a torrent finishes: stop
  sharing, share until a ratio of 1 or 2, or keep sharing. The download is
  ready either way.

## [0.2.3] - 2026-10-31

### Changed
- The download window's progress bar is a solid, easy-to-read bar instead of a
  thin line, and the status no longer starts with a coloured dot.

## [0.2.2] - 2026-10-31

### Added
- **Theme setting.** Settings → General → Theme: System (follow the computer's
  light or dark mode), Light or Dark. Picking one previews it at once, and open
  download windows follow it.

### Fixed
- The extension's "Download video" button no longer breaks onto two lines on
  large videos.

## [0.2.1] - 2026-10-30

### Added
- **YouTube and other video sites.** The video button, the popup and the New
  download window offer each quality, from 4K down to audio only. These sites
  go through yt-dlp, with ffmpeg joining video and audio; both are fetched on
  first use (ffmpeg comes from the system on macOS and Linux) and yt-dlp is
  checked for updates daily. The download window shows yt-dlp's progress and
  steps such as merging.

### Changed
- **Adding the extension is a guided step.** Until the extension is on the
  Chrome Web Store, *Add the extension* opens a four-step guide that points at
  the copy SwiftDL installs, with a button to open that folder, instead of
  store buttons that led nowhere. The automatic-offer switch is hidden until
  the listing exists.
- **Simpler extension popup.** A compact menu in the app's style: the status
  under the name, single-line settings, and the video list only when the page
  has something to download.
- The New download window comes to the front instead of flashing in the
  taskbar.
- The README now points at the installed extension folder rather than the zip.

### Fixed
- Settings → Browser showed every browser as needing the extension even when
  it was connected. SwiftDL now records which browser the extension talked to
  from, and marks that browser as connected.
- Contacts that didn't come from a browser (such as a test) no longer count
  as the extension being connected.
- The extension popup no longer shows "SwiftDL isn't installed" while it is
  connected.
- The download window's shadow no longer shows as a faint box around it.

## [0.2.0] - 2026-10-30

### Added
- **A window for every download.** A file caught from the browser opens a
  small *New download* window (address, category, save location, size) instead
  of the main window. Starting it turns the same window into live progress, with
  each connection's start position and progress, and then a *Download complete*
  summary with Open and Show in folder.
- **Download video button.** The extension detects video and audio files a page
  loads and shows a button over video players that lists them. The popup lists
  them too, and the toolbar icon shows how many were found.
- **Automatic extension offer.** SwiftDL registers its extension with installed
  Chrome, Edge and Brave, which show a one-time *New extension added* prompt
  (requires the Chrome Web Store listing). A *Connect your browser* bar and a
  Browser settings page show which browsers were found and whether the
  extension has connected.
- **Alt+click bypass**, a per-site switch and a small-file threshold in the
  extension.
- **System tray.** Closing the main window keeps downloads and browser
  integration running; the tray menu opens or quits SwiftDL.
- The engine reports each connection's start, position and end.
- The uninstaller removes SwiftDL's browser registrations.
- Test pages for the extension in `tools/throttle_server.py`.

### Changed
- Calmer visual design: neutral buttons, one system-blue accent for progress
  and focus, no gradients, label/value details instead of boxed stat cards.
- New graphite logo.
- Redesigned extension popup in the app's style.
- Connections are shown as one rounded slot per connection, in both the main
  window and the download window.
- Softer file-type tiles throughout the app.

### Fixed
- Frameless windows no longer show the light Windows 11 border, and download
  windows no longer show white edges around their frame.

## [0.1.0] - 2026-10-30

### Added
- Segmented, resumable download engine: multiple connections per file, dynamic
  segmentation (idle connections take half of the largest remaining part),
  pre-allocated output written in place, resume file saved every second,
  retries with backoff, and a global speed limiter.
- `swiftdl` command-line tool.
- Desktop app (Tauri 2) with a queue, categories, settings, notifications,
  command palette, inspector with speed chart, and light and dark themes.
- Chromium extension (Manifest V3) that hands browser downloads to the app with
  cookies, referrer and user agent, plus a *Download with SwiftDL* context menu.
- The app doubles as the native messaging host and registers itself per user.
- GitHub Actions for CI and tagged releases (Windows, macOS, Linux and the
  extension zip).

[Unreleased]: https://github.com/harshsinghsv/swiftdl/compare/v0.3.3...HEAD
[0.3.3]: https://github.com/harshsinghsv/swiftdl/compare/v0.3.2...v0.3.3
[0.3.1]: https://github.com/harshsinghsv/swiftdl/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/harshsinghsv/swiftdl/compare/v0.2.3...v0.3.0
[0.2.3]: https://github.com/harshsinghsv/swiftdl/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/harshsinghsv/swiftdl/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/harshsinghsv/swiftdl/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/harshsinghsv/swiftdl/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/harshsinghsv/swiftdl/releases/tag/v0.1.0
