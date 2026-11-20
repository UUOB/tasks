# Tasks

**Simple to-do lists, kept on your own devices.**

**[Download](https://github.com/jachymri/uuob-releases)** for Android and Mac · Built for **[UUOB](https://github.com/UUOB/uuob)**

## Built for UUOB

Tasks has no server and no account. Your tasks live in [UUOB](https://github.com/UUOB/uuob), on your own
devices. They are end-to-end encrypted, so only your devices (and people you share with) can read them, and
UUOB keeps them in sync.

1. Install UUOB, then Tasks.
2. Open Tasks. UUOB asks *"Tasks wants to use your network"*. Tap **Allow**.
3. Your tasks are now on all your linked devices.

No UUOB on this device? In UUOB on another device choose **Apps → Connect with a code**, and type the code
into Tasks. [Full setup guide →](https://github.com/UUOB/uuob/blob/main/docs/setup.md)

## Features

- **Lists**: Home, Work, Groceries, each with its own colour.
- **Today, Upcoming, All, Completed**
- **Quick add**: type "call mom fri 5pm" and the date and time are set for you.
- **Dates, times and notes**
- **Alerts** at a task's time, on your phone and computer.
- **Share a list** with someone, so they can view it or edit it.
- **Shows in Calendar**, if you allow it.
- **Up next widget** on Android, together with your events.
- **Works offline**: changes are sent when UUOB is reachable again.

## For developers

Tasks is native on each system, over one Rust core, like UUOB itself:

```
core/            Tasks without its screens (Rust): its data, rules and the calls to UUOB, for all three
                 systems; Swift and Kotlin reach it through UniFFI
android/         the Android app (Jetpack Compose, Material 3)
mac/             the Mac app (SwiftUI, macOS 14 or newer)
linux/           the Linux app (GTK 4 and Libadwaita 1.5, through Relm4)
sdk/             UUOB's SDK for apps, as a git submodule (UUOB-sdk): the client (sdk/client) and the
                 controls and connect screen every app shares (sdk/kit: android, mac, linux)
uuob/            UUOB's repository, as a git submodule: the build scripts (uuob/docker)
app/             the earlier web version (Svelte in Tauri), kept until the native one replaces it
```

```bash
git clone --recurse-submodules https://github.com/UUOB/tasks.git && cd tasks
```

Everything builds in UUOB's containers (`uuob/docker`; each builds its image on first use):

```bash
uuob/docker/linux.sh cargo build -p tasks-linux     # the Linux app, target/debug/uuob-tasks
uuob/docker/linux.sh cargo test                       # the core's tests
uuob/docker/android.sh uuob/docker/apk.sh             # a signed APK for arm64 phones, in dist/
gh workflow run mac.yml                               # the Mac app, built on GitHub (or mac/build.sh on a Mac)
```

To publish a new version, raise `version` in `Cargo.toml`, commit, push and run `uuob/docker/release.sh`
(see [UUOB for developers](https://github.com/UUOB/uuob/blob/main/docs/developers.md), *Updates*).

To use a newer SDK or UUOB: `git -C sdk pull origin main` (or `uuob`), then commit that folder.

## License

[GNU Affero General Public License, version 3 or later](LICENSE).
