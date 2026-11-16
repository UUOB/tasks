# Tasks

**Tasks, kept in your own UUOB.**

Tasks has no server of its own. It keeps your tasks in [UUOB](https://github.com/UUOB/uuob), on your own
devices, end-to-end encrypted, and UUOB syncs them between your devices. Install UUOB on the same device,
or use the UUOB on another of your devices: there, choose *Apps → Connect with a code* and type the code
into Tasks.

Downloads for Android and Mac: **[github.com/jachymri/uuob-releases](https://github.com/jachymri/uuob-releases)**.

## Layout

```
app/             the app (Svelte) and its shell (Tauri v2, in app/src-tauri)
uuob/            UUOB's repository, as a git submodule: the app SDK (uuob/ui/uuob.js and
                 uuob/crates/uuob-sdk), the connect screen and design tokens (uuob/ui),
                 in-app updates (uuob/crates/uuob-update) and the build scripts (uuob/docker)
```

## Develop

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
`uuob/docker/release.sh` (see UUOB's README, *Updates*).

To use a newer UUOB SDK: `git -C uuob pull origin main`, then commit the `uuob` folder.

## License

[GNU Affero General Public License, version 3 or later](LICENSE).
