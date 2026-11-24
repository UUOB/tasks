#!/usr/bin/env bash
# Screenshots of Tasks on the Mac, for checking how it looks (mac.yml):
# connected to a UUOB with made-up demo data; Today, a list, a task's card and
# editor, Settings and a click outside it, a button's hint, light and dark; then
# a small window. A button a version doesn't have is simply left out.
#   mac/shots.sh <Tasks.app> <uuob binary> <seeded data dir> <out dir>
set -uo pipefail
name=Tasks exe=$1/Contents/MacOS/Tasks UUOB=$2 DATA=$3 out=$4
mkdir -p "$out"
. "$(dirname "$0")/../sdk/kit/mac/shots-lib.sh"

mode=start
uuob_up
app_up
for mode in light dark; do
  [ $mode = dark ] && { se 'tell appearance preferences to set dark mode to true'; sleep 2; }
  click "~Today"; shot today
  click "~Work" && shot list
  click "~Send invoice" && { shot card; cmd e; shot editor; esc; esc; }
  # The side panel's foot: Settings and the toggle on one line, the toggle's hint.
  # The toggle's hint (an icon has no words to find; the window sits at 12,31, 1000 wide, 680 high).
  pointer 222 681; sleep 1; shot sidebar-hint
  cmd ","; shot settings
  # Beside the sheet, on the window it covers (the screen is 1024 wide).
  win "click at {70, 640}"; sleep 1.5; shot outside-click
  esc
done

mode=small
win "set size of front window to {520, 620}"; sleep 2
shot today
app_down
uuob_down
