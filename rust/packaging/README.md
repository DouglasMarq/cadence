# Packaging the native app

Everything needed to ship Cadence on Linux and macOS.

| Path | What it is |
| --- | --- |
| `io.github.douglasmarq.cadence.desktop` | Desktop entry shared by every package (`Exec=cadence`) |
| `io.github.douglasmarq.cadence.metainfo.xml` | AppStream metainfo for software centers |
| `install-desktop-integration.sh` | Installs the desktop entry, metainfo and hicolor icons under a prefix |
| `arch/PKGBUILD` | `cadence-git` package for Arch Linux |
| `appimage/build-appimage.sh` | Builds a portable AppImage with libmpv bundled |
| `appimage/build-libmpv.sh` | Builds libmpv from a pinned source for bases whose mpv is older than 0.38 |
| `macos/build-app.sh` | Builds `Cadence.app` (and a zip of it) with libmpv bundled |
| `macos/Info.plist` | Bundle metadata, including the local network and Bonjour declarations |
| `../../.github/workflows/release.yml` | Builds, verifies and publishes a release when a `v*` tag is pushed |

The app id is `io.github.douglasmarq.cadence`. The binary sets it as the Wayland app id and X11
`WM_CLASS`, and reports it as the MPRIS `DesktopEntry`, so the window, the media controls and
the menu entry are matched to the same icon. The icon name is the app id too.

## Install for the current user

```sh
rust/scripts/install.sh              # builds and installs into ~/.local
PREFIX=/opt/cadence rust/scripts/install.sh
rust/scripts/install.sh --uninstall
```

It reuses `install-desktop-integration.sh`, pointing `Exec=` at the installed binary.

## Arch Linux package

```sh
cd rust/packaging/arch
makepkg -si
```

The package builds the workspace from `https://github.com/DouglasMarq/cadence`, branch
`main`, installs `/usr/bin/cadence`, the desktop entry, the metainfo and the icons
from `assets/icons`, depends on `mpv` and the libraries winit opens at runtime (Wayland, X11,
xkbcommon, EGL) plus any `ttf-font`.

### Building from a local commit

`CADENCE_SOURCE` replaces the source URL, which is how the package is tested before a commit
is pushed. makepkg clones with `git clone --mirror`, so point it at a bare clone (a git
worktree directory cannot be mounted into a container on its own):

```sh
git clone --bare --single-branch -b "$(git branch --show-current)" . /tmp/cadence.git
commit="$(git rev-parse HEAD)"
CADENCE_SOURCE="git+file:///tmp/cadence.git#commit=$commit" makepkg
```

To build in a clean Arch container as a non-root user (`rust/packaging/arch` has no other
files, so the PKGBUILD is read straight from the commit):

```sh
podman run --rm -it -e commit="$commit" -v /tmp/cadence.git:/src/cadence.git:ro,Z \
    docker.io/library/archlinux bash
pacman -Syu --noconfirm --needed base-devel git
useradd -m builder
su builder -c "git config --global --add safe.directory '*'"
su builder -c "git --git-dir=/src/cadence.git show $commit:rust/packaging/arch/PKGBUILD > ~/PKGBUILD"
pacman -S --noconfirm --needed $(source ~builder/PKGBUILD && echo "${depends[@]} ${makedepends[@]}")
su builder -c "cd ~ && CADENCE_SOURCE='git+file:///src/cadence.git#commit=$commit' makepkg"
pacman -U --noconfirm ~builder/cadence-git-*.pkg.tar.zst
```

## AppImage

```sh
rust/packaging/appimage/build-appimage.sh            # writes rust/target/appimage/Cadence-<version>-x86_64.AppImage
rust/packaging/appimage/build-appimage.sh ~/out      # another output directory
```

The script builds the release binary, installs it and the desktop integration into an
AppDir, lets `linuxdeploy` bundle `libmpv` and every library it links (ffmpeg, libass,
libplacebo, PipeWire, PulseAudio, ...) and packs the result with `appimagetool` and the AppImage
type 2 runtime. Pinned releases of the three are downloaded into a temporary directory that is
removed afterwards, and the build stops if a download does not match its SHA-256 checksum. The
tools run with `APPIMAGE_EXTRACT_AND_RUN=1`, so FUSE is not needed while building.

| Variable | Effect |
| --- | --- |
| `CADENCE_BINARY` | Package this binary instead of running `cargo build` |
| `LINUXDEPLOY` | Path to a linuxdeploy executable |
| `APPIMAGETOOL` | Path to an appimagetool executable |
| `APPIMAGE_RUNTIME` | Path to an AppImage type 2 runtime file |

What is bundled and what is not:

- `libmpv` and everything it links (ffmpeg, libass, libplacebo, libpulse, ...) are bundled.
- `libjack` and `libpipewire` are on the AppImage exclude list but are bundled anyway: libmpv
  links them, so a system without JACK or PipeWire could not start the AppImage otherwise. The
  bundled PipeWire client only finds its plugins on Debian/Ubuntu library layouts; elsewhere it
  prints a `pw.loop ... can't make support.system handle` line and mpv falls back to the
  PulseAudio protocol, which `pipewire-pulse` serves.
- Taken from the system: glibc, `libstdc++`, `libgcc_s`, GL/EGL drivers, X11/xcb, ALSA,
  fontconfig, freetype and the rest of the default exclude list, plus `libxkbcommon` and the
  `libwayland-*` libraries. winit opens `libxkbcommon-x11` from the system with `dlopen`, and it
  must match `libxkbcommon` exactly (a bundled copy crashed on exit).
- At least one system font is required; every desktop has one.

An AppImage only runs on systems whose glibc is at least as new as the one it was built
against, so build release AppImages on an old, stable base rather than on a rolling
distribution (a CachyOS build would even require an x86-64-v3 CPU). Ubuntu 24.04 (glibc 2.39)
is the base the script was verified on. From a full checkout
(for a git worktree, mount a bare clone as in the Arch section instead):

```sh
mkdir -p out
podman run --rm -it -v "$PWD":/src:ro,Z -v "$PWD/out":/out:Z docker.io/library/ubuntu:24.04 bash
apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git \
    build-essential pkg-config file libmpv-dev libfontconfig1-dev desktop-file-utils appstream
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal && . ~/.cargo/env
git config --global --add safe.directory '*'
git clone /src /build && bash /build/rust/packaging/appimage/build-appimage.sh /out
```

Ubuntu 24.04's `libmpv-dev` is mpv 0.37, which has no `volume-gain` property, so an AppImage built
as above, like any install from source against mpv older than 0.38, stops playing at the first
crossfade. Release builds run `appimage/build-libmpv.sh` first: it installs mpv 0.41 from a
checksummed source into `/usr/local`, and fails if an audio output, Lua or libavdevice is missing
rather than leaving it out. `.github/workflows/release.yml` lists the packages it needs.

The result is about 106 MB. Run it with `chmod +x Cadence-*.AppImage && ./Cadence-*.AppImage`;
on systems without FUSE use `./Cadence-*.AppImage --appimage-extract-and-run`. A headless smoke
test that expects exit status 0 (the app saves its state and quits on `SIGTERM`):

```sh
xvfb-run -a dbus-run-session -- timeout --preserve-status -s TERM 5 ./Cadence-*.AppImage --demo
echo $?
```

Run it through FUSE or on an extracted AppImage (`--appimage-extract`, then
`squashfs-root/AppRun`): in extract-and-run mode the AppImage runtime does not forward
`SIGTERM` to the app.

## macOS app

```sh
brew install mpv dylibbundler
rust/packaging/macos/build-app.sh
```

The script builds the release binary, writes `rust/target/macos/Cadence.app` with the version
from `Cargo.toml`, makes the icon from `assets/icons/512x512.png`, copies libmpv and every
Homebrew library it links into `Contents/Frameworks` with `dylibbundler`, drops the duplicate
`LC_RPATH` entries that make dyld refuse to load libmpv, signs the bundle ad hoc and zips it as
`Cadence-<version>-macos-<arch>.zip`. `OUT_DIR` changes the output folder.

The bundle is not notarized: on another Mac, open it once from Finder with right-click > Open,
or allow it in System Settings > Privacy & Security. yt-dlp is not bundled; the app finds it in
`/opt/homebrew/bin` or `/usr/local/bin` even when started from Finder.

## Cutting a release

A release is made by pushing a tag. The tag must be `v` plus the version in `rust/Cargo.toml`:

```sh
# 1. Set version = "1.2.3" under [workspace.package] in rust/Cargo.toml
cargo update --manifest-path rust/Cargo.toml --workspace
git commit -am "chore: release 1.2.3"
git push origin main

# 2. Tag that commit
git tag v1.2.3
git push origin v1.2.3
```

`.github/workflows/release.yml` then runs these jobs:

1. `tag` fails at once if the tag is not the workspace version.
2. `linux` (`ubuntu-24.04`) and `macos` (`macos-15`) each run clippy and the tests, then build
   `Cadence-<version>-x86_64.AppImage` and `Cadence-<version>-macos-arm64.zip`. The test step is
   limited to 30 minutes, so a test that hangs fails the job instead of using up all of it.
3. `publish` runs only if both succeeded. It checks that both assets exist, writes `SHA256SUMS`,
   creates a draft release with every asset and generated notes, and only then makes it visible.
   Drafts are invisible to `/releases/latest`, so clients never see a partial release.

If `publish` fails after creating the draft, delete the draft with
`gh release delete v1.2.3 --yes` (this keeps the tag) and re-run the failed job.

Runner choices:

- `ubuntu-24.04` (glibc 2.39) keeps the AppImage running on Ubuntu 24.04 and newer; `ubuntu-26.04`
  (glibc 2.43) would not, and `ubuntu-latest` is moving there, so the label is pinned. It is also
  the oldest base that can work: `libmpv2` needs libmpv API 2, which is mpv 0.35 or newer, and
  `ubuntu-22.04` ships mpv 0.34 (it is also deprecated and will be removed in April 2027). Its own
  mpv 0.37 is too old for crossfade, so the job builds libmpv with `appimage/build-libmpv.sh`.
- `macos-15` is the oldest arm64 macOS runner still supported. Homebrew's libraries are built for
  the runner's macOS, so releases are only tested on macOS 15 and newer, even though `Info.plist`
  declares 11.0.

## How updates work

About five seconds after launch the app asks the GitHub API for the latest release, once per
launch and off the UI thread. No release, an older or equal version, no asset for this platform, a
rate limit or a network error all end there: they are logged at debug level and nothing is
shown. Demo mode does not check.

When a newer version has an asset for this platform and a `SHA256SUMS`, a small prompt offers it.
The app never restarts on its own, and "Later" hides the prompt until the next launch. What the
prompt offers depends on how the app was installed:

| Install | Detected by | Update |
| --- | --- | --- |
| macOS `.app` | The running executable is `*.app/Contents/MacOS/*` and the folder holding the bundle is writable | Replaces the bundle and relaunches |
| AppImage | `$APPIMAGE` is set, the running executable is inside `$APPDIR` and the folder holding the file is writable | Replaces the file and relaunches |
| Anything else (`install.sh`, the Arch package, `cargo run`, a read-only location) | | "Open release page" in the browser |

`$APPDIR` is checked because `$APPIMAGE` is inherited by child processes: a source build started
from a terminal inside another AppImage must not overwrite that AppImage.

"Update and restart" works in a hidden `.cadence-update` folder next to the installed app, so
that the final renames stay on one volume:

1. Download over HTTPS only, also across redirects, and verify the SHA-256 against `SHA256SUMS`.
2. On macOS, extract the zip with `ditto -x -k`, which keeps symlinks and the signature, and check
   that the executable exists and that `Info.plist` has the release's version.
3. Move the new version into place. On macOS the old bundle is moved aside first and moved back if
   the new one cannot be placed. An AppImage is replaced by a single rename, keeping its mode.
4. Quit the normal way, which saves the queue, then relaunch.

A failure at any step removes the staging folder, leaves the installed app untouched and shows the
reason in the status message. If putting the old bundle back fails too, the staging folder is
kept and the message says where the old bundle is.

The relaunch is a small detached `sh` that waits for the old process to exit, because the
single-instance lock is only released then: started any earlier, the new version would just raise
the old window and exit. It gives up waiting after about ten seconds and starts anyway.

Known limits of an ad hoc signature on macOS: each build has a new code identity, so after an update
the system may ask again for access to the saved passwords in the keychain and to the local
network, and an update of an app in `/Applications` can be refused by the App Management privacy
setting. A refusal is reported in the status message and the installed app keeps working.
