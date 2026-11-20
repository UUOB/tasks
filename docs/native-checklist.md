# Tasks on native screens: what has to keep working

Taken from the Svelte app (`app/Tasks.svelte`, `uuob/ui/Settings.svelte`,
`TimeField.svelte`, `dates.js`, Android's `Alert.kt`) before it is replaced.
Ticked once checked on the new app; the note says how.
L = Linux window against a seeded UUOB; A = Android emulator with UUOB 0.4.0;
T = a test in `core/` or `uuob-client`. The Mac app is written in the web look (uuob/kit/mac/Web.swift), not yet built.

## Data (the same as the web version, so both can run side by side)
- [x] Tasks are `tasks` records `{title, notes, done, due, time, created, done_at}` (L: the record UUOB holds after a quick add; T)
- [x] Tasks in no list live in the app's first container; each list is a container of its own (L, A)
- [x] A list's colour and creation time live in its `list/info` record (L: Books, Garden; A: colour change shows in the old app)
- [x] Fields this version doesn't know survive an edit (L: `custom: 42` kept through the move; T)
- [x] Moving a task to another list: put in the new container, delete from the old (L: after editing it, all edits kept)
- [x] `prefs/alerts {on}` in the first container; on unless set (L: `{"on": false}` written; T)
- [x] Old lists move into containers keyed by their old id, with their tasks (L: `tasks.oldlist0000books`, `lists` record gone)
- [x] `upcoming` as the web version writes it (L: the list UUOB holds; T: field for field)
- [x] Old and new side by side (A: web 0.3.2 wrote a list and tasks, native 0.4.0 installed over it showed them; web put back read native edits; native again)

## Smart views
- [x] Today, Upcoming, All, Completed with counts (L, A)
- [x] Upcoming grouped by day (L: Tomorrow, Saturday, Sunday, Tuesday)
- [x] Sorted by date, time, creation; Completed newest first (T, L)
- [x] Empty states (L, A)

## Lists
- [x] My lists with colour, open count and the shared mark; hint when none (L, A)
- [x] Add list with the next colour preselected (L: yellow as 4th; A)
- [x] Edit list: rename, colour, Share list…, Delete (L: Delete; A: colour)
- [ ] A list someone shared with you (view only, Leave list): T only, needs a second person's UUOB
- [x] "Shared with A, B" under the title (L: Groceries, shared with Petra)
- [x] View-only lists can't be added to or edited (T)

## Tasks
- [x] Quick add with the date preview (L: "call mom fri 5pm" → Tomorrow, 17:00; A: "20:34" → Today, 20:34); in Today undated tasks are due today (A); in a list they go to it (L: Garden)
- [x] Checking off: fill in the list's colour, fade, gone after 0.7 s (L: frames)
- [x] Notes line, date and time (late in red), list name (L, A)
- [x] Editing title, notes, date, time ("2pm" → 14:00, "1830" → 18:30), list, Delete, Done (L, A); typed text is kept when the editor closes (A)

## Screens
- [x] Wide: sidebar and list (L)
- [x] Phone: overview, then a list, with Back (L narrow window, A)
- [x] Settings: Time for all apps (L: switching to 2:30 PM changed every time on screen; UUOB holds `hour12: true`), Alerts On/Off (L, A), About with the version (L, A)
- [x] Connect screen and Waiting for UUOB (L: code; A: Allow, and offline: a task added while UUOB was stopped shows at once and is sent when it's back)
- [ ] The Update button: not tried (no newer version published)

## Android
- [x] Alerts as Tasks' own notification, only from UUOB (A: UUOB rang "alert test" at 20:34 under Tasks' name and icon; a broadcast without `from` was ignored)
- [x] Asks for the notification permission (A)
- [x] Installs over the web version and keeps the data, asking for Allow once (A: twice on a fresh emulator the first Allow saved the key within a second, also when Tasks was reopened 2 s after it. In an earlier run the first Allow did not take and the second did; that could not be made to happen again)
