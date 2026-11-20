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
    /// A task changed; `quiet`: from a field still being edited, so the
    /// screen isn't rebuilt under the user's cursor.
    Save(Value, bool),
    /// One field of a task changed in its editor (`quiet` as for Save).
    Set(Value, &'static str, Value, bool),
    Remove(Value),
    Move(Value, Option<String>),
    Open(Option<String>),
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
    open_id: Option<String>,
    /// The screen last drawn (the core's answer), to draw only when it changed.
    last: Value,
    upcoming_gen: u64,
}

pub struct Widgets {
    split: adw::NavigationSplitView,
    smart: gtk::Grid,
    lists: gtk::ListBox,
    hint: gtk::Label,
    content_page: adw::NavigationPage,
    title: gtk::Label,
    edit: gtk::Button,
    share: gtk::Button,
    count: gtk::Label,
    note: gtk::Label,
    quick: gtk::Entry,
    quick_row: gtk::Box,
    groups: gtk::Box,
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

fn icon_label(icon: &str, label: &str) -> gtk::Box {
    let b = gtk::Box::builder().spacing(6).build();
    b.append(&uuob_kit_gtk::icon(icon, 19, 1.75, uuob_kit_gtk::text2()));
    b.append(&gtk::Label::new(Some(label)));
    b
}

/// Saves when the field is left or Enter is pressed (as `onchange` did).
fn on_leave(widget: &impl IsA<gtk::Widget>, f: impl Fn() + 'static) {
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave(move |_| f());
    widget.add_controller(focus);
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
        while let Some(c) = w.smart.first_child() {
            w.smart.remove(&c);
        }
        for (i, sm) in screen["smart"].as_array().into_iter().flatten().enumerate() {
            let top = gtk::Box::builder().spacing(6).build();
            top.append(&circle(s(sm, "color"), 30, icon_of(s(sm, "icon"))));
            top.append(&gtk::Label::builder().label(sm["count"].to_string()).hexpand(true).xalign(1.0).css_classes(["count-big"]).build());
            let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).build();
            col.append(&top);
            col.append(&gtk::Label::builder().label(s(sm, "name")).xalign(0.0).css_classes(["tile-label"]).build());
            let b = gtk::Button::builder().child(&col).hexpand(true).css_classes(["smart-tile"]).build();
            if s(sm, "id") == self.view {
                b.add_css_class("on");
            }
            let id = s(sm, "id").to_string();
            let tx = sender.clone();
            b.connect_clicked(move |_| tx.input(Msg::Go(id.clone())));
            w.smart.attach(&b, (i % 2) as i32, (i / 2) as i32, 1, 1);
        }
        while let Some(c) = w.lists.first_child() {
            w.lists.remove(&c);
        }
        let lists = screen["lists"].as_array().cloned().unwrap_or_default();
        w.hint.set_visible(lists.is_empty());
        for l in lists {
            let row = gtk::Box::builder().spacing(12).hexpand(true).css_classes(["list-row"]).build();
            row.append(&circle(s(&l, "color"), 26, "list"));
            row.append(&gtk::Label::builder().label(s(&l, "name")).hexpand(true).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).css_classes(["list-name"]).build());
            if l["shared"] == json!(true) {
                row.append(&uuob_kit_gtk::icon("people", 15, 1.75, uuob_kit_gtk::text2()));
            }
            row.append(&gtk::Label::builder().label(l["count"].to_string()).css_classes(["dim-label"]).build());
            let r = gtk::ListBoxRow::builder().child(&row).build();
            r.set_widget_name(s(&l, "id"));
            if s(&l, "id") == self.view {
                r.add_css_class("on");
            }
            w.lists.append(&r);
        }
    }

    fn render_content(&self, screen: &Value, w: &Widgets, sender: &ComponentSender<Self>) {
        let current = &screen["current"];
        let list = &current["list"];
        let color = s(current, "color");
        w.content_page.set_title(s(current, "name"));
        w.title.set_label(s(current, "name"));
        for c in uuob_kit_gtk::COLORS {
            w.title.remove_css_class(&format!("uuob-fg-{c}"));
        }
        if color != "gray" {
            w.title.add_css_class(&format!("uuob-fg-{color}"));
        }
        let from = list.get("from").is_some_and(Value::is_object);
        w.edit.set_visible(list.is_object());
        w.edit.set_child(Some(&uuob_kit_gtk::icon(if from { "people" } else { "edit" }, 18, 1.75, uuob_kit_gtk::text2())));
        w.edit.set_tooltip_text(Some(if from { "About this list" } else { "Edit list" }));
        w.share.set_visible(list.is_object() && !from);
        w.count.set_label(&screen["count"].to_string());
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
            for x in g["items"].as_array().into_iter().flatten() {
                w.groups.append(&self.task_row(x, screen, sender));
            }
        }
        if let Some(e) = screen["empty"].as_object() {
            let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).margin_top(60).halign(gtk::Align::Center).build();
            col.append(&gtk::Label::builder().label(e["glyph"].as_str().unwrap_or_default()).css_classes(["glyph"]).build());
            col.append(&gtk::Label::builder().label(e["headline"].as_str().unwrap_or_default()).css_classes(["title-3"]).build());
            col.append(&gtk::Label::builder().label(e["caption"].as_str().unwrap_or_default()).wrap(true).justify(gtk::Justification::Center).css_classes(["dim-label"]).build());
            w.groups.append(&col);
        }
    }

    fn task_row(&self, x: &Value, screen: &Value, sender: &ComponentSender<Self>) -> gtk::Widget {
        let show = &x["show"];
        let id = s(x, "id").to_string();
        let open = self.open_id.as_deref() == Some(id.as_str());
        let done = x["done"] == json!(true);
        let can_edit = show["canEdit"] == json!(true);
        let card = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["task"]).build();
        if open {
            card.add_css_class("open");
        }
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
        if !notes.is_empty() && !open {
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
        let open_btn = gtk::Button::builder().child(&body).hexpand(true).sensitive(can_edit).css_classes(["flat", "body"]).build();
        let tx = sender.clone();
        let next = if open { None } else { Some(id.clone()) };
        open_btn.connect_clicked(move |_| tx.input(Msg::Open(next.clone())));
        line.append(&open_btn);
        card.append(&line);
        if open {
            card.append(&self.editor(x, screen, sender));
        }
        card.upcast()
    }

    fn editor(&self, x: &Value, screen: &Value, sender: &ComponentSender<Self>) -> gtk::Widget {
        let ed = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).css_classes(["editor"]).build();
        let saver = |k: &'static str| {
            let (tx, x0) = (sender.clone(), x.clone());
            move |v: Value, quiet: bool| tx.input(Msg::Set(x0.clone(), k, v, quiet))
        };
        // Title and notes save when they are left.
        let title = gtk::Entry::builder().text(s(x, "title")).css_classes(["field"]).build();
        let save = Rc::new(saver("title"));
        let (sv, e) = (save.clone(), title.clone());
        title.connect_activate(move |e| sv(json!(e.text().to_string()), true));
        on_leave(&title, move || save(json!(e.text().to_string()), true));
        ed.append(&title);
        let notes = gtk::TextView::builder().wrap_mode(gtk::WrapMode::WordChar).top_margin(8).bottom_margin(8).left_margin(10).right_margin(10).css_classes(["field", "notes-field"]).build();
        notes.buffer().set_text(s(x, "notes"));
        let (save, n) = (saver("notes"), notes.clone());
        on_leave(&notes, move || {
            let b = n.buffer();
            save(json!(b.text(&b.start_iter(), &b.end_iter(), false).to_string()), true);
        });
        ed.append(&gtk::Frame::builder().child(&notes).build());

        let grid = gtk::Box::builder().spacing(10).homogeneous(true).build();
        let field = |label: &str, w: &gtk::Widget| {
            let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).build();
            b.append(&gtk::Label::builder().label(label).xalign(0.0).css_classes(["caption", "dim-label"]).build());
            b.append(w);
            b
        };
        // Date: picked from a calendar, or none.
        let due = opt(x, "due");
        let date_btn = gtk::MenuButton::builder().css_classes(["field"]).always_show_arrow(true).child(&gtk::Label::builder().label(due.as_deref().map(dates::friendly).unwrap_or_else(|| "None".into())).xalign(0.0).hexpand(true).build()).build();
        let cal = gtk::Calendar::new();
        if let Some(d) = due.as_deref().and_then(dates::parse) {
            if let Ok(dt) = glib::DateTime::from_local(d.year() as i32, d.month() as i32, d.day() as i32, 12, 0, 0.0) {
                cal.select_day(&dt);
            }
        }
        let pop_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).build();
        pop_box.append(&cal);
        let none = gtk::Button::builder().label("None").css_classes(["flat"]).build();
        pop_box.append(&none);
        let pop = gtk::Popover::builder().child(&pop_box).build();
        date_btn.set_popover(Some(&pop));
        let save = Rc::new(saver("due"));
        let (sv, p) = (save.clone(), pop.clone());
        cal.connect_day_selected(move |c| {
            let d = c.date();
            p.popdown();
            sv(json!(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day_of_month())), false);
        });
        let p = pop.clone();
        none.connect_clicked(move |_| {
            p.popdown();
            save(Value::Null, false);
        });
        grid.append(&field("Date", date_btn.upcast_ref()));
        let save = saver("time");
        let time = uuob_kit_gtk::time_field(opt(x, "time"), uuob_kit_gtk::hour12(&self.client), true, "None", move |t| save(t.map(Value::from).unwrap_or(Value::Null), true));
        grid.append(&field("Time", time.upcast_ref()));
        // List: the lists this user can change.
        let lists: Vec<Value> = screen["lists"].as_array().into_iter().flatten().filter(|l| l["canWrite"] != json!(false)).cloned().collect();
        let mut names = vec!["None".to_string()];
        names.extend(lists.iter().map(|l| s(l, "name").to_string()));
        let model = gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>());
        let drop = gtk::DropDown::builder().model(&model).css_classes(["field"]).build();
        drop.set_selected(lists.iter().position(|l| l["id"] == x["list"]).map_or(0, |i| i + 1) as u32);
        let (tx, x0) = (sender.clone(), x.clone());
        drop.connect_selected_notify(move |d| {
            let i = d.selected() as usize;
            let to = if i == 0 { None } else { lists.get(i - 1).map(|l| s(l, "id").to_string()) };
            tx.input(Msg::Move(x0.clone(), to));
        });
        grid.append(&field("List", drop.upcast_ref()));
        ed.append(&grid);

        let actions = gtk::Box::builder().margin_top(4).build();
        let del = uuob_kit_gtk::btn("Delete", Some("trash"), &["danger"]);
        let (tx, x0) = (sender.clone(), x.clone());
        del.connect_clicked(move |_| tx.input(Msg::Remove(x0.clone())));
        let done = uuob_kit_gtk::btn("Done", None, &[]);
        done.set_halign(gtk::Align::End);
        done.set_hexpand(true);
        let tx = sender.clone();
        done.connect_clicked(move |_| tx.input(Msg::Open(None)));
        actions.append(&del);
        actions.append(&done);
        ed.append(&actions);
        ed.upcast()
    }
}

fn style() {
    let css = gtk::CssProvider::new();
    css.load_from_string(
        ".sidebar-col { padding: 12px 12px 18px; }
         .smart-tile { padding: 12px; border-radius: 14px; background: @card_bg_color; border: 1px solid @uuob_line; transition: background 160ms, border-color 160ms; }
         .smart-tile:hover { background: mix(@card_bg_color, @window_fg_color, 0.04); }
         .smart-tile:active { transform: scale(0.97); }
         .smart-tile.on { background: alpha(@accent_bg_color, 0.12); border-color: alpha(@accent_bg_color, 0.5); }
         .circle { border-radius: 999px; color: white; }
         .uuob-c-gray { background: alpha(@window_fg_color, 0.55); color: white; }
         .count-big { font-size: 24px; font-weight: 700; }
         .tile-label { font-size: 15px; font-weight: 600; color: @uuob_text2; }
         .list-name { font-weight: 500; }
         .large-title.narrow-title { font-size: 30px; }
         .lists-label { margin: 22px 8px 6px; }
         button.btn.ghost.settings-btn { color: @uuob_text2; }
         .list-row { padding: 2px 0; }
         list.lists row { border-radius: 10px; padding: 6px 8px; }
         list.lists row.on { background: alpha(@window_fg_color, 0.1); }
         .main-col { padding: 8px 28px 60px; }
         .count-title { font-size: 34px; font-weight: 700; color: @uuob_text3; }
         .quick { padding: 0 8px 0 16px; min-height: 54px; border-radius: 14px; background: @card_bg_color; border: 1px solid @uuob_line; margin: 6px 0 16px; transition: border-color 180ms, box-shadow 180ms; }
         .quick:focus-within { border-color: @accent_bg_color; box-shadow: 0 0 0 4px alpha(@accent_bg_color, 0.12); }
         .quick entry { background: none; box-shadow: none; outline: none; font-size: 17px; }
         .preview { font-size: 13px; font-weight: 600; color: @accent_color; background: alpha(@accent_bg_color, 0.18); padding: 3px 10px; border-radius: 999px; margin-right: 6px; }
         .day { font-weight: 700; margin: 20px 4px 6px; }
         .task { border-radius: 14px; padding: 6px 8px; transition: opacity 500ms ease 250ms; }
         .task:hover { background: alpha(@window_fg_color, 0.04); }
         .task.open { background: @card_bg_color; box-shadow: 0 1px 4px alpha(black, 0.15); margin: 6px 0; }
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
         .editor { padding: 4px 6px 8px 40px; }
         .notes-field { min-height: 60px; }
         entry.field.name-field { font-size: 20px; font-weight: 600; min-height: 54px; }
         textview.field { border: 1px solid @uuob_line; box-shadow: none; }
         .glyph { font-size: 44px; }
         .hero { min-width: 72px; min-height: 72px; }
         .swatch { min-width: 30px; min-height: 30px; padding: 0; border-radius: 999px; }
         .swatch.on { box-shadow: 0 0 0 3px @window_bg_color, 0 0 0 5px @window_fg_color; }
         .name-field { font-size: 18px; font-weight: 600; }",
    );
    gtk::style_context_add_provider_for_display(&gdk::Display::default().expect("display"), &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

/// Add or edit a list: its name, colour and sharing; or, for a list someone
/// shared with the user, what it is and Leave list.
fn list_sheet(sheet: Value, root: &adw::ApplicationWindow, sender: &ComponentSender<Tasks>) {
    let id = opt(&sheet, "id");
    if let Some(from) = sheet["from"].as_object() {
        let who = from.get("name").and_then(Value::as_str).unwrap_or_default();
        let can = if sheet["canWrite"] == json!(true) { "You can both change it." } else { "You can view it, but not change it." };
        let tx = sender.clone();
        let d = uuob_kit_gtk::confirm_sheet(s(&sheet, "name"), &format!("{who} shares this list with you. {can}"), &[("Leave list", &["danger"], "leave"), ("Done", &["primary"], "done")], false, move |r| {
            if let (Some(id), "leave") = (&id, r) {
                tx.input(Msg::DeleteList(id.clone()));
            }
        });
        d.present(Some(root));
        return;
    }
    let dialog = adw::Dialog::builder().title(if id.is_some() { "Edit list" } else { "New list" }).content_width(380).build();
    let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(14).margin_top(6).margin_bottom(22).margin_start(22).margin_end(22).build();
    let color = Rc::new(std::cell::RefCell::new(s(&sheet, "color").to_string()));
    let hero = gtk::Box::builder().halign(gtk::Align::Center).css_classes(["hero", "circle", &format!("uuob-c-{}", color.borrow())]).build();
    let hi = uuob_kit_gtk::icon("list", 30, 2.4, "#ffffff");
    hi.set_hexpand(true);
    hi.set_halign(gtk::Align::Center);
    hero.append(&hi);
    hero.set_hexpand(false);
    col.append(&hero);
    let name = gtk::Entry::builder().placeholder_text("List name").text(s(&sheet, "name")).xalign(0.5).css_classes(["field", "name-field"]).build();
    col.append(&name);
    let swatches = gtk::Box::builder().spacing(12).halign(gtk::Align::Center).build();
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
        let (color, hero, all, c) = (color.clone(), hero.clone(), buttons.clone(), *c);
        b.connect_clicked(move |_| {
            hero.remove_css_class(&format!("uuob-c-{}", color.borrow()));
            *color.borrow_mut() = c.to_string();
            hero.add_css_class(&format!("uuob-c-{c}"));
            for (k, b) in all.iter() {
                if *k == c { b.add_css_class("on") } else { b.remove_css_class("on") }
            }
        });
    }
    col.append(&swatches);
    if let Some(id) = &id {
        let shared = sheet["sharedWith"].as_array().is_some_and(|a| !a.is_empty());
        let share = uuob_kit_gtk::btn(if shared { s(&sheet, "note") } else { "Share list…" }, Some("share"), &["ghost"]);
        let (tx, id, d) = (sender.clone(), id.clone(), dialog.clone());
        share.connect_clicked(move |_| {
            d.close();
            tx.input(Msg::Share(id.clone()));
        });
        col.append(&share);
    }
    let actions = gtk::Box::builder().spacing(10).homogeneous(true).margin_top(8).build();
    let left = uuob_kit_gtk::btn(if id.is_some() { "Delete" } else { "Cancel" }, None, &["lg"]);
    if id.is_some() {
        left.add_css_class("danger");
    }
    let save = uuob_kit_gtk::btn(if id.is_some() { "Save" } else { "Create" }, None, &["lg", "primary"]);
    save.set_sensitive(!s(&sheet, "name").trim().is_empty());
    let sv = save.clone();
    name.connect_changed(move |e| sv.set_sensitive(!e.text().trim().is_empty()));
    let (tx, d, del) = (sender.clone(), dialog.clone(), id.clone());
    left.connect_clicked(move |_| {
        d.close();
        if let Some(id) = &del {
            tx.input(Msg::DeleteList(id.clone()));
        }
    });
    let submit = {
        let (tx, d, name, color, sheet) = (sender.clone(), dialog.clone(), name.clone(), color.clone(), sheet.clone());
        Rc::new(move || {
            let n = name.text().trim().to_string();
            if n.is_empty() {
                return;
            }
            let mut out = sheet.clone();
            out["name"] = json!(n);
            out["color"] = json!(*color.borrow());
            d.close();
            tx.input(Msg::SaveList(out));
        })
    };
    let sb = submit.clone();
    save.connect_clicked(move |_| sb());
    name.connect_activate(move |_| submit());
    actions.append(&left);
    actions.append(&save);
    col.append(&actions);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&col));
    dialog.set_child(Some(&view));
    dialog.present(Some(root));
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

        // The sidebar: smart views, lists, Settings.
        let smart = gtk::Grid::builder().column_spacing(10).row_spacing(10).column_homogeneous(true).build();
        let lists = gtk::ListBox::builder().css_classes(["navigation-sidebar", "lists"]).selection_mode(gtk::SelectionMode::None).build();
        let tx = sender.clone();
        lists.connect_row_activated(move |_, r| tx.input(Msg::Go(r.widget_name().to_string())));
        let hint = gtk::Label::builder().label("Group tasks into lists like Home, Work or Groceries.").wrap(true).xalign(0.0).margin_start(8).css_classes(["caption", "dim-label"]).build();
        let add_list = uuob_kit_gtk::btn("Add list", Some("plus"), &["ghost"]);
        add_list.set_halign(gtk::Align::Start);
        let tx = sender.clone();
        add_list.connect_clicked(move |_| tx.input(Msg::NewList));
        let settings = gtk::Button::builder().child(&icon_label("settings", "Settings")).css_classes(["btn", "ghost", "settings-btn"]).halign(gtk::Align::Start).valign(gtk::Align::End).vexpand(true).build();
        let tx = sender.clone();
        settings.connect_clicked(move |_| tx.input(Msg::Settings));
        let side = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).css_classes(["sidebar-col"]).build();
        // On phones the overview has its own large title, as on the web.
        let side_title = gtk::Label::builder().label("Tasks").xalign(0.0).visible(false).margin_bottom(18).css_classes(["large-title", "narrow-title"]).build();
        side.append(&side_title);
        side.append(&smart);
        side.append(&gtk::Label::builder().label("MY LISTS").xalign(0.0).css_classes(["caption-heading", "dim-label", "lists-label"]).build());
        side.append(&lists);
        side.append(&hint);
        side.append(&add_list);
        side.append(&settings);
        let side_scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&side).build();
        let side_header = adw::HeaderBar::builder().show_title(false).build();
        side_header.pack_start(&uuob_kit_gtk::update_button(FEED, info.version.clone()));
        let side_view = adw::ToolbarView::new();
        side_view.add_top_bar(&side_header);
        side_view.set_content(Some(&side_scroll));
        let side_page = adw::NavigationPage::builder().title("Tasks").child(&side_view).build();

        // The view or list shown.
        let title = gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).css_classes(["large-title"]).build();
        let edit = uuob_kit_gtk::icon_btn("edit", 18, "Edit list");
        edit.set_visible(false);
        let share = uuob_kit_gtk::icon_btn("share", 18, "Share list");
        share.set_visible(false);
        let count = gtk::Label::builder().hexpand(true).xalign(1.0).css_classes(["count-title"]).build();
        let head = gtk::Box::builder().spacing(6).build();
        head.append(&title);
        head.append(&edit);
        head.append(&share);
        head.append(&count);
        let tx = sender.clone();
        edit.connect_clicked(move |_| tx.input(Msg::EditList));
        let tx = sender.clone();
        share.connect_clicked(move |_| tx.input(Msg::Share(String::new())));
        let note = gtk::Label::builder().xalign(0.0).css_classes(["caption", "dim-label"]).visible(false).build();
        let quick = gtk::Entry::builder().placeholder_text("New task — try “call mom fri 5pm”").hexpand(true).build();
        let preview = gtk::Label::builder().css_classes(["preview"]).visible(false).valign(gtk::Align::Center).build();
        let quick_row = gtk::Box::builder().spacing(6).css_classes(["quick"]).build();
        quick_row.append(&uuob_kit_gtk::icon("plus", 20, 2.2, "accent"));
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
        main.append(&head);
        main.append(&note);
        main.append(&quick_row);
        main.append(&groups);
        let clamp = adw::Clamp::builder().maximum_size(760).child(&main).build();
        let main_scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build();
        let main_view = adw::ToolbarView::new();
        main_view.add_top_bar(&adw::HeaderBar::builder().show_title(false).build());
        main_view.set_content(Some(&main_scroll));
        let content_page = adw::NavigationPage::builder().title("Today").child(&main_view).build();

        let split = adw::NavigationSplitView::builder().sidebar(&side_page).content(&content_page).min_sidebar_width(290.0).max_sidebar_width(320.0).build();
        // Phones and narrow windows: the overview, then a list.
        let narrow = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 720sp").expect("condition"));
        narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
        narrow.add_setter(&side_title, "visible", Some(&true.to_value()));
        root.add_breakpoint(narrow);

        let gate = Gate::new(info.clone(), client.clone(), {
            let tx = sender.clone();
            move || tx.input(Msg::Reload)
        });
        gate.set_content(&split);
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
            open_id: None,
            last: Value::Null,
            upcoming_gen: 0,
        };
        let widgets = Widgets { split, smart, lists, hint, content_page, title, edit, share, count, note, quick, quick_row, groups };
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
                self.open_id = None;
                w.split.set_show_content(true);
                w.quick.set_text("");
                force = true;
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
                sender.input(Msg::Save(next, false));
                return;
            }
            Msg::Finished(id) => {
                self.finishing.remove(&id);
            }
            Msg::Set(x, field, value, quiet) => {
                // Only a task still there (an editor's text must not bring back a deleted or moved one).
                if !self.tasks.iter().any(|t| t["id"] == x["id"] && t["container"] == x["container"]) {
                    return;
                }
                let mut y = self.current(&x);
                if y[field] == value || (y[field].is_null() && value == json!("")) {
                    return;
                }
                y[field] = value;
                sender.input(Msg::Save(y, quiet));
                return;
            }
            Msg::Save(x, quiet) => {
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
                if quiet {
                    // The editor shows it already: remember it as drawn.
                    self.last = self.screen();
                    self.schedule_upcoming(&sender);
                    return;
                }
            }
            Msg::Remove(x) => {
                let x = self.current(&x);
                self.open_id = None;
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
            Msg::Open(id) => {
                self.open_id = id;
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
                let group = uuob_kit_gtk::group("Alerts", "For tasks with a time. They ring on your phones and computers, as Tasks notifications.", vec![row]);
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
