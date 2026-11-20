//! Tasks without its screens, for Android and the Mac (UniFFI) and Linux
//! (directly): the smart views and lists as the screens show them, quick
//! add, what is coming up (alerts), and moving lists of old versions. Its
//! data lives in UUOB (uuob-client); the rules are the Svelte app's
//! (app/Tasks.svelte), so both versions read and write the same records.
//!
//! Records go in and out as JSON text, as UUOB's API has them:
//!   task  `tasks` `{ title, notes, done, due: "YYYY-MM-DD"|null, time: "HH:MM"|null, created, done_at }`
//!   list  a container of its own, with `list/info` `{ color, created }`
//!   prefs `prefs/alerts` `{ on }` in the app's first container

use std::sync::Arc;

use serde_json::{Map, Value as Json, json};
use uuob_client::dates;
use uuob_client::ffi::UuobClient;

// The SDK is part of this library, so Kotlin and Swift get both.
pub use uuob_client;

uniffi::setup_scaffolding!();

/// The colours a list can have, in the order new lists take them.
pub const COLORS: [&str; 8] = ["blue", "red", "orange", "yellow", "green", "teal", "purple", "pink"];

/// How far ahead `upcoming` looks, in days.
const DAYS: i64 = 14;

#[uniffi::export]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

/// The colour a new list starts with, when there are `lists` already.
#[uniffi::export]
pub fn next_color(lists: u32) -> String {
    COLORS[lists as usize % COLORS.len()].into()
}

fn parse(s: &str) -> Json {
    serde_json::from_str(s).unwrap_or(Json::Null)
}

fn arr(v: &Json) -> Vec<Json> {
    v.as_array().cloned().unwrap_or_default()
}

fn str_of<'a>(v: &'a Json, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Json::as_str)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// What the app's data is, as read from UUOB.
struct Data {
    /// Every task, each with the container it is in and `list` (null in the first container).
    tasks: Vec<Json>,
    /// Every list: its container merged with its info, oldest first.
    lists: Vec<Json>,
    containers: Vec<Json>,
}

impl Data {
    /// `tasks`: `all("tasks")` (each with its container); `containers`; `infos`: `all("list")`.
    fn new(tasks: &Json, containers: &Json, infos: &Json) -> Data {
        let containers = arr(containers);
        let main = containers.iter().find(|c| c["main"] == json!(true)).and_then(|c| str_of(c, "id")).unwrap_or("tasks").to_string();
        let infos = arr(infos);
        let mut lists: Vec<Json> = containers
            .iter()
            .filter(|c| c["main"] != json!(true))
            .map(|c| {
                let mut l = Map::new();
                l.insert("created".into(), json!(0));
                l.insert("color".into(), json!("blue"));
                if let Some(Json::Object(i)) = infos.iter().find(|i| i["container"] == c["id"]) {
                    l.extend(i.clone());
                }
                if let Json::Object(c) = c {
                    l.extend(c.clone());
                }
                Json::Object(l)
            })
            .collect();
        lists.sort_by_key(|l| l["created"].as_i64().unwrap_or(0));
        let tasks = arr(tasks)
            .into_iter()
            .map(|mut x| {
                let list = if str_of(&x, "container") == Some(main.as_str()) { Json::Null } else { x["container"].clone() };
                if let Some(o) = x.as_object_mut() {
                    o.insert("list".into(), list);
                }
                x
            })
            .collect();
        Data { tasks, lists, containers }
    }

    fn list_of(&self, x: &Json) -> Option<&Json> {
        x["list"].as_str().and_then(|id| self.lists.iter().find(|l| l["id"] == json!(id)))
    }

    fn can_edit(&self, x: &Json) -> bool {
        self.containers.iter().find(|c| c["id"] == x["container"]).is_none_or(|c| c["canWrite"] != json!(false))
    }
}

fn open(x: &Json) -> bool {
    x["done"] != json!(true)
}

fn due(x: &Json) -> Option<&str> {
    str_of(x, "due").filter(|d| !d.is_empty())
}

/// The four smart views.
const SMART: [(&str, &str, &str, &str); 4] = [
    ("today", "Today", "sun", "blue"),
    ("upcoming", "Upcoming", "calendar", "red"),
    ("all", "All", "inbox", "gray"),
    ("done", "Completed", "check", "green"),
];

fn smart_filter(id: &str, x: &Json, t: &str) -> bool {
    match id {
        "today" => open(x) && due(x).is_some_and(|d| d <= t),
        "upcoming" => open(x) && due(x).is_some_and(|d| d > t),
        "all" => open(x),
        _ => !open(x),
    }
}

/// "From Ana · view only", "Shared with Ana, Ben", or "".
fn shared_note(l: &Json) -> String {
    if let Some(from) = l.get("from").filter(|f| f.is_object()) {
        let name = str_of(from, "name").unwrap_or("");
        return format!("From {name}{}", if l["canWrite"] == json!(true) { "" } else { " · view only" });
    }
    let with: Vec<&str> = l["sharedWith"].as_array().into_iter().flatten().filter_map(|p| str_of(p, "name")).collect();
    if with.is_empty() { String::new() } else { format!("Shared with {}", with.join(", ")) }
}

/// The whole screen, from what UUOB holds. `state`:
/// `{ tasks, containers, infos, view, finishing: [ids], hour12 }`.
/// Answers `{ smart, lists, current, groups, count, empty }`: everything a
/// screen draws, with each task as stored plus what to show (`show`).
#[uniffi::export]
pub fn screen(state: String) -> String {
    screen_on(&parse(&state), &dates::today()).to_string()
}

pub fn screen_on(s: &Json, t: &str) -> Json {
    let d = Data::new(&s["tasks"], &s["containers"], &s["infos"]);
    let view = str_of(s, "view").unwrap_or("today");
    let hour12 = s["hour12"] == json!(true);
    let finishing: Vec<&str> = s["finishing"].as_array().into_iter().flatten().filter_map(Json::as_str).collect();

    let smart: Vec<Json> = SMART
        .iter()
        .map(|(id, name, icon, color)| {
            let count = d.tasks.iter().filter(|x| smart_filter(id, x, t)).count();
            json!({ "id": id, "name": name, "icon": icon, "color": color, "count": count })
        })
        .collect();
    let lists: Vec<Json> = d
        .lists
        .iter()
        .map(|l| {
            let mut l = l.clone();
            let id = l["id"].clone();
            let count = d.tasks.iter().filter(|x| x["list"] == id && open(x)).count();
            let shared = l.get("from").is_some_and(Json::is_object) || l["sharedWith"].as_array().is_some_and(|a| !a.is_empty());
            l["count"] = json!(count);
            l["shared"] = json!(shared);
            l["note"] = json!(shared_note(&l));
            l
        })
        .collect();

    // The view shown: a smart view, a list, or Today when the list is gone.
    let list = lists.iter().find(|l| l["id"] == json!(view));
    let (cur_id, filter): (String, Box<dyn Fn(&Json) -> bool>) = match (SMART.iter().find(|s| s.0 == view), list) {
        (Some(sm), _) => (sm.0.into(), Box::new(move |x: &Json| smart_filter(sm.0, x, t))),
        (None, Some(l)) => {
            let id = l["id"].clone();
            (view.into(), Box::new(move |x: &Json| x["list"] == id && open(x)))
        }
        _ => ("today".into(), Box::new(move |x: &Json| smart_filter("today", x, t))),
    };
    let current = match list.filter(|_| cur_id == view && !SMART.iter().any(|s| s.0 == view)) {
        Some(l) => json!({
            "id": l["id"], "name": l["name"], "color": l["color"], "list": l,
            "canAdd": l["canWrite"] != json!(false),
        }),
        None => {
            let sm = SMART.iter().find(|s| s.0 == cur_id).unwrap();
            json!({ "id": sm.0, "name": sm.1, "color": sm.3, "list": null, "canAdd": sm.0 != "done" })
        }
    };
    let current_list = current["list"]["id"].clone();

    let mut visible: Vec<&Json> =
        d.tasks.iter().filter(|x| filter(x) || str_of(x, "id").is_some_and(|id| finishing.contains(&id))).collect();
    if cur_id == "done" {
        visible.sort_by_key(|x| std::cmp::Reverse(x["done_at"].as_i64().unwrap_or(0)));
    } else {
        visible.sort_by(|a, b| {
            let key = |x: &Json| (due(x).unwrap_or("9999").to_string(), str_of(x, "time").unwrap_or("99").to_string());
            key(a).cmp(&key(b)).then(a["created"].as_i64().unwrap_or(0).cmp(&b["created"].as_i64().unwrap_or(0)))
        });
    }

    let item = |x: &Json| {
        let list = d.list_of(x);
        let when = due(x).map(|dd| match str_of(x, "time").filter(|t| !t.is_empty()) {
            Some(tm) => format!("{}, {}", dates::friendly_on(dd, t), dates::friendly_time(tm, hour12)),
            None => dates::friendly_on(dd, t),
        });
        let other_list = list.filter(|_| current_list != x["list"]);
        let mut x = x.clone();
        x["show"] = json!({
            "canEdit": d.can_edit(&x),
            "ring": list.map(|l| l["color"].clone()),
            "when": when,
            "late": open(&x) && due(&x).is_some_and(|dd| dd < t),
            "listName": other_list.map(|l| l["name"].clone()),
            "listColor": other_list.map(|l| l["color"].clone()),
            "finishing": str_of(&x, "id").is_some_and(|id| finishing.contains(&id)),
        });
        x
    };

    // Upcoming is grouped by day, like Reminders.
    let groups: Vec<Json> = if cur_id == "upcoming" {
        let mut out: Vec<(String, Vec<Json>)> = vec![];
        for x in &visible {
            let key = due(x).unwrap_or("").to_string();
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some((_, items)) => items.push(item(x)),
                None => out.push((key, vec![item(x)])),
            }
        }
        out.into_iter().map(|(k, items)| json!({ "key": dates::friendly_on(&k, t), "items": items })).collect()
    } else {
        vec![json!({ "key": "", "items": visible.iter().map(|x| item(x)).collect::<Vec<_>>() })]
    };

    let empty = visible.is_empty().then(|| {
        let done = cur_id == "done";
        json!({
            "glyph": if done { "🏁" } else if cur_id == "today" { "☀️" } else { "✨" },
            "headline": if done { "Nothing completed yet" } else { "All clear" },
            "caption": if done { "Finished tasks show up here." } else { "Add a task above. Dates like “tomorrow 9am” just work." },
        })
    });

    json!({ "smart": smart, "lists": lists, "current": current, "groups": groups, "count": visible.len(), "empty": empty })
}

/// A new task from quick-add text, in `view` (in Today, it is due today
/// unless the text says otherwise). The record to put; null for empty text.
#[uniffi::export]
pub fn quick_task(text: String, view: String) -> String {
    quick_task_on(&text, &view, &dates::today(), now_ms()).to_string()
}

pub fn quick_task_on(text: &str, view: &str, t: &str, now: i64) -> Json {
    if text.trim().is_empty() {
        return Json::Null;
    }
    let q = dates::parse_quick_on(text, t, &dates::now_time());
    let due = q.due.or_else(|| (view == "today").then(|| t.to_string()));
    let title = if q.title.is_empty() { "New task".to_string() } else { q.title };
    json!({ "title": title, "notes": "", "done": false, "due": due, "time": q.time, "created": now })
}

/// What quick-add text will be due, while it is typed: "Friday, 17:00", or null.
#[uniffi::export]
pub fn quick_preview(text: String, hour12: bool) -> Option<String> {
    let q = dates::parse_quick(&text);
    let due = q.due?;
    Some(match q.time {
        Some(tm) => format!("{}, {}", dates::friendly(&due), dates::friendly_time(&tm, hour12)),
        None => dates::friendly(&due),
    })
}

/// The task checked or unchecked.
#[uniffi::export]
pub fn toggled(task: String) -> String {
    let mut x = parse(&task);
    let done = x["done"] != json!(true);
    x["done"] = json!(done);
    x["done_at"] = if done { json!(now_ms()) } else { Json::Null };
    x.to_string()
}

/// What is saved of a task: the record without what the app adds (its
/// container, list and what to show). Fields this version doesn't know stay.
#[uniffi::export]
pub fn stored(task: String) -> String {
    stored_of(parse(&task)).to_string()
}

pub fn stored_of(mut x: Json) -> Json {
    if let Some(o) = x.as_object_mut() {
        for k in ["container", "list", "show"] {
            o.remove(k);
        }
    }
    x
}

/// Alerts at a task's time: on unless turned off (`prefs` records).
#[uniffi::export]
pub fn alerts_on(prefs: String) -> bool {
    arr(&parse(&prefs)).iter().find(|r| r["id"] == json!("alerts")).is_none_or(|r| r["on"] != json!(false))
}

/// What is coming up, for alerts on the user's devices: each open task due
/// by two weeks from now (also those already late), with an alert at its
/// time if it has one and alerts are on. `state`: `{ tasks, containers, infos, alerts }`.
#[uniffi::export]
pub fn upcoming(state: String) -> String {
    upcoming_on(&parse(&state), &dates::today()).to_string()
}

pub fn upcoming_on(s: &Json, t: &str) -> Json {
    let d = Data::new(&s["tasks"], &s["containers"], &s["infos"]);
    let alerts = s["alerts"] != json!(false);
    let last = dates::add_days(t, DAYS);
    let items: Vec<Json> = d
        .tasks
        .iter()
        .filter(|x| open(x) && due(x).is_some_and(|dd| dd <= last.as_str()))
        .map(|x| {
            let list = d.list_of(x);
            let mut o = Map::new();
            o.insert("id".into(), json!(format!("{}~{}", str_of(x, "container").unwrap_or(""), str_of(x, "id").unwrap_or(""))));
            o.insert("date".into(), json!(due(x)));
            if let Some(tm) = str_of(x, "time").filter(|t| !t.is_empty()) {
                o.insert("time".into(), json!(tm));
                if alerts {
                    o.insert("alert".into(), json!(0));
                }
            }
            let title = str_of(x, "title").map(str::trim).filter(|t| !t.is_empty()).unwrap_or("Task");
            o.insert("title".into(), json!(title));
            if let Some(name) = list.and_then(|l| str_of(l, "name")).filter(|n| !n.is_empty()) {
                o.insert("body".into(), json!(name));
            }
            if let Some(c) = list.map(|l| l["color"].clone()).filter(|c| !c.is_null()) {
                o.insert("color".into(), c);
            }
            Json::Object(o)
        })
        .collect();
    Json::Array(items)
}

/// Lists used to be records in the app's first container. Moves each one
/// into a container of its own (keyed by its old id, so every device ends
/// up with the same container), with its tasks.
#[uniffi::export]
pub async fn move_old_lists(uuob: Arc<UuobClient>) -> Result<(), uuob_client::Error> {
    move_old_lists_with(uuob.inner().clone()).await
}

/// [`move_old_lists`], for Rust (Linux).
pub async fn move_old_lists_with(c: uuob_client::Client) -> Result<(), uuob_client::Error> {
    uuob_client::runtime()
        .spawn(async move {
            let old = c.list("lists", None).await?;
            if old.is_empty() {
                return Ok(());
            }
            let loose = c.list("tasks", None).await?;
            for l in old {
                let name = str_of(&l, "name").filter(|n| !n.is_empty()).unwrap_or("List").to_string();
                let key = str_of(&l, "id").unwrap_or("");
                let made = match c.create_container(&name, Some(key)).await {
                    Ok(v) => v,
                    Err(_) => c.create_container(&name, None).await?,
                };
                let list = made.as_str().map(String::from).unwrap_or_else(|| made.to_string());
                let info = json!({ "color": l.get("color").filter(|c| !c.is_null()).cloned().unwrap_or(json!("blue")),
                                   "created": l.get("created").filter(|c| !c.is_null()).cloned().unwrap_or(json!(0)) });
                c.put("list", Some(&list), Some("info"), info).await?;
                for x in loose.iter().filter(|x| x["list"] == l["id"]) {
                    let mut x = x.clone();
                    x.as_object_mut().map(|o| o.remove("list"));
                    let id = str_of(&x, "id").unwrap_or("").to_string();
                    c.put("tasks", Some(&list), None, x).await?;
                    c.delete("tasks", None, &id).await?;
                }
                c.delete("lists", None, key).await?;
            }
            Ok(())
        })
        .await
        .unwrap_or(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "2026-11-19";

    fn state(view: &str) -> Json {
        json!({
            "view": view, "hour12": false, "finishing": [],
            "containers": [
                { "id": "tasks", "name": "Tasks", "main": true, "canWrite": true },
                { "id": "tasks.home", "name": "Home", "main": false, "canWrite": true, "sharedWith": [{ "name": "Ana" }] },
                { "id": "tasks.club", "name": "Club", "main": false, "canWrite": false, "from": { "name": "Ben" } },
            ],
            "infos": [
                { "id": "info", "container": "tasks.home", "color": "green", "created": 2 },
                { "id": "info", "container": "tasks.club", "color": "pink", "created": 1 },
            ],
            "tasks": [
                { "id": "a", "container": "tasks", "title": "Late", "done": false, "due": "2026-11-18", "time": null, "created": 1 },
                { "id": "b", "container": "tasks.home", "title": "Today 9", "done": false, "due": T, "time": "09:00", "created": 2, "extra": 7 },
                { "id": "c", "container": "tasks.home", "title": "Fri", "done": false, "due": "2026-11-20", "created": 3 },
                { "id": "d", "container": "tasks.club", "title": "Fri too", "done": false, "due": "2026-11-20", "time": "08:00", "created": 4 },
                { "id": "e", "container": "tasks", "title": "Undated", "done": false, "due": null, "created": 5 },
                { "id": "f", "container": "tasks", "title": "Done", "done": true, "done_at": 10, "created": 6 },
                { "id": "g", "container": "tasks", "title": "  ", "done": false, "due": "2026-12-31", "created": 7 },
            ],
        })
    }

    fn titles(s: &Json) -> Vec<String> {
        s["groups"].as_array().unwrap().iter().flat_map(|g| g["items"].as_array().unwrap().iter().map(|x| x["title"].as_str().unwrap().to_string())).collect()
    }

    #[test]
    fn smart_views_count_and_sort() {
        let s = screen_on(&state("today"), T);
        let counts: Vec<i64> = s["smart"].as_array().unwrap().iter().map(|v| v["count"].as_i64().unwrap()).collect();
        assert_eq!(counts, [2, 3, 6, 1]);
        assert_eq!(titles(&s), ["Late", "Today 9"]);
        let late = &s["groups"][0]["items"][0]["show"];
        assert_eq!(late["late"], json!(true));
        assert_eq!(late["when"], json!("Yesterday"));
        assert_eq!(s["groups"][0]["items"][1]["show"]["when"], json!("Today, 9:00"));
        assert_eq!(s["groups"][0]["items"][1]["show"]["listName"], json!("Home"));

        let up = screen_on(&state("upcoming"), T);
        let keys: Vec<&str> = up["groups"].as_array().unwrap().iter().map(|g| g["key"].as_str().unwrap()).collect();
        assert_eq!(keys, ["Tomorrow", "Thu 31 Dec"]);
        assert_eq!(titles(&up), ["Fri too", "Fri", "  "]);
        assert_eq!(titles(&screen_on(&state("all"), T)), ["Late", "Today 9", "Fri too", "Fri", "  ", "Undated"]);
        let done = screen_on(&state("done"), T);
        assert_eq!(titles(&done), ["Done"]);
        assert_eq!(done["current"]["canAdd"], json!(false));
    }

    #[test]
    fn lists_shared_and_view_only() {
        let s = screen_on(&state("tasks.club"), T);
        let names: Vec<&str> = s["lists"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["Club", "Home"]); // oldest first
        assert_eq!(s["lists"][0]["note"], json!("From Ben · view only"));
        assert_eq!(s["lists"][1]["note"], json!("Shared with Ana"));
        assert_eq!(s["current"]["name"], json!("Club"));
        assert_eq!(s["current"]["canAdd"], json!(false));
        assert_eq!(titles(&s), ["Fri too"]);
        assert_eq!(s["groups"][0]["items"][0]["show"]["canEdit"], json!(false));
        assert_eq!(s["groups"][0]["items"][0]["show"]["listName"], Json::Null); // in its own list
        // A list that is gone shows Today.
        assert_eq!(screen_on(&state("tasks.gone"), T)["current"]["id"], json!("today"));
    }

    #[test]
    fn finishing_tasks_stay_until_the_animation_ends() {
        let mut st = state("today");
        st["tasks"][0]["done"] = json!(true);
        assert_eq!(titles(&screen_on(&st, T)), ["Today 9"]);
        st["finishing"] = json!(["a"]);
        assert_eq!(titles(&screen_on(&st, T)), ["Late", "Today 9"]);
    }

    #[test]
    fn quick_add_and_saving_keep_the_record() {
        let x = quick_task_on("call mom tomorrow 5pm", "all", T, 99);
        assert_eq!(x, json!({ "title": "call mom", "notes": "", "done": false, "due": "2026-11-20", "time": "17:00", "created": 99 }));
        assert_eq!(quick_task_on("buy milk", "today", T, 1)["due"], json!(T));
        assert_eq!(quick_task_on("buy milk", "tasks.home", T, 1)["due"], Json::Null);
        assert_eq!(quick_task_on("tomorrow", "all", T, 1)["title"], json!("New task"));
        assert_eq!(quick_task_on("  ", "all", T, 1), Json::Null);

        let s = screen_on(&state("today"), T);
        let shown = s["groups"][0]["items"][1].clone();
        let saved = stored_of(shown);
        assert_eq!(saved, json!({ "id": "b", "title": "Today 9", "done": false, "due": T, "time": "09:00", "created": 2, "extra": 7 }));
        let t: Json = parse(&toggled(saved.to_string()));
        assert_eq!(t["done"], json!(true));
        assert!(t["done_at"].as_i64().unwrap() > 0);
        assert_eq!(parse(&toggled(t.to_string()))["done_at"], Json::Null);
    }

    #[test]
    fn upcoming_as_the_web_version_writes_it() {
        let mut st = state("today");
        st["alerts"] = json!(true);
        let up = upcoming_on(&st, T);
        assert_eq!(
            up,
            json!([
                { "id": "tasks~a", "date": "2026-11-18", "title": "Late" },
                { "id": "tasks.home~b", "date": T, "time": "09:00", "alert": 0, "title": "Today 9", "body": "Home", "color": "green" },
                { "id": "tasks.home~c", "date": "2026-11-20", "title": "Fri", "body": "Home", "color": "green" },
                { "id": "tasks.club~d", "date": "2026-11-20", "time": "08:00", "alert": 0, "title": "Fri too", "body": "Club", "color": "pink" },
            ])
        );
        st["alerts"] = json!(false);
        assert_eq!(upcoming_on(&st, T)[1].get("alert"), None);
        assert!(alerts_on("[]".into()));
        assert!(!alerts_on(r#"[{"id":"alerts","on":false}]"#.into()));
        assert_eq!(next_color(9), "red");
    }
}
