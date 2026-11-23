//! Tasks' window on Linux: GTK 4 and Libadwaita through Relm4, on Tasks'
//! core (`core/`) and UUOB's SDK. The connect screen, Settings and the time
//! field are UUOB's kit (`uuob/kit/linux`), the same in every app.

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use relm4::adw::prelude::*;
use relm4::{ComponentParts, ComponentSender, RelmApp, adw, gtk, gtk::gdk, gtk::glib};
use serde_json::{Value, json};
use tasks_core as core;
use uuob_client::{App, Client, Event, dates};
use uuob_kit_gtk::{AppInfo, Gate};

const FEED: &str = "https://raw.githubusercontent.com/jachymri/uuob-releases/main/tasks.json";

/// Where Tasks keeps its own files (its key to UUOB, what it last saw).
fn data_dir() -> PathBuf {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    std::env::var_os("XDG_DATA_HOME").map(Into::into).unwrap_or_else(|| home.join(".local/share")).join("app.uuob.tasks")
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or_default()
}

fn opt(v: &Value, k: &str) -> Option<String> {
    v[k].as_str().filter(|x| !x.is_empty()).map(String::from)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[derive(Debug)]
pub enum Msg {
    Reload,
    Go(String),
    Add(String),
    Toggle(Value),
    /// A task changed (from its editor or its tick).
    Save(Value),
    /// A task edited and moved to another list: kept as edited, then moved.
    SaveMove(Value, Option<String>),
    Remove(Value),
    Move(Value, Option<String>),
    /// The task's card, and its editor.
    Card(Value),
    Edit(Value),
    /// New task (Ctrl+N or C): the quick field, ready for typing.
    NewTask,
    Finished(String),
    NewList,
    EditList,
    SaveList(Value),
    DeleteList(String),
    Share(String),
    Settings,
    SetAlerts(bool),
    /// Write what is coming up (0: schedule it).
    Upcoming(u64),
    Redraw,
}

#[derive(Debug)]
pub enum Cmd {
    Loaded { tasks: Vec<Value>, complete: bool, containers: Option<Vec<Value>>, infos: Vec<Value>, prefs: Option<Vec<Value>> },
    Done,
}

pub struct Tasks {
    client: Client,
    info: AppInfo,
    tasks: Vec<Value>,
    containers: Vec<Value>,
    infos: Vec<Value>,
    alerts: bool,
    tasks_known: bool,
    alerts_known: bool,
    view: String,
    finishing: HashSet<String>,
    /// Just checked off: the circle fills and the row fades as it is drawn.
    animate: HashSet<String>,
    /// The screen last drawn (the core's answer), to draw only when it changed.
    last: Value,
    upcoming_gen: u64,
}

pub struct Widgets {
    /// The sidebar's views and lists, and the rail's (redrawn with the screen).
    side_rows: gtk::Box,
    rail_rows: gtk::Box,
    /// The page, tinted with the list's colour.
    page: gtk::Widget,
    title: gtk::Label,
    edit: gtk::Button,
    share: gtk::Button,
    note: gtk::Label,
    quick: gtk::Entry,
    quick_row: gtk::Box,
    groups: gtk::Box,
    /// Closes the sidebar shown over a narrow window, once a view is picked.
    close_peek: Rc<dyn Fn()>,
}

fn circle(color: &str, size: i32, icon: &str) -> gtk::Widget {
    let b = gtk::Box::builder().width_request(size).height_request(size).halign(gtk::Align::Start).valign(gtk::Align::Center).css_classes(["circle", &format!("uuob-c-{color}")]).build();
    let i = uuob_kit_gtk::icon(icon, size * 55 / 100, 2.2, "#ffffff");
    i.set_hexpand(true);
    i.set_halign(gtk::Align::Center);
    b.append(&i);
    // An explicit no, so the icon's expand doesn't spread to the row.
    b.set_hexpand(false);
    b.upcast()
}

fn icon_of(name: &str) -> &'static str {
    match name {
        "sun" => "sun",
        "calendar" => "calendar",
        "inbox" => "inbox",
        _ => "check",
    }
}

/// A circle of the list's colour with its icon, centred (for the sidebar and the rail).
fn mark(color: &str, size: i32, icon: &str) -> gtk::Widget {
    let m = circle(color, size, icon);
    m.set_halign(gtk::Align::Center);
    m
}

fn label(text: &str, classes: &[&str]) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(classes.to_vec()).build()
}

/// A key on a dialog (Ctrl+E Edit, Delete…), while it is in front.
fn dialog_key(d: &adw::Dialog, accel: &str, f: impl Fn() + 'static) {
    let keys = gtk::ShortcutController::new();
    keys.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string(accel),
        Some(gtk::CallbackAction::new(move |_, _| {
            f();
            glib::Propagation::Stop
        })),
    ));
    d.add_controller(keys);
}

impl Tasks {
    /// The task as it is now (the screens' copy may be older than an edit).
    fn current(&self, x: &Value) -> Value {
        self.tasks.iter().find(|t| t["id"] == x["id"] && t["container"] == x["container"]).cloned().unwrap_or_else(|| x.clone())
    }

    fn main_id(&self) -> String {
        self.containers.iter().find(|c| c["main"] == json!(true)).map(|c| s(c, "id").to_string()).unwrap_or_else(|| "tasks".into())
    }

    /// The container argument for the SDK: None for the app's first container.
    fn arg(&self, c: Option<&str>) -> Option<String> {
        c.filter(|c| *c != self.main_id()).map(String::from)
    }

    fn screen(&self) -> Value {
        let state = json!({
            "tasks": self.tasks, "containers": self.containers, "infos": self.infos, "view": self.view,
            "finishing": self.finishing.iter().collect::<Vec<_>>(), "hour12": uuob_kit_gtk::hour12(&self.client),
        });
        core::screen_on(&state, &dates::today())
    }

    /// What is coming up is written 1.5 s after the last change, once every
    /// list and the setting have been read.
    fn schedule_upcoming(&mut self, sender: &ComponentSender<Self>) {
        if !self.tasks_known || !self.alerts_known {
            return;
        }
        self.upcoming_gen += 1;
        let generation = self.upcoming_gen;
        let tx = sender.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || tx.input(Msg::Upcoming(generation)));
    }

    fn render(&mut self, w: &Widgets, sender: &ComponentSender<Self>, force: bool) {
        let screen = self.screen();
        if screen == self.last && !force {
            return;
        }
        if screen["lists"] != self.last["lists"] || screen["groups"] != self.last["groups"] {
            self.schedule_upcoming(sender);
        }
        self.last = screen.clone();
        self.render_sidebar(&screen, w, sender);
        self.render_content(&screen, w, sender);
        self.animate.clear();
    }

    fn render_sidebar(&self, screen: &Value, w: &Widgets, sender: &ComponentSender<Self>) {
        for b in [&w.side_rows, &w.rail_rows] {
            while let Some(c) = b.first_child() {
                b.remove(&c);
            }
        }
        let go = |b: &gtk::Button, id: &str| {
            let (tx, id, close) = (sender.clone(), id.to_string(), w.close_peek.clone());
            b.connect_clicked(move |_| {
                close();
                tx.input(Msg::Go(id.clone()));
            });
        };
        // Today, Upcoming, All, Completed: a row with the count; on the rail their circles.
        for sm in screen["smart"].as_array().into_iter().flatten() {
            let on = s(sm, "id") == self.view;
            let count = gtk::Label::new(Some(&sm["count"].to_string()));
            let r = uuob_kit_gtk::sidebar_row(Some(&mark(s(sm, "color"), 22, icon_of(s(sm, "icon")))), s(sm, "name"), Some(count.upcast_ref()), on);
            go(&r, s(sm, "id"));
            w.side_rows.append(&r);
            let rb = uuob_kit_gtk::rail_button(&mark(s(sm, "color"), 24, icon_of(s(sm, "icon"))), s(sm, "name"), on);
            go(&rb, s(sm, "id"));
            w.rail_rows.append(&rb);
        }
        let lists = screen["lists"].as_array().cloned().unwrap_or_default();
        if !lists.is_empty() {
            let l = uuob_kit_gtk::rows_label("My lists");
            l.set_margin_top(18);
            l.set_margin_start(10);
            l.set_margin_bottom(4);
            w.side_rows.append(&l);
            w.rail_rows.append(&gtk::Separator::builder().margin_start(18).margin_end(18).margin_top(4).margin_bottom(4).build());
        }
        for l in lists {
            let on = s(&l, "id") == self.view;
            let end = gtk::Box::builder().spacing(6).build();
            if l["shared"] == json!(true) {
                end.append(&uuob_kit_gtk::icon("people", 15, 1.75, uuob_kit_gtk::text2()));
            }
            end.append(&gtk::Label::new(Some(&l["count"].to_string())));
            let r = uuob_kit_gtk::sidebar_row(Some(&mark(s(&l, "color"), 22, "list")), s(&l, "name"), Some(end.upcast_ref()), on);
            go(&r, s(&l, "id"));
            w.side_rows.append(&r);
            let rb = uuob_kit_gtk::rail_button(&mark(s(&l, "color"), 22, "list"), s(&l, "name"), on);
            go(&rb, s(&l, "id"));
            w.rail_rows.append(&rb);
        }
    }

    fn render_content(&self, screen: &Value, w: &Widgets, sender: &ComponentSender<Self>) {
        let current = &screen["current"];
        let list = &current["list"];
        let color = s(current, "color");
        // The name, its count in grey after it (the toolbar's title, as Calendar's month and year).
        w.title.set_markup(&format!("<b>{}</b> <span weight=\"normal\" alpha=\"55%\">{}</span>", glib::markup_escape_text(s(current, "name")), screen["count"]));
        for c in uuob_kit_gtk::COLORS {
            w.title.remove_css_class(&format!("uuob-fg-{c}"));
        }
        if color != "gray" {
            w.title.add_css_class(&format!("uuob-fg-{color}"));
        }
        // The list's colour tints the page: the +, the ticks, Save.
        uuob_kit_gtk::set_tint(&w.page, Some(color).filter(|c| *c != "gray"));
        let from = list.get("from").is_some_and(Value::is_object);
        w.edit.set_visible(list.is_object());
        w.edit.set_child(Some(&uuob_kit_gtk::icon(if from { "people" } else { "edit" }, 19, 1.75, uuob_kit_gtk::text2())));
        w.edit.set_tooltip_text(Some(if from { "About this list" } else { "Edit list" }));
        w.share.set_visible(list.is_object() && !from);
        let note = s(list, "note");
        w.note.set_label(note);
        w.note.set_visible(!note.is_empty());
        w.quick_row.set_visible(current["canAdd"] == json!(true));

        while let Some(c) = w.groups.first_child() {
            w.groups.remove(&c);
        }
        for g in screen["groups"].as_array().into_iter().flatten() {
            if !s(g, "key").is_empty() {
                w.groups.append(&gtk::Label::builder().label(s(g, "key")).xalign(0.0).css_classes(["day"]).build());
            }
            let items = g["items"].as_array().cloned().unwrap_or_default();
            for (i, x) in items.iter().enumerate() {
                w.groups.append(&self.task_row(x, sender));
                if i + 1 < items.len() {
                    w.groups.append(&gtk::Separator::builder().margin_start(44).css_classes(["task-line"]).build());
                }
            }
        }
        if let Some(e) = screen["empty"].as_object() {
            w.groups.append(&uuob_kit_gtk::empty_state("check", e["headline"].as_str().unwrap_or_default(), "text3", None));
        }
    }

    fn task_row(&self, x: &Value, sender: &ComponentSender<Self>) -> gtk::Widget {
        let show = &x["show"];
        let id = s(x, "id").to_string();
        let done = x["done"] == json!(true);
        let can_edit = show["canEdit"] == json!(true);
        let card = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["task"]).build();
        let line = gtk::Box::builder().spacing(12).build();
        let check = gtk::Button::builder().valign(gtk::Align::Start).sensitive(can_edit).tooltip_text("Complete").css_classes(["check"]).build();
        check.add_css_class(&show["ring"].as_str().map(|c| format!("uuob-c-{c}")).unwrap_or_else(|| "accent-fill".into()));
        let finishing = show["finishing"] == json!(true);
        if self.animate.contains(&id) {
            // Drawn plain, then changed, so the fill and the fade play.
            let (c, k) = (check.clone(), card.clone());
            glib::idle_add_local_once(move || {
                c.add_css_class("done");
                k.add_css_class("finishing");
            });
        } else {
            if done {
                check.add_css_class("done");
            }
            if finishing {
                card.add_css_class("finishing");
            }
        }
        let (tx, xc) = (sender.clone(), x.clone());
        check.connect_clicked(move |_| tx.input(Msg::Toggle(xc.clone())));
        line.append(&check);

        let body = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).hexpand(true).build();
        let title = gtk::Label::builder().xalign(0.0).wrap(true).wrap_mode(gtk::pango::WrapMode::WordChar).css_classes(["t"]).build();
        if done {
            title.set_markup(&format!("<s>{}</s>", glib::markup_escape_text(s(x, "title"))));
            title.add_css_class("dim-label");
        } else {
            title.set_label(s(x, "title"));
        }
        body.append(&title);
        let notes = s(x, "notes");
        if !notes.is_empty() {
            body.append(&gtk::Label::builder().label(notes.lines().next().unwrap_or_default()).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).css_classes(["notes", "dim-label"]).build());
        }
        let when = show["when"].as_str();
        let list_name = show["listName"].as_str();
        if when.is_some() || list_name.is_some() {
            let meta = gtk::Box::builder().spacing(10).css_classes(["meta"]).build();
            if let Some(wh) = when {
                let l = gtk::Label::builder().label(wh).build();
                l.add_css_class(if show["late"] == json!(true) { "late" } else { "dim-label" });
                meta.append(&l);
            }
            if let Some(n) = list_name {
                meta.append(&gtk::Label::builder().label(n).css_classes(["lst", &format!("uuob-fg-{}", s(show, "listColor"))]).build());
            }
            body.append(&meta);
        }
        // A click opens the task's card.
        let open_btn = gtk::Button::builder().child(&body).hexpand(true).css_classes(["flat", "body"]).build();
        let (tx, xc) = (sender.clone(), x.clone());
        open_btn.connect_clicked(move |_| tx.input(Msg::Card(xc.clone())));
        line.append(&open_btn);
        card.append(&line);
        card.upcast()
    }

    /// The lists a task can be in: (id, name, colour) of those this user can change.
    fn writable_lists(&self) -> Vec<(String, String, String)> {
        self.last["lists"].as_array().into_iter().flatten().filter(|l| l["canWrite"] != json!(false)).map(|l| (s(l, "id").to_string(), s(l, "name").to_string(), s(l, "color").to_string())).collect()
    }

    /// The task's card (the kit's detail card): when, its list, notes; Edit
    /// (Ctrl+E), Delete, Complete.
    fn show_card(&self, x: &Value, root: &adw::ApplicationWindow, sender: &ComponentSender<Self>) {
        // The task as drawn (with how it shows), its fields as they are now.
        let show = &x["show"];
        let list_id = x["list"].clone();
        let mut x = self.current(x);
        x["show"] = show.clone();
        x["list"] = list_id.clone();
        let show = &x["show"];
        let can_edit = show["canEdit"] == json!(true);
        let edit = uuob_kit_gtk::icon_btn("edit", 19, "Edit (Ctrl+E)");
        let actions: Vec<gtk::Widget> = if can_edit { vec![edit.clone().upcast()] } else { vec![] };
        let (d, body) = uuob_kit_gtk::detail_sheet(s(&x, "title"), &actions);
        let rows = gtk::Box::builder().orientation(gtk::Orientation::Vertical).margin_top(6).css_classes(["icon-rows"]).build();
        let mut all: Vec<gtk::Box> = vec![];
        if let Some(wh) = show["when"].as_str() {
            all.push(uuob_kit_gtk::icon_row(Some("clock"), &label(wh, if show["late"] == json!(true) { &["late", "card-line"] } else { &["card-line"] })));
        }
        if let Some(l) = self.last["lists"].as_array().into_iter().flatten().find(|l| l["id"] == list_id) {
            let b = gtk::Box::builder().spacing(8).build();
            b.append(&gtk::Box::builder().valign(gtk::Align::Center).css_classes(["dot", &format!("uuob-c-{}", s(l, "color"))]).build());
            b.append(&label(s(l, "name"), &["card-line"]));
            all.push(uuob_kit_gtk::icon_row(Some("list"), &b));
        }
        if !s(&x, "notes").is_empty() {
            let n = label(s(&x, "notes"), &["card-line"]);
            n.set_selectable(true);
            all.push(uuob_kit_gtk::icon_row(Some("notes"), &n));
        }
        let n = all.len();
        for (i, r) in all.into_iter().enumerate() {
            if i + 1 == n {
                r.add_css_class("last");
            }
            rows.append(&r);
        }
        body.append(&rows);
        if can_edit {
            let bottom = gtk::Box::builder().spacing(10).margin_top(18).build();
            let del = uuob_kit_gtk::btn("Delete", None, &["danger"]);
            let done = x["done"] == json!(true);
            let complete = uuob_kit_gtk::btn(if done { "Not done" } else { "Complete" }, None, &["primary"]);
            bottom.append(&del);
            bottom.append(&gtk::Box::builder().hexpand(true).build());
            bottom.append(&complete);
            body.append(&bottom);
            let remove = {
                let (tx, xc, d) = (sender.clone(), x.clone(), d.clone());
                Rc::new(move || {
                    d.close();
                    tx.input(Msg::Remove(xc.clone()));
                })
            };
            let r = remove.clone();
            del.connect_clicked(move |_| r());
            dialog_key(&d, "Delete", move || remove());
            let (tx, xc, dd) = (sender.clone(), x.clone(), d.clone());
            complete.connect_clicked(move |_| {
                dd.close();
                tx.input(Msg::Toggle(xc.clone()));
            });
            let open_editor = {
                let (tx, xc, d) = (sender.clone(), x.clone(), d.clone());
                Rc::new(move || {
                    d.close();
                    tx.input(Msg::Edit(xc.clone()));
                })
            };
            let o = open_editor.clone();
            edit.connect_clicked(move |_| o());
            dialog_key(&d, "<Control>e", move || open_editor());
        }
        if let Some(c) = show["listColor"].as_str().or_else(|| self.last["current"]["color"].as_str()) {
            uuob_kit_gtk::set_tint(&d, Some(c));
        }
        d.present(Some(root));
    }

    /// The task's editor (the kit's editor): the title, its list under it, then
    /// notes, date and time. Nothing changes until Save; Esc or ✕ leave it as it was.
    fn show_editor(&self, x: &Value, root: &adw::ApplicationWindow, sender: &ComponentSender<Self>) {
        let color = x["show"]["listColor"].as_str().or_else(|| self.last["current"]["color"].as_str()).map(String::from);
        // Its list as the screen has it (the stored task only knows its container).
        let list_id = x["list"].as_str().map(String::from);
        let x = self.current(x);
        let due = Rc::new(std::cell::RefCell::new(opt(&x, "due")));
        let time = Rc::new(std::cell::RefCell::new(opt(&x, "time")));
        let lists = self.writable_lists();
        let notes = gtk::TextView::builder().wrap_mode(gtk::WrapMode::WordChar).top_margin(10).bottom_margin(10).left_margin(6).right_margin(6).css_classes(["field", "notes-field"]).build();
        notes.buffer().set_text(s(&x, "notes"));
        let mut names = vec!["No list".to_string()];
        names.extend(lists.iter().map(|l| l.1.clone()));
        let drop = gtk::DropDown::builder().model(&gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>())).css_classes(["flat"]).build();
        drop.set_selected(lists.iter().position(|l| list_id.as_deref() == Some(l.0.as_str())).map_or(0, |i| i + 1) as u32);
        let title_cell: Rc<std::cell::OnceCell<gtk::Entry>> = Rc::new(std::cell::OnceCell::new());
        let on_save = {
            let (tx, x, due, time, notes, drop, lists, title_cell, list_id) = (sender.clone(), x.clone(), due.clone(), time.clone(), notes.clone(), drop.clone(), lists.clone(), title_cell.clone(), list_id.clone());
            move || {
                let title = title_cell.get().map(|e| e.text().trim().to_string()).unwrap_or_default();
                if title.is_empty() {
                    return false;
                }
                let mut y = x.clone();
                y["title"] = json!(title);
                let b = notes.buffer();
                y["notes"] = json!(b.text(&b.start_iter(), &b.end_iter(), false).to_string());
                y["due"] = due.borrow().clone().map(Value::from).unwrap_or(Value::Null);
                y["time"] = time.borrow().clone().map(Value::from).unwrap_or(Value::Null);
                let i = drop.selected() as usize;
                let to = if i == 0 { None } else { lists.get(i - 1).map(|l| l.0.clone()) };
                if to != list_id {
                    tx.input(Msg::SaveMove(y, to));
                } else {
                    tx.input(Msg::Save(y));
                }
                true
            }
        };
        let remove: Box<dyn Fn()> = {
            let (tx, x) = (sender.clone(), x.clone());
            Box::new(move || tx.input(Msg::Remove(x.clone())))
        };
        let es = uuob_kit_gtk::edit_sheet("Task", s(&x, "title"), false, on_save, Some(remove));
        let _ = title_cell.set(es.title.clone());
        // Under the title: the list it is in.
        es.under.append(&uuob_kit_gtk::icon("list", 16, 1.75, uuob_kit_gtk::text2()));
        es.under.append(&drop);
        let rows = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["icon-rows"]).build();
        rows.append(&uuob_kit_gtk::icon_row(Some("notes"), &notes));
        // Date: picked from a calendar, or none.
        let shown = |d: &Option<String>| d.as_deref().map(dates::friendly).unwrap_or_else(|| "No date".into());
        let date_label = gtk::Label::builder().label(shown(&due.borrow())).xalign(0.0).hexpand(true).build();
        let date_btn = gtk::MenuButton::builder().css_classes(["field", "flat"]).child(&date_label).build();
        let cal = gtk::Calendar::new();
        if let Some(d) = due.borrow().as_deref().and_then(dates::parse) {
            if let Ok(dt) = glib::DateTime::from_local(d.year() as i32, d.month() as i32, d.day() as i32, 12, 0, 0.0) {
                cal.select_day(&dt);
            }
        }
        let pop_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).build();
        pop_box.append(&cal);
        let none = gtk::Button::builder().label("No date").css_classes(["flat"]).build();
        pop_box.append(&none);
        let pop = gtk::Popover::builder().child(&pop_box).build();
        date_btn.set_popover(Some(&pop));
        let (p, dl, du) = (pop.clone(), date_label.clone(), due.clone());
        cal.connect_day_selected(move |c| {
            let d = c.date();
            let v = Some(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day_of_month()));
            dl.set_label(&shown(&v));
            *du.borrow_mut() = v;
            p.popdown();
        });
        let (p, dl, du) = (pop.clone(), date_label.clone(), due.clone());
        none.connect_clicked(move |_| {
            *du.borrow_mut() = None;
            dl.set_label("No date");
            p.popdown();
        });
        rows.append(&uuob_kit_gtk::icon_row(Some("calendar"), &date_btn));
        let tm = time.clone();
        let time_field = uuob_kit_gtk::time_field(opt(&x, "time"), uuob_kit_gtk::hour12(&self.client), true, "No time", move |t| *tm.borrow_mut() = t);
        time_field.add_css_class("field");
        let last = uuob_kit_gtk::icon_row(Some("clock"), &time_field);
        last.add_css_class("last");
        rows.append(&last);
        es.body.append(&rows);
        uuob_kit_gtk::set_tint(&es.dialog, color.as_deref());
        es.dialog.present(Some(root));
        es.title.grab_focus();
    }
}

fn style() {
    let css = gtk::CssProvider::new();
    css.load_from_string(
        ".side-col { padding: 4px 12px 12px; }
         .circle { border-radius: 999px; color: white; }
         .uuob-c-gray { background: alpha(@window_fg_color, 0.55); color: white; }
         .dot { min-width: 10px; min-height: 10px; border-radius: 999px; }
         .card-line { font-size: 15px; margin: 12px 0; }
         .task-line { background: @uuob_line; min-height: 1px; }
         .main-col { padding: 8px 28px 60px; }
         .main-col.narrow { padding: 4px 14px 40px; }
         .quick { padding: 0 8px 0 16px; min-height: 54px; border-radius: 14px; background: @card_bg_color; border: 1px solid @uuob_line; margin: 0 0 16px; transition: border-color 180ms, box-shadow 180ms; }
         .quick:focus-within { border-color: @accent_bg_color; box-shadow: 0 0 0 4px alpha(@accent_bg_color, 0.12); }
         .quick entry { background: none; box-shadow: none; outline: none; font-size: 17px; }
         .preview { font-size: 13px; font-weight: 600; color: @accent_color; background: alpha(@accent_bg_color, 0.18); padding: 3px 10px; border-radius: 999px; margin-right: 6px; }
         .day { font-weight: 700; margin: 20px 4px 6px; }
         .task { border-radius: 14px; padding: 6px 8px; transition: opacity 500ms ease 250ms; }
         .task:hover { background: alpha(@window_fg_color, 0.04); }
         .task.finishing { opacity: 0.35; }
         .task .body { padding: 2px 4px; }
         .task button.body:hover, .task button.body:active { background: none; }
         .t { font-size: 17px; }
         .notes { font-size: 14px; }
         .meta { font-size: 14px; color: @uuob_text2; }
         .late { color: @error_color; font-weight: 500; }
         .lst { font-weight: 500; }
         .check { min-width: 20px; min-height: 20px; padding: 0; margin-top: 6px; border-radius: 999px; border: 2px solid alpha(@window_fg_color, 0.35); transition: background 220ms, border-color 220ms; }
         .check:not(.done) { background: none; }
         .check.done { border-color: transparent; }
         .check.done.accent-fill { background: @accent_bg_color; }
         .notes-field { min-height: 60px; background: none; }
         dropdown.flat > button { background: none; box-shadow: none; padding: 4px 6px; }
         .swatch { min-width: 30px; min-height: 30px; padding: 0; border-radius: 999px; }
         .swatch.on { box-shadow: 0 0 0 3px @dialog_bg_color, 0 0 0 5px @window_fg_color; }",
    );
    gtk::style_context_add_provider_for_display(&gdk::Display::default().expect("display"), &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

/// Add or edit a list (the kit's editor): its name, colour and sharing; or,
/// for a list someone shared with the user, a card with Leave list.
fn list_sheet(sheet: Value, root: &adw::ApplicationWindow, sender: &ComponentSender<Tasks>) {
    let id = opt(&sheet, "id");
    if let Some(from) = sheet["from"].as_object() {
        let who = from.get("name").and_then(Value::as_str).unwrap_or_default();
        let can = if sheet["canWrite"] == json!(true) { "You can both change it." } else { "You can view it, but not change it." };
        let (d, body) = uuob_kit_gtk::detail_sheet(s(&sheet, "name"), &[]);
        let rows = gtk::Box::builder().orientation(gtk::Orientation::Vertical).margin_top(6).css_classes(["icon-rows"]).build();
        let r = uuob_kit_gtk::icon_row(Some("people"), &label(&format!("{who} shares this list with you. {can}"), &["card-line"]));
        r.add_css_class("last");
        rows.append(&r);
        body.append(&rows);
        let leave = uuob_kit_gtk::btn("Leave list", None, &["danger"]);
        leave.set_halign(gtk::Align::Start);
        leave.set_margin_top(18);
        let (tx, dd) = (sender.clone(), d.clone());
        leave.connect_clicked(move |_| {
            dd.close();
            if let Some(id) = &id {
                tx.input(Msg::DeleteList(id.clone()));
            }
        });
        body.append(&leave);
        d.present(Some(root));
        return;
    }
    let color = Rc::new(std::cell::RefCell::new(s(&sheet, "color").to_string()));
    let title_cell: Rc<std::cell::OnceCell<gtk::Entry>> = Rc::new(std::cell::OnceCell::new());
    let on_save = {
        let (tx, color, sheet, title_cell) = (sender.clone(), color.clone(), sheet.clone(), title_cell.clone());
        move || {
            let n = title_cell.get().map(|e| e.text().trim().to_string()).unwrap_or_default();
            if n.is_empty() {
                return false;
            }
            let mut out = sheet.clone();
            out["name"] = json!(n);
            out["color"] = json!(*color.borrow());
            tx.input(Msg::SaveList(out));
            true
        }
    };
    let on_delete: Option<Box<dyn Fn()>> = id.clone().map(|id| {
        let tx = sender.clone();
        Box::new(move || tx.input(Msg::DeleteList(id.clone()))) as Box<dyn Fn()>
    });
    let es = uuob_kit_gtk::edit_sheet("List name", s(&sheet, "name"), id.is_none(), on_save, on_delete);
    let _ = title_cell.set(es.title.clone());
    es.save.set_label(if id.is_some() { "Save" } else { "Create" });
    es.save.set_sensitive(!s(&sheet, "name").trim().is_empty());
    let sv = es.save.clone();
    es.title.connect_changed(move |e| sv.set_sensitive(!e.text().trim().is_empty()));
    uuob_kit_gtk::set_tint(&es.dialog, Some(&color.borrow()));
    let rows = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["icon-rows"]).build();
    let swatches = gtk::Box::builder().spacing(10).margin_top(8).margin_bottom(8).build();
    let buttons: Rc<Vec<(&str, gtk::Button)>> = Rc::new(
        uuob_kit_gtk::COLORS
            .iter()
            .map(|c| {
                let b = gtk::Button::builder().tooltip_text(*c).css_classes(["swatch", &format!("uuob-c-{c}")]).build();
                if *color.borrow() == *c {
                    b.add_css_class("on");
                }
                swatches.append(&b);
                (*c, b)
            })
            .collect(),
    );
    for (c, b) in buttons.iter() {
        let (color, all, c, d) = (color.clone(), buttons.clone(), *c, es.dialog.clone());
        b.connect_clicked(move |_| {
            *color.borrow_mut() = c.to_string();
            uuob_kit_gtk::set_tint(&d, Some(c));
            for (k, b) in all.iter() {
                if *k == c { b.add_css_class("on") } else { b.remove_css_class("on") }
            }
        });
    }
    let colour_row = uuob_kit_gtk::icon_row(Some("list"), &swatches);
    rows.append(&colour_row);
    if let Some(id) = &id {
        let shared = sheet["sharedWith"].as_array().is_some_and(|a| !a.is_empty());
        let share = gtk::Button::builder().label(if shared { s(&sheet, "note") } else { "Share list…" }).halign(gtk::Align::Start).css_classes(["flat", "field"]).build();
        let (tx, id, d) = (sender.clone(), id.clone(), es.dialog.clone());
        share.connect_clicked(move |_| {
            d.close();
            tx.input(Msg::Share(id.clone()));
        });
        let r = uuob_kit_gtk::icon_row(Some("share"), &share);
        r.add_css_class("last");
        rows.append(&r);
    } else {
        colour_row.add_css_class("last");
    }
    es.body.append(&rows);
    es.dialog.present(Some(root));
    es.title.grab_focus();
}

impl relm4::Component for Tasks {
    type Init = Client;
    type Input = Msg;
    type Output = ();
    type CommandOutput = Cmd;
    type Root = adw::ApplicationWindow;
    type Widgets = Widgets;

    fn init_root() -> Self::Root {
        adw::ApplicationWindow::builder().title("Tasks").default_width(1100).default_height(760).width_request(360).height_request(560).build()
    }

    fn init(client: Client, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        uuob_kit_gtk::style();
        style();
        let info = AppInfo { id: "tasks".into(), name: "Tasks".into(), noun: "tasks".into(), icon: "object-select-symbolic".into(), color: "blue".into(), version: core::version() };

        // The sidebar (the kit's): the views and lists, New list, Settings.
        let side_rows = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).build();
        let new_list = uuob_kit_gtk::sidebar_row(Some(uuob_kit_gtk::icon("plus", 18, 2.0, "text2").upcast_ref()), "New list", None, false);
        new_list.set_margin_top(6);
        let tx = sender.clone();
        new_list.connect_clicked(move |_| tx.input(Msg::NewList));
        let settings = uuob_kit_gtk::sidebar_row(Some(uuob_kit_gtk::icon("settings", 19, 1.75, "text2").upcast_ref()), "Settings", None, false);
        settings.set_valign(gtk::Align::End);
        settings.set_vexpand(true);
        let tx = sender.clone();
        settings.connect_clicked(move |_| tx.input(Msg::Settings));
        let side = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).vexpand(true).css_classes(["side-col"]).build();
        side.append(&side_rows);
        side.append(&new_list);
        side.append(&settings);
        let side_view = adw::ToolbarView::new();
        // The window's buttons once, on the content's side.
        let sh = adw::HeaderBar::builder().show_title(false).show_end_title_buttons(false).build();
        sh.pack_start(&uuob_kit_gtk::update_button(FEED, info.version.clone()));
        side_view.add_top_bar(&sh);
        side_view.set_content(Some(&gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&side).build()));
        // Folded: the views' and lists' circles, New list, Settings.
        let rail = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).margin_top(12).vexpand(true).build();
        let rail_rows = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).build();
        rail.append(&rail_rows);
        let rail_add = uuob_kit_gtk::rail_icon("plus", "New list", false);
        let tx = sender.clone();
        rail_add.connect_clicked(move |_| tx.input(Msg::NewList));
        rail.append(&rail_add);
        let rail_settings = uuob_kit_gtk::rail_icon("settings", "Settings (Ctrl+,)", false);
        rail_settings.set_vexpand(true);
        rail_settings.set_valign(gtk::Align::End);
        let tx = sender.clone();
        rail_settings.connect_clicked(move |_| tx.input(Msg::Settings));
        rail.append(&rail_settings);

        // The view or list shown: the toolbar (name and count, Edit, Share), the quick field, the tasks.
        let edit = uuob_kit_gtk::icon_btn("edit", 19, "Edit list");
        edit.set_visible(false);
        let share = uuob_kit_gtk::icon_btn("share", 19, "Share list");
        share.set_visible(false);
        let (bar, title) = uuob_kit_gtk::toolbar("", false, &[edit.clone().upcast(), share.clone().upcast()], None);
        let tx = sender.clone();
        edit.connect_clicked(move |_| tx.input(Msg::EditList));
        let tx = sender.clone();
        share.connect_clicked(move |_| tx.input(Msg::Share(String::new())));
        let note = gtk::Label::builder().xalign(0.0).css_classes(["caption", "dim-label"]).visible(false).build();
        let quick = gtk::Entry::builder().placeholder_text("New task — try “call mom fri 5pm”").hexpand(true).build();
        let preview = gtk::Label::builder().css_classes(["preview"]).visible(false).valign(gtk::Align::Center).build();
        let quick_row = gtk::Box::builder().spacing(6).css_classes(["quick"]).build();
        let plus = uuob_kit_gtk::icon("plus", 20, 2.2, "accent");
        plus.add_css_class("uuob-tint-fg");
        quick_row.append(&plus);
        quick_row.append(&quick);
        quick_row.append(&preview);
        let (p, c) = (preview.clone(), client.clone());
        quick.connect_changed(move |e| {
            let text = e.text();
            let shown = if text.trim().is_empty() { None } else { core::quick_preview(text.to_string(), uuob_kit_gtk::hour12(&c)) };
            p.set_label(shown.as_deref().unwrap_or_default());
            p.set_visible(shown.is_some());
        });
        let tx = sender.clone();
        quick.connect_activate(move |e| {
            let text = e.text().to_string();
            if !text.trim().is_empty() {
                e.set_text("");
                tx.input(Msg::Add(text));
            }
        });
        let groups = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        let main = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).css_classes(["main-col"]).build();
        main.append(&bar);
        main.append(&note);
        main.append(&quick_row);
        main.append(&groups);
        let clamp = adw::Clamp::builder().maximum_size(760).child(&main).build();
        let main_scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build();
        let main_view = adw::ToolbarView::new();
        main_view.add_top_bar(&adw::HeaderBar::builder().show_title(false).show_start_title_buttons(false).build());
        main_view.set_content(Some(&main_scroll));

        let scaffold = uuob_kit_gtk::SidebarScaffold::new("tasks", &side_view, &rail, &main_view);
        // Narrow windows: a smaller title and margins.
        let narrow = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 720sp").expect("condition"));
        {
            let (t, m) = (title.clone(), main.clone());
            narrow.connect_apply(move |_| {
                t.add_css_class("compact");
                m.add_css_class("narrow");
            });
            let (t, m) = (title.clone(), main.clone());
            narrow.connect_unapply(move |_| {
                t.remove_css_class("compact");
                m.remove_css_class("narrow");
            });
        }
        root.add_breakpoint(narrow);

        let gate = Gate::new(info.clone(), client.clone(), {
            let tx = sender.clone();
            move || tx.input(Msg::Reload)
        });
        gate.set_content(&scaffold.widget());
        // The shared shortcuts (docs/design.md) and Tasks' own: New list, the four views.
        {
            let m = |msg: fn() -> Msg| -> Rc<dyn Fn()> {
                let tx = sender.clone();
                Rc::new(move || tx.input(msg()))
            };
            let mut extra: Vec<(String, String, Rc<dyn Fn()>)> = vec![("New list".into(), "<Control><Shift>n".into(), m(|| Msg::NewList))];
            for (i, (name, id)) in [("Today", "today"), ("Upcoming", "upcoming"), ("All", "all"), ("Completed", "completed")].into_iter().enumerate() {
                let tx = sender.clone();
                extra.push((name.into(), format!("<Control>{}", i + 1), Rc::new(move || tx.input(Msg::Go(id.into())))));
            }
            uuob_kit_gtk::Shortcuts {
                settings: Some(m(|| Msg::Settings)),
                new: Some(("New task".into(), m(|| Msg::NewTask))),
                find: None,
                toggle_sidebar: Some(scaffold.toggler()),
                refresh: Some(m(|| Msg::Reload)),
                extra,
            }
            .install(root.upcast_ref());
        }
        let close_peek = scaffold.closer();
        std::mem::forget(scaffold);
        root.set_content(Some(&gate.widget()));
        std::mem::forget(gate);

        // Live: what changes in UUOB, on any device.
        let mut events = client.subscribe();
        let tx = sender.clone();
        glib::spawn_future_local(async move {
            while let Ok(e) = events.recv().await {
                match e {
                    Event::Changed(_) | Event::State(uuob_client::State::Connected) => tx.input(Msg::Reload),
                    Event::Settings(_) => tx.input(Msg::Redraw),
                    _ => {}
                }
            }
        });
        // Every hour the coming-up window moves on.
        let tx = sender.clone();
        glib::timeout_add_seconds_local(3600, move || {
            tx.input(Msg::Upcoming(0));
            glib::ControlFlow::Continue
        });
        // Lists of old versions move into containers of their own.
        let c = client.clone();
        glib::spawn_future_local(async move {
            let _ = core::move_old_lists_with(c).await;
        });

        let tasks = client.cached_all("tasks");
        let infos = client.cached_all("list");
        let containers = client.cached_containers().unwrap_or_default();
        let alerts = client.cached("prefs", None).map(|p| core::alerts_on(Value::Array(p).to_string())).unwrap_or(true);
        let mut model = Tasks {
            client,
            info,
            tasks,
            containers,
            infos,
            alerts,
            tasks_known: false,
            alerts_known: false,
            view: "today".into(),
            finishing: HashSet::new(),
            animate: HashSet::new(),
            last: Value::Null,
            upcoming_gen: 0,
        };
        let widgets = Widgets { side_rows, rail_rows, page: main_view.upcast(), title, edit, share, note, quick, quick_row, groups, close_peek };
        model.render(&widgets, &sender, true);
        ComponentParts { model, widgets }
    }

    fn update_with_view(&mut self, w: &mut Widgets, msg: Msg, sender: ComponentSender<Self>, root: &Self::Root) {
        let client = self.client.clone();
        let mut force = false;
        match msg {
            Msg::Reload => {
                sender.oneshot_command(async move {
                    let containers = client.containers().await.ok();
                    let (tasks, complete) = client.all("tasks").await;
                    let (infos, _) = client.all("list").await;
                    let prefs = client.load("prefs", None).await.ok();
                    Cmd::Loaded { tasks, complete, containers, infos, prefs }
                });
                return;
            }
            Msg::Redraw => force = true,
            Msg::Go(id) => {
                self.view = id;
                w.quick.set_text("");
                force = true;
            }
            Msg::NewTask => {
                // A view without the field (Completed): Today, where it is.
                if self.last["current"]["canAdd"] != json!(true) {
                    self.view = "today".into();
                    self.render(w, &sender, true);
                }
                (w.close_peek)();
                w.quick.grab_focus();
                return;
            }
            Msg::Card(x) => {
                self.show_card(&x, root, &sender);
                return;
            }
            Msg::Edit(x) => {
                self.show_editor(&x, root, &sender);
                return;
            }
            Msg::Add(text) => {
                let record = core::quick_task_on(&text, &self.view, &dates::today(), now_ms());
                if record.is_null() {
                    return;
                }
                let c = self.arg(self.last["current"]["list"]["id"].as_str());
                sender.oneshot_command(async move {
                    let _ = client.put("tasks", c.as_deref(), None, record).await;
                    Cmd::Done
                });
                return;
            }
            Msg::Toggle(x) => {
                if x["show"]["canEdit"] != json!(true) {
                    return;
                }
                let x = self.current(&x);
                let next: Value = serde_json::from_str(&core::toggled(x.to_string())).unwrap_or_default();
                let id = s(&x, "id").to_string();
                if next["done"] == json!(true) {
                    self.finishing.insert(id.clone());
                    self.animate.insert(id.clone());
                    let tx = sender.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(700), move || tx.input(Msg::Finished(id)));
                }
                sender.input(Msg::Save(next));
                return;
            }
            Msg::Finished(id) => {
                self.finishing.remove(&id);
            }
            Msg::Save(x) => {
                for t in self.tasks.iter_mut() {
                    if t["id"] == x["id"] && t["container"] == x["container"] {
                        *t = x.clone();
                    }
                }
                let c = self.arg(x["container"].as_str());
                let record = core::stored_of(x);
                sender.oneshot_command(async move {
                    let _ = client.put("tasks", c.as_deref(), None, record).await;
                    Cmd::Done
                });
            }
            Msg::SaveMove(x, list) => {
                // Kept as edited here; the move stores it once, in its new list.
                for t in self.tasks.iter_mut() {
                    if t["id"] == x["id"] && t["container"] == x["container"] {
                        *t = x.clone();
                    }
                }
                sender.input(Msg::Move(x, list));
                return;
            }
            Msg::Remove(x) => {
                let x = self.current(&x);
                self.tasks.retain(|t| !(t["id"] == x["id"] && t["container"] == x["container"]));
                let c = self.arg(x["container"].as_str());
                let id = s(&x, "id").to_string();
                sender.oneshot_command(async move {
                    let _ = client.delete("tasks", c.as_deref(), &id).await;
                    Cmd::Done
                });
                force = true;
            }
            Msg::Move(x, list) => {
                let x = self.current(&x);
                // Moving a task to another list moves it to that list's container.
                let main = self.main_id();
                let to = list.unwrap_or_else(|| main.clone());
                let from = x["container"].as_str().unwrap_or(&main).to_string();
                if to == from {
                    return;
                }
                for t in self.tasks.iter_mut() {
                    if t["id"] == x["id"] && t["container"] == x["container"] {
                        t["container"] = json!(to);
                    }
                }
                let (to_arg, from_arg) = (self.arg(Some(&to)), self.arg(Some(&from)));
                let id = s(&x, "id").to_string();
                let record = core::stored_of(x);
                sender.oneshot_command(async move {
                    if client.put("tasks", to_arg.as_deref(), None, record).await.is_ok() {
                        let _ = client.delete("tasks", from_arg.as_deref(), &id).await;
                    }
                    Cmd::Done
                });
                force = true;
            }
            Msg::NewList => {
                let n = self.last["lists"].as_array().map_or(0, Vec::len);
                list_sheet(json!({ "name": "", "color": core::next_color(n as u32), "created": now_ms() }), root, &sender);
                return;
            }
            Msg::EditList => {
                let l = self.last["current"]["list"].clone();
                if l.is_object() {
                    list_sheet(l, root, &sender);
                }
                return;
            }
            Msg::SaveList(sheet) => {
                let name = s(&sheet, "name").trim().to_string();
                let info = json!({ "color": s(&sheet, "color"), "created": sheet["created"].as_i64().unwrap_or(0) });
                let id = opt(&sheet, "id");
                let was = id.as_ref().and_then(|id| self.last["lists"].as_array()?.iter().find(|l| s(l, "id") == id).map(|l| s(l, "name").to_string()));
                let tx = sender.clone();
                glib::spawn_future_local(async move {
                    match id {
                        Some(id) => {
                            if Some(&name) != was.as_ref() {
                                let _ = client.rename_container(&id, &name).await;
                            }
                            let _ = client.put("list", Some(&id), Some("info"), info).await;
                        }
                        None => {
                            if let Ok(made) = client.create_container(&name, None).await {
                                let list = made.as_str().map(String::from).unwrap_or_else(|| made.to_string());
                                let mut info = info;
                                info["created"] = json!(now_ms());
                                let _ = client.put("list", Some(&list), Some("info"), info).await;
                                tx.input(Msg::Go(list));
                            }
                        }
                    }
                    tx.input(Msg::Reload);
                });
                return;
            }
            Msg::DeleteList(id) => {
                // Deletes the list everywhere; a list someone shared with you is only left.
                self.view = "today".into();
                sender.oneshot_command(async move {
                    let _ = client.delete_container(&id).await;
                    Cmd::Done
                });
                force = true;
            }
            Msg::Share(id) => {
                let id = if id.is_empty() { s(&self.last["current"]["list"], "id").to_string() } else { id };
                if id.is_empty() {
                    return;
                }
                // UUOB comes to the front, where the user picks people.
                if !client.remote() {
                    let _ = gtk::gio::AppInfo::launch_default_for_uri("uuob://share", gtk::gio::AppLaunchContext::NONE);
                }
                sender.oneshot_command(async move {
                    let _ = client.share(&id).await;
                    Cmd::Done
                });
                return;
            }
            Msg::Settings => {
                let tx = sender.clone();
                let row = uuob_kit_gtk::choice_row("At a task's time", &["On", "Off"], if self.alerts { 0 } else { 1 }, move |i| tx.input(Msg::SetAlerts(i == 0)));
                let group = uuob_kit_gtk::group("Alerts", "", vec![row]);
                let tx = sender.clone();
                let dialog = uuob_kit_gtk::settings_dialog(&self.info, &self.client, false, vec![group], move || tx.input(Msg::Redraw));
                dialog.present(Some(root));
                return;
            }
            Msg::SetAlerts(on) => {
                self.alerts = on;
                sender.oneshot_command(async move {
                    let _ = client.put("prefs", None, Some("alerts"), json!({ "on": on })).await;
                    Cmd::Done
                });
                self.schedule_upcoming(&sender);
                return;
            }
            Msg::Upcoming(0) => return self.schedule_upcoming(&sender),
            Msg::Upcoming(generation) => {
                if generation != self.upcoming_gen {
                    return;
                }
                let list = core::upcoming_on(&json!({ "tasks": self.tasks, "containers": self.containers, "infos": self.infos, "alerts": self.alerts }), &dates::today());
                glib::spawn_future_local(async move { client.upcoming(list).await });
                return;
            }
        }
        self.render(w, &sender, force);
    }

    fn update_cmd_with_view(&mut self, w: &mut Widgets, cmd: Cmd, sender: ComponentSender<Self>, _: &Self::Root) {
        if let Cmd::Loaded { tasks, complete, containers, infos, prefs } = cmd {
            self.tasks = tasks;
            self.tasks_known |= complete;
            if let Some(c) = containers {
                self.containers = c;
            }
            self.infos = infos;
            if let Some(p) = prefs {
                let on = core::alerts_on(Value::Array(p).to_string());
                let first = !self.alerts_known;
                self.alerts_known = true;
                if on != self.alerts || first {
                    self.alerts = on;
                    self.schedule_upcoming(&sender);
                }
            }
            self.render(w, &sender, false);
        }
    }
}

fn main() {
    let client = Client::new(App { id: "tasks".into(), name: "Tasks".into(), android_package: None }, data_dir());
    RelmApp::new("app.uuob.tasks").with_args(Vec::new()).run::<Tasks>(client);
}
