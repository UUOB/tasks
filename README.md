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

```
app/             the app (Svelte) and its shell (Tauri v2, in app/src-tauri)
uuob/            UUOB's repository, as a git submodule: the app SDK (uuob/ui/uuob.js and
                 uuob/crates/uuob-sdk), the connect screen and design tokens (uuob/ui),
                 in-app updates (uuob/crates/uuob-update) and the build scripts (uuob/docker)
```

```bash
git clone --recurse-submodules https://github.com/UUOB/tasks.git && cd tasks
```

Everything runs in UUOB's dev container (`uuob/docker/dev.sh` builds it on first use):

```bash
uuob/docker/dev.sh sh -c 'npm ci && npm run build'   # the front end
uuob/docker/dev.sh cargo build                       # the desktop app (Linux)
uuob/docker/android.sh uuob/docker/apk.sh            # a signed APK for arm64 phones, in dist/
gh workflow run mac.yml                              # the Mac app, built on GitHub
```

To publish a new version, raise `version` in `app/src-tauri/tauri.conf.json`, commit, push and run
`uuob/docker/release.sh` (see [UUOB for developers](https://github.com/UUOB/uuob/blob/main/docs/developers.md), *Updates*).

To use a newer UUOB SDK: `git -C uuob pull origin main`, then commit the `uuob` folder.

## License

[GNU Affero General Public License, version 3 or later](LICENSE).
