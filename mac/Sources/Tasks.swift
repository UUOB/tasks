// Tasks for the Mac: SwiftUI over Tasks' core (core/, through UniFFI) and
// UUOB's SDK. The connect screen, Settings, the time field and the update
// button are UUOB's kit for apps (uuob/kit/mac).

import AppKit
import SwiftUI

let tasksApp = AppInfo(
    id: "tasks", name: "Tasks", noun: "tasks", symbol: "checklist", color: "blue",
    version: version(),
    updateFeed: "https://raw.githubusercontent.com/jachymri/uuob-releases/main/tasks.json",
    updateKey: "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDU5QjY3MTNBQjA5NTI0QzAKUldUQUpKV3dPbkcyV2VzZU9XSWJQT29NWTR3angxNjU1VWJZazJsMi80Zjl6WDJoSWxMK0t3MlUK"
)


@main
struct TasksApp: App {
    @State private var model: TasksModel
    @State private var updater = Updater(tasksApp)

    init() {
        let uuob = Uuob(tasksApp)
        _model = State(initialValue: TasksModel(uuob))
    }

    var body: some Scene {
        WindowGroup("Tasks") {
            Gate(uuob: model.uuob, updater: updater) { TasksView(model: model, updater: updater) }
                .frame(minWidth: 620, minHeight: 560)
                .background(Look.bg)
        }
        // No title bar: the sidebar and the list are the window, as on the web.
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1100, height: 760)
    }
}

func nowMs() -> Int64 { Int64(Date().timeIntervalSince1970 * 1000) }

// The core's answers (core/src/lib.rs), named apart from the model's own `screen`.
private func coreScreen(_ state: String) -> String { screen(state: state) }
private func coreUpcoming(_ state: String) -> String { upcoming(state: state) }

/// A colour as the core names it ("gray" for All).
func tint(_ name: String) -> Color { name == "gray" ? Color(light: 0x8e8e93, dark: 0x8e8e93) : Palette.color(name) }

/// Tasks' state: every task, list and preference, kept live from UUOB. What
/// the screen shows comes from the core (`screen`); this only reads, writes
/// and remembers what the user is looking at.
@Observable
@MainActor
final class TasksModel {
    let uuob: Uuob
    let settings: UuobSettings
    var tasks = "[]"
    var containers = "[]"
    var infos = "[]"
    var alerts = true
    var tasksKnown = false
    var alertsKnown = false
    var view = "today"
    var finishing: Set<String> = []
    var openId: String?
    var listSheet: J?
    var settingsOpen = false
    private var upcomingGen = 0

    init(_ uuob: Uuob) {
        self.uuob = uuob
        settings = UuobSettings(uuob)
        let c = uuob.client
        tasks = c.cachedAll(collection: "tasks")
        infos = c.cachedAll(collection: "list")
        if let cs = c.cachedContainers() { containers = cs }
        if let p = c.cached(collection: "prefs", container: nil) { alerts = alertsOn(prefs: p) }
        Task { try? await moveOldLists(uuob: c) }
        Task {
            await load()
            for await e in uuob.events() where e.type.string == "changed" || e.state.string == "connected" {
                await load()
            }
        }
        // Every hour the coming-up window moves on.
        Task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(3600))
                scheduleUpcoming()
            }
        }
    }

    var client: UuobClient { uuob.client }

    /// The app's first container: tasks in no list.
    private var main: String { J(text: containers).array.first { $0.main.bool }?.id ?? "tasks" }
    private func arg(_ c: String?) -> String? { c == nil || c == main ? nil : c }

    private func state(_ extra: [String: Any]) -> String {
        var s: [String: Any] = [
            "tasks": J(text: tasks).value ?? [Any](), "containers": J(text: containers).value ?? [Any](), "infos": J(text: infos).value ?? [Any](),
        ]
        s.merge(extra) { _, b in b }
        return J(s).text
    }

    var screen: J {
        J(text: coreScreen(state(["view": view, "finishing": Array(finishing), "hour12": settings.hour12])))
    }

    func load() async {
        if let cs = try? await client.containers() { containers = cs }
        let all = J(text: await client.all(collection: "tasks"))
        tasks = all.records.text
        if all.complete.bool { tasksKnown = true }
        infos = J(text: await client.all(collection: "list")).records.text
        if let p = try? await client.load(collection: "prefs", container: nil) {
            alerts = alertsOn(prefs: p)
            alertsKnown = true
        }
        scheduleUpcoming()
    }

    /// What is coming up is written 1.5 s after the last change, once every
    /// list and the setting have been read.
    func scheduleUpcoming() {
        guard tasksKnown, alertsKnown else { return }
        upcomingGen += 1
        let generation = upcomingGen
        Task {
            try? await Task.sleep(for: .milliseconds(1500))
            guard generation == upcomingGen else { return }
            await client.upcoming(list: coreUpcoming(state(["alerts": alerts])))
        }
    }

    private func replace(_ x: J) {
        let items = J(text: tasks).array.map { $0.id == x.id && $0.container == x.container ? x.value as Any : $0.value as Any }
        tasks = J(items).text
    }

    /// The task as it is now (a view's copy may be older than an edit).
    func current(_ x: J) -> J { J(text: tasks).array.first { $0.id == x.id && $0.container == x.container } ?? x }

    func go(_ id: String) {
        view = id
        openId = nil
    }

    func add(_ text: String, list: String?) {
        let record = quickTask(text: text, view: view)
        guard record != "null" else { return }
        Task { _ = try? await client.put(collection: "tasks", container: arg(list), id: nil, value: record) }
    }

    func toggle(_ x: J) {
        guard x.show.canEdit.bool else { return }
        let next = J(text: toggled(task: current(x).text))
        if next.done.bool {
            finishing.insert(x.id)
            Task {
                try? await Task.sleep(for: .milliseconds(700))
                finishing.remove(x.id)
            }
        }
        save(next)
    }

    func save(_ x: J) {
        replace(x)
        scheduleUpcoming()
        Task { _ = try? await client.put(collection: "tasks", container: arg(x.container.str), id: nil, value: stored(task: x.text)) }
    }

    /// One field of a task changed in its editor.
    func set(_ x: J, _ field: String, _ value: Any?) {
        // Only a task still there (an editor's text must not bring back a deleted or moved one).
        guard let now = J(text: tasks).array.first(where: { $0.id == x.id && $0.container == x.container }) else { return }
        var y = (now.value as? [String: Any]) ?? [:]
        y[field] = value ?? NSNull()
        let next = J(y)
        if next != now { save(next) }
    }

    func remove(_ x: J) {
        openId = nil
        tasks = J(J(text: tasks).array.filter { !($0.id == x.id && $0.container == x.container) }.map { $0.value as Any }).text
        Task { _ = try? await client.delete(collection: "tasks", container: arg(x.container.str), id: x.id) }
    }

    /// Moving a task to another list moves it to that list's container.
    func move(_ x: J, to list: String?) {
        let x = current(x)
        let to = list ?? main
        let from = x.container.str ?? main
        guard to != from else { return }
        var moved = (x.value as? [String: Any]) ?? [:]
        moved["container"] = to
        replace(J(moved))
        let record = stored(task: x.text)
        Task {
            if (try? await client.put(collection: "tasks", container: arg(to), id: nil, value: record)) != nil {
                _ = try? await client.delete(collection: "tasks", container: arg(from), id: x.id)
            }
        }
    }

    func newList() {
        listSheet = J(["name": "", "color": nextColor(lists: UInt32(screen.lists.array.count)), "created": nowMs()])
    }

    func saveList(id: String?, name: String, color: String, created: Int64) {
        let name = name.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        listSheet = nil
        let was = screen.lists.array.first { $0.id == id }?.name.string
        Task {
            if let id {
                if name != was { try? await client.renameContainer(id: id, name: name) }
                _ = try? await client.put(collection: "list", container: id, id: "info", value: J(["color": color, "created": created]).text)
            } else if let list = try? await client.createContainer(name: name, key: nil) {
                _ = try? await client.put(collection: "list", container: list, id: "info", value: J(["color": color, "created": nowMs()]).text)
                view = list
            }
        }
    }

    /// Deletes the list everywhere; a list someone shared with you is only left.
    func deleteList(_ id: String) {
        listSheet = nil
        view = "today"
        Task { try? await client.deleteContainer(id: id) }
    }

    func share(_ id: String) {
        listSheet = nil
        if !client.remote() { uuob.open("share") }
        Task { try? await client.share(id: id) }
    }

    func setAlerts(_ on: Bool) {
        alerts = on
        scheduleUpcoming()
        Task { _ = try? await client.put(collection: "prefs", container: nil, id: "alerts", value: J(["on": on]).text) }
    }
}

struct TasksView: View {
    @Bindable var model: TasksModel
    @Bindable var updater: Updater

    var body: some View {
        let s = model.screen
        HStack(spacing: 0) {
            Sidebar(model: model, updater: updater, s: s).frame(width: 290)
            Rectangle().fill(Look.line).frame(width: 1)
            MainList(model: model, s: s)
        }
        .ignoresSafeArea(.container, edges: .top)
        .foregroundStyle(Look.text)
        .sheet(isPresented: $model.settingsOpen) {
            SettingsSheet(app: tasksApp, settings: model.settings) {
                Card {
                    Trailing("At a task's time") {
                        Segmented([("on", "On"), ("off", "Off")], selected: model.alerts ? "on" : "off") { model.setAlerts($0 == "on") }
                    }
                } header: {
                    SectionLabel("Alerts")
                } footer: {
                    Text("For tasks with a time. They ring on your phones and computers, as Tasks notifications.")
                }
            }
        }
        .sheet(item: $model.listSheet) { sheet in ListSheet(model: model, start: sheet) }
    }
}

/// The sidebar: the smart views as tiles, the lists, Add list, Settings.
struct Sidebar: View {
    @Bindable var model: TasksModel
    @Bindable var updater: Updater
    let s: J

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                // Room for the window's buttons, and the update button when there is one.
                HStack { Spacer(); UpdateButton(updater: updater) }.frame(minHeight: 28).padding(.bottom, 8)
                LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible())], spacing: 10) {
                    ForEach(s.smart.array) { sm in
                        Button { model.go(sm.id) } label: { SmartTile(sm: sm, on: model.view == sm.id) }.buttonStyle(PressStyle())
                    }
                }
                SectionLabel("My lists").padding(.top, 26).padding(.horizontal, 8).padding(.bottom, 8)
                ForEach(s.lists.array) { l in
                    Button { model.go(l.id) } label: { ListRow(l: l, on: model.view == l.id) }.buttonStyle(.plain)
                }
                if s.lists.array.isEmpty {
                    Text("Group tasks into lists like Home, Work or Groceries.").font(.system(size: 13)).foregroundStyle(Look.text2).padding(.horizontal, 8).padding(.bottom, 6)
                }
                Button { model.newList() } label: { IconLabel("plus", "Add list") }.buttonStyle(UuobButton(.ghost)).padding(.top, 8)
            }
            .padding(.horizontal, 16).padding(.top, 20)
        }
        .safeAreaInset(edge: .bottom) {
            HStack {
                Button { model.settingsOpen = true } label: { IconLabel("settings", "Settings") }.buttonStyle(UuobButton(.quiet))
                Spacer()
            }
            .padding(.horizontal, 16).padding(.bottom, 18)
        }
        .background(Look.elevated)
    }
}

/// A button that shrinks a little while pressed (the tiles).
struct PressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.scaleEffect(configuration.isPressed ? 0.97 : 1).animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}

/// `.smart-tile`: a white tile with its round icon, count and name.
struct SmartTile: View {
    let sm: J
    let on: Bool
    @State private var hover = false
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Circled(icon: sm.icon.string, color: tint(sm.color.string), size: 30)
                Spacer()
                Text("\(sm.count.int)").font(.system(size: 24, weight: .bold))
            }
            Text(sm.name.string).font(.system(size: 15, weight: .semibold)).foregroundStyle(Look.text2)
        }
        .padding(12)
        .background(on ? Look.accentSoft : hover ? Look.hover : Look.surface, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(on ? Look.accent.opacity(0.5) : Look.line))
        .contentShape(Rectangle())
        .onHover { hover = $0 }
    }
}

/// `.list-row`.
struct ListRow: View {
    let l: J
    let on: Bool
    @State private var hover = false
    var body: some View {
        HStack(spacing: 12) {
            Circled(icon: "list", color: Palette.color(l.color.string), size: 26)
            Text(l.name.string).font(.system(size: 15, weight: .medium)).lineLimit(1)
            Spacer()
            if l.shared.bool { Ico("people", size: 15).foregroundStyle(Look.text3).help(l.note.string) }
            Text("\(l.count.int)").font(.system(size: 15)).foregroundStyle(Look.text2)
        }
        .padding(8)
        .background(on ? Look.active : hover ? Look.hover : .clear, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .contentShape(Rectangle())
        .onHover { hover = $0 }
    }
}

struct MainList: View {
    @Bindable var model: TasksModel
    let s: J
    @State private var draft = ""
    @FocusState private var typing: Bool

    var body: some View {
        let current = s.current
        let list = current.list
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 8) {
                    LargeTitle(current.name.string, color: current.color.string == "gray" ? Look.text : Palette.color(current.color.string))
                    if !list.isNull {
                        let from = !list.from.isNull
                        IconButton(from ? "people" : "edit", size: 19, help: from ? "About this list" : "Edit list") { model.listSheet = list }
                        if !from { IconButton("share", size: 19, help: "Share list") { model.share(list.id) } }
                    }
                    Spacer()
                    Text("\(s.count.int)").font(.system(size: 34, weight: .bold)).foregroundStyle(Look.text3)
                }
                .padding(.bottom, 18)
                if !list.note.string.isEmpty {
                    HStack(spacing: 6) { Ico("people", size: 14); Text(list.note.string) }.font(.system(size: 13)).foregroundStyle(Look.text2).padding(.top, -10).padding(.bottom, 14)
                }
                if current.canAdd.bool { quickAdd(list) }
                ForEach(s.groups.array, id: \.groupKey) { g in
                    if !g.key.string.isEmpty { Text(g.key.string).font(.system(size: 15, weight: .bold)).padding(.top, 22).padding(.horizontal, 4).padding(.bottom, 6) }
                    ForEach(g.items.array, id: \.rowKey) { x in TaskRow(model: model, x: x, lists: s.lists.array) }
                }
                if let e = Optional(s.empty), !e.isNull {
                    Empty(glyph: e.glyph.string, headline: e.headline.string, caption: e.caption.string)
                }
            }
            .frame(maxWidth: 760)
            .padding(.horizontal, 48).padding(.top, 48).padding(.bottom, 80)
            .frame(maxWidth: .infinity)
            .animation(.spring(duration: 0.26), value: s.groups)
        }
        .background(Look.bg)
        .onChange(of: model.view) { draft = "" }
    }

    /// `.quick`: a white field with a blue plus, and what the date will be while typing.
    private func quickAdd(_ list: J) -> some View {
        HStack(spacing: 12) {
            Ico("plus", size: 20, stroke: 2.2).foregroundStyle(Look.accent)
            TextField("New task — try “call mom fri 5pm”", text: $draft)
                .textFieldStyle(.plain).font(.system(size: 17)).focused($typing)
                .onSubmit {
                    guard !draft.trimmingCharacters(in: .whitespaces).isEmpty else { return }
                    model.add(draft, list: list.id.str)
                    draft = ""
                }
            if !draft.trimmingCharacters(in: .whitespaces).isEmpty, let p = quickPreview(text: draft, hour12: model.settings.hour12) {
                Text(p).font(.system(size: 13, weight: .semibold)).foregroundStyle(Look.accent)
                    .padding(.horizontal, 10).padding(.vertical, 4).background(Look.accentSoft, in: Capsule())
            }
        }
        .padding(.horizontal, 16).frame(height: 54)
        .background(Look.surface, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(typing ? Look.accent : Look.line))
        .overlay(RoundedRectangle(cornerRadius: 17, style: .continuous).stroke(typing ? Look.accentSoft : .clear, lineWidth: 4).padding(-3))
        .animation(.easeOut(duration: 0.18), value: typing)
        .padding(.bottom, 16)
    }
}

extension J {
    var groupKey: String { self["key"].string }
    var rowKey: String { "\(self["container"].string)/\(id)" }
}

struct TaskRow: View {
    @Bindable var model: TasksModel
    let x: J
    let lists: [J]
    @State private var hover = false

    var body: some View {
        let show = x.show
        let open = model.openId == x.id
        let done = x.done.bool
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 14) {
                Button { withAnimation(.easeOut(duration: 0.22)) { model.toggle(x) } } label: {
                    Check(on: done, ring: show.ring.str.map(Palette.color))
                }
                .buttonStyle(PressStyle()).disabled(!show.canEdit.bool).opacity(show.canEdit.bool ? 1 : 0.45).padding(.top, 1)
                Button { if show.canEdit.bool { model.openId = open ? nil : x.id } } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(x.title.string).font(.system(size: 17)).strikethrough(done, color: Look.text3).foregroundStyle(done ? Look.text3 : Look.text)
                        if !x.notes.string.isEmpty && !open {
                            Text(x.notes.string).font(.system(size: 14)).foregroundStyle(Look.text2).lineLimit(1)
                        }
                        if show.when.str != nil || show.listName.str != nil {
                            HStack(spacing: 10) {
                                if let w = show.when.str {
                                    Text(w).foregroundStyle(show.late.bool ? Look.danger : Look.text2).fontWeight(show.late.bool ? .medium : .regular)
                                }
                                if let n = show.listName.str { Text(n).fontWeight(.medium).foregroundStyle(Palette.color(show.listColor.string)) }
                            }
                            .font(.system(size: 14))
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .padding(12)
            if open { Editor(model: model, x: x, lists: lists).transition(.opacity.combined(with: .move(edge: .top))) }
        }
        .background(open ? Look.surface : hover ? Look.hover : .clear, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .shadow(color: .black.opacity(open ? 0.1 : 0), radius: 6, y: 2)
        .onHover { hover = $0 }
        .opacity(show.finishing.bool ? 0.35 : 1)
        .animation(.easeOut(duration: 0.5).delay(0.25), value: show.finishing.bool)
        .padding(.vertical, open ? 8 : 0)
    }
}

/// A task opened in place: title, notes, date, time, list, Delete and Done.
struct Editor: View {
    @Bindable var model: TasksModel
    let x: J
    let lists: [J]
    @State private var title = ""
    @State private var notes = ""
    @FocusState private var focus: Int?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            TextField("Title", text: $title).focused($focus, equals: 1)
                .onSubmit { model.set(x, "title", title) }
            TextField("Notes", text: $notes, axis: .vertical).lineLimit(2...6).focused($focus, equals: 2)
            HStack(alignment: .bottom, spacing: 10) {
                Stack("Date") {
                    HStack(spacing: 6) {
                        if let due = x.due.str, let d = ISO8601DateFormatter.day.date(from: due) {
                            DatePicker("", selection: Binding(get: { d }, set: { model.set(x, "due", ISO8601DateFormatter.day.string(from: $0)) }), displayedComponents: .date)
                                .labelsHidden().datePickerStyle(.field)
                                .padding(.horizontal, 10).frame(height: 46)
                                .background(Look.surface2, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Look.line))
                            IconButton("close", size: 16, help: "No date") { model.set(x, "due", nil) }
                        } else {
                            Button("Add date") { model.set(x, "due", today()) }.frame(height: 46)
                        }
                    }
                }
                Stack("Time") {
                    TimeField(value: Binding(get: { x.time.str }, set: { model.set(x, "time", $0) }), hour12: model.settings.hour12, optional: true, placeholder: "None")
                        .frame(width: 120)
                }
                Stack("List") {
                    Menu {
                        Button("None") { model.move(x, to: nil) }
                        ForEach(lists.filter { $0.canWrite.value == nil || $0.canWrite.bool }) { l in Button(l.name.string) { model.move(x, to: l.id) } }
                    } label: {
                        HStack {
                            Text(lists.first { $0.id == x.list.str }?.name.string ?? "None").foregroundStyle(Look.text)
                            Spacer()
                            Ico("chevron", size: 14).rotationEffect(.degrees(90)).foregroundStyle(Look.text2)
                        }
                        .font(.system(size: 17))
                        .padding(.horizontal, 14).frame(height: 46)
                        .background(Look.surface2, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Look.line))
                        .contentShape(Rectangle())
                    }
                    .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden)
                    .frame(maxWidth: 220)
                }
            }
            HStack {
                Button { model.remove(x) } label: { IconLabel("trash", "Delete") }.buttonStyle(UuobButton(.danger))
                Spacer()
                Button("Done") { commit(); model.openId = nil }
            }
            .padding(.top, 4)
        }
        .textFieldStyle(UuobField())
        .buttonStyle(UuobButton())
        .padding(.leading, 48).padding(.trailing, 14).padding(.bottom, 14)
        .onAppear {
            title = x.title.string
            notes = x.notes.string
        }
        // Title and notes save when they are left (as `onchange` did).
        .onChange(of: focus) { old, _ in
            if old == 1 { model.set(x, "title", title) }
            if old == 2 { model.set(x, "notes", notes) }
        }
        .onDisappear(perform: commit)
    }

    private func commit() {
        model.set(x, "title", title)
        model.set(x, "notes", notes)
    }
}

extension ISO8601DateFormatter {
    /// "YYYY-MM-DD" in this Mac's time zone.
    static let day: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withFullDate, .withDashSeparatorInDate]
        f.timeZone = .current
        return f
    }()
}

/// New list / edit a list (`.sheet` with the hero circle and swatches), or one shared with you.
struct ListSheet: View {
    @Bindable var model: TasksModel
    let start: J
    @State private var name = ""
    @State private var color = "blue"

    var body: some View {
        let id = start.id.isEmpty ? nil : start.id
        Group {
            if !start.from.isNull {
                SheetFrame(title: start.name.string, lead: "\(start.from.name.string) shares this list with you. " + (start.canWrite.bool ? "You can both change it." : "You can view it, but not change it.")) {
                    EmptyView()
                } buttons: {
                    Button("Leave list", role: .destructive) { if let id { model.deleteList(id) } }
                    Button("Done") { model.listSheet = nil }.buttonStyle(UuobButton(.primary, large: true)).keyboardShortcut(.defaultAction)
                }
            } else {
                SheetFrame(title: "", center: true) {
                    Circled(icon: "list", color: Palette.color(color), size: 72)
                        .shadow(color: .black.opacity(0.15), radius: 8, y: 3).padding(.bottom, 4)
                    TextField("List name", text: $name)
                        .font(.system(size: 20, weight: .semibold)).multilineTextAlignment(.center).frame(height: 54)
                        .onSubmit { model.saveList(id: id, name: name, color: color, created: start.created.int64) }
                    HStack(spacing: 12) {
                        ForEach(Palette.names, id: \.self) { c in
                            Button { color = c } label: {
                                Circle().fill(Palette.color(c)).frame(width: 32, height: 32)
                                    .overlay(Circle().strokeBorder(Look.elevated, lineWidth: c == color ? 3 : 0))
                                    .overlay(Circle().strokeBorder(Look.text2, lineWidth: c == color ? 2 : 0).padding(-5))
                            }
                            .buttonStyle(PressStyle()).help(c)
                        }
                    }
                    .padding(.vertical, 6)
                    if let id {
                        Button { model.share(id) } label: { IconLabel("share", start.sharedWith.array.isEmpty ? "Share list…" : start.note.string) }
                            .buttonStyle(UuobButton(.ghost))
                    }
                } buttons: {
                    if let id {
                        Button("Delete", role: .destructive) { model.deleteList(id) }
                    } else {
                        Button("Cancel") { model.listSheet = nil }.keyboardShortcut(.cancelAction)
                    }
                    Button(id == nil ? "Create" : "Save") { model.saveList(id: id, name: name, color: color, created: start.created.int64) }
                        .buttonStyle(UuobButton(.primary, large: true)).keyboardShortcut(.defaultAction)
                        .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
        .onAppear {
            name = start.name.string
            color = start.color.str ?? "blue"
        }
    }
}
