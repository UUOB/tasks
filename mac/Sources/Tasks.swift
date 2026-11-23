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
                // Small windows work too: the lists fold to a rail.
                .frame(minWidth: 380, minHeight: 420)
                .background(Look.bg)
        }
        // No title bar: the sidebar and the list are the window, as on the web.
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1100, height: 760)
        .commands { Self.commands(model) }
    }

    /// The shared shortcuts (docs/design.md) and Tasks' own: New list, the four views.
    @MainActor static func commands(_ model: TasksModel) -> UuobCommands {
        var extra: [Shortcut] = [Shortcut(title: "New List", key: "n", modifiers: [.command, .shift]) { model.newList() }]
        for (i, v) in [("Today", "today"), ("Upcoming", "upcoming"), ("All", "all"), ("Completed", "completed")].enumerated() {
            extra.append(Shortcut(title: v.0, key: KeyEquivalent(Character("\(i + 1)"))) { model.go(v.1) })
        }
        return UuobCommands(
            settings: { model.settingsOpen = true },
            new: { model.newTask() }, newTitle: "New Task",
            refresh: { Task { await model.load() } },
            extraMenu: "Go", extra: extra
        )
    }
}

func nowMs() -> Int64 { Int64(Date().timeIntervalSince1970 * 1000) }

// The core's answers (core/src/lib.rs), named apart from the model's own `screen`.
private func coreScreen(_ state: String) -> String { screen(state: state) }
private func coreUpcoming(_ state: String) -> String { upcoming(state: state) }

/// A colour as the core names it ("gray" for All).
func listTint(_ name: String) -> Color { name == "gray" ? Color(light: 0x8e8e93, dark: 0x8e8e93) : Palette.color(name) }

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
    /// The task whose card is shown, and the one being edited (as the screen drew them).
    var card: J?
    var editing: J?
    /// Asks the quick field for the keyboard (New task: ⌘N or C).
    var quickFocus = 0
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
    }

    /// New task: the quick field, ready for typing (Today, where a view has none).
    func newTask() {
        if !screen.current.canAdd.bool { view = "today" }
        quickFocus += 1
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

    func remove(_ x: J) {
        card = nil
        editing = nil
        tasks = J(J(text: tasks).array.filter { !($0.id == x.id && $0.container == x.container) }.map { $0.value as Any }).text
        Task { _ = try? await client.delete(collection: "tasks", container: arg(x.container.str), id: x.id) }
    }

    /// Moving a task to another list moves it to that list's container.
    /// `edited`: the task as its editor left it (else as it is now).
    func move(_ x: J, to list: String?, edited: Bool = false) {
        let x = edited ? x : current(x)
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
        let color = s.current.color.string
        let tint: Color? = color == "gray" || color == "blue" || color.isEmpty ? nil : Palette.color(color)
        // The views and lists on the left (a rail of their circles when folded or narrow), the list on the right.
        SidebarScaffold {
            Sidebar(model: model, updater: updater, s: s)
        } rail: {
            TasksRail(model: model, s: s)
        } content: {
            MainList(model: model, s: s).uuobTint(tint)
        } footer: {
            SettingsFoot { model.settingsOpen = true }
        } railFooter: {
            RailButton("settings", help: "Settings (⌘,)") { model.settingsOpen = true }
        }
        // ⌘K, ⌘?, and C / ? where no field is typing (docs/design.md).
        .uuobKeys(TasksApp.commands(model))
        .ignoresSafeArea(.container, edges: .top)
        .foregroundStyle(Look.text)
        .sheet(isPresented: $model.settingsOpen) {
            SettingsSheet(app: tasksApp, settings: model.settings) {
                RowsLabel("Tasks")
                IconRow("bell", last: true) {
                    Text("Alerts at a task's time")
                    Spacer(minLength: 8)
                    Segmented([("on", "On"), ("off", "Off")], selected: model.alerts ? "on" : "off") { model.setAlerts($0 == "on") }
                }
            }
        }
        .sheet(item: $model.card) { x in TaskCard(model: model, x: x, lists: s.lists.array).uuobTint(sheetTint(x, tint)) }
        .sheet(item: $model.editing) { x in TaskEditor(model: model, x: x, lists: s.lists.array).uuobTint(sheetTint(x, tint)) }
        .sheet(item: $model.listSheet) { sheet in ListSheet(model: model, start: sheet) }
    }

    /// A task's sheets take its list's colour (in a smart view) or the page's.
    private func sheetTint(_ x: J, _ page: Color?) -> Color? {
        if let c = x.show.listColor.str { return Palette.color(c) }
        return page
    }
}

/// A coloured circle with a white icon (`.circle`), centred.
private func mark(_ color: Color, _ icon: String, _ size: CGFloat) -> some View {
    Circled(icon: icon, color: color, size: size)
}

/// The sidebar (the kit's): the views, the lists, New list, Settings.
struct Sidebar: View {
    @Bindable var model: TasksModel
    @Bindable var updater: Updater
    let s: J
    @Environment(\.uuobClosePeek) private var closePeek

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            // Room for the window's buttons, and the update button when there is one.
            HStack { Spacer(); UpdateButton(updater: updater) }.frame(minHeight: 28).padding(.bottom, 8)
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(s.smart.array) { sm in
                        SidebarRow(title: sm.name.string, selected: model.view == sm.id) {
                            mark(listTint(sm.color.string), sm.icon.string, 22)
                        } trailing: {
                            Text("\(sm.count.int)")
                        } action: { model.go(sm.id); closePeek() }
                    }
                    if !s.lists.array.isEmpty {
                        RowsLabel("My lists").padding(.horizontal, 10).padding(.top, 12).padding(.bottom, 4)
                    }
                    ForEach(s.lists.array) { l in
                        SidebarRow(title: l.name.string, selected: model.view == l.id) {
                            mark(Palette.color(l.color.string), "list", 22)
                        } trailing: {
                            HStack(spacing: 6) {
                                if l.shared.bool { Ico("people", size: 14).uuobHint(l.note.string) }
                                Text("\(l.count.int)")
                            }
                        } action: { model.go(l.id); closePeek() }
                    }
                    SidebarRow(title: "New list") { Ico("plus", size: 18, stroke: 2).foregroundStyle(Look.text2) } action: { closePeek(); model.newList() }
                        .padding(.top, 6)
                }
            }
            .scrollIndicators(.never)
        }
        .padding(.horizontal, 12).padding(.top, 20).padding(.bottom, 4)
    }
}

/// Folded or narrow: the views' and lists' circles, New list, Settings.
struct TasksRail: View {
    @Bindable var model: TasksModel
    let s: J

    var body: some View {
        VStack(spacing: 6) {
            Color.clear.frame(height: 30) // the window's buttons
            ForEach(s.smart.array) { sm in
                RailButton(help: sm.name.string, selected: model.view == sm.id) { mark(listTint(sm.color.string), sm.icon.string, 24) } action: { model.go(sm.id) }
            }
            if !s.lists.array.isEmpty { Rectangle().fill(Look.line).frame(width: 28, height: 1).padding(.vertical, 4) }
            ForEach(s.lists.array) { l in
                RailButton(help: l.name.string, selected: model.view == l.id) { mark(Palette.color(l.color.string), "list", 22) } action: { model.go(l.id) }
            }
            RailButton("plus", help: "New list (⇧⌘N)") { model.newList() }
            Spacer()
        }
        .padding(.top, 4)
    }
}

/// A button that shrinks a little while pressed (the swatches).
struct PressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.scaleEffect(configuration.isPressed ? 0.97 : 1).animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}

struct MainList: View {
    @Bindable var model: TasksModel
    let s: J
    @State private var draft = ""
    @FocusState private var typing: Bool
    @Environment(\.uuobCompact) private var compact
    @Environment(\.uuobTint) private var tint

    var body: some View {
        let current = s.current
        let list = current.list
        let color = current.color.string
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Toolbar(title: titleText(current.name.string, "\(s.count.int)").foregroundStyle(color == "gray" ? Look.text : Palette.color(color)), compact: compact) {
                    if !list.isNull {
                        let from = !list.from.isNull
                        HStack(spacing: 4) {
                            IconButton(from ? "people" : "edit", size: 19, help: from ? "About this list" : "Edit list") { model.listSheet = list }
                            if !from { IconButton("share", size: 19, help: "Share list") { model.share(list.id) } }
                        }
                    }
                }
                if !list.note.string.isEmpty {
                    HStack(spacing: 6) { Ico("people", size: 14); Text(list.note.string) }.font(.system(size: 13)).foregroundStyle(Look.text2).padding(.top, -10).padding(.bottom, 14)
                }
                if current.canAdd.bool { quickAdd(list) }
                ForEach(s.groups.array, id: \.groupKey) { g in
                    if !g.key.string.isEmpty { Text(g.key.string).font(.system(size: 15, weight: .bold)).padding(.top, 22).padding(.horizontal, 4).padding(.bottom, 6) }
                    let items = g.items.array
                    ForEach(Array(items.enumerated()), id: \.element.rowKey) { i, x in
                        TaskRow(model: model, x: x, last: i == items.count - 1)
                    }
                }
                if let e = Optional(s.empty), !e.isNull {
                    EmptyState(icon: "check", headline: e.headline.string)
                }
            }
            .frame(maxWidth: 760)
            .padding(.horizontal, compact ? 14 : 28).padding(.top, compact ? 40 : 48).padding(.bottom, 80)
            .frame(maxWidth: .infinity)
            .animation(.spring(duration: 0.26), value: s.groups)
        }
        .scrollIndicators(.never)
        .background(Look.bg)
        .onChange(of: model.view) { draft = "" }
        .onChange(of: model.quickFocus) { typing = true }
    }

    /// `.quick`: a white field with a plus in the list's colour, and what the date will be while typing.
    private func quickAdd(_ list: J) -> some View {
        let accent = tint ?? Look.accent
        return HStack(spacing: 12) {
            Ico("plus", size: 20, stroke: 2.2).foregroundStyle(accent)
            TextField("New task — try “call mom fri 5pm”", text: $draft)
                .textFieldStyle(.plain).font(.system(size: 17)).focused($typing)
                .onSubmit {
                    guard !draft.trimmingCharacters(in: .whitespaces).isEmpty else { return }
                    model.add(draft, list: list.id)
                    draft = ""
                }
            if !draft.trimmingCharacters(in: .whitespaces).isEmpty, let p = quickPreview(text: draft, hour12: model.settings.hour12) {
                Text(p).font(.system(size: 13, weight: .semibold)).foregroundStyle(accent)
                    .padding(.horizontal, 10).padding(.vertical, 4).background(accent.opacity(0.16), in: Capsule())
            }
        }
        .padding(.horizontal, 16).frame(height: 54)
        .background(Look.surface, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(typing ? accent : Look.line))
        .overlay(RoundedRectangle(cornerRadius: 17, style: .continuous).stroke(typing ? accent.opacity(0.16) : .clear, lineWidth: 4).padding(-3))
        .animation(.easeOut(duration: 0.18), value: typing)
        .padding(.bottom, 16)
    }
}

extension J {
    var groupKey: String { self["key"].string }
    var rowKey: String { "\(self["container"].string)/\(id)" }
}

/// A task: its tick, title, notes and when; a click opens its card.
struct TaskRow: View {
    @Bindable var model: TasksModel
    let x: J
    var last = false
    @State private var hover = false

    var body: some View {
        let show = x.show
        let done = x.done.bool
        HStack(alignment: .top, spacing: 14) {
            Button { withAnimation(.easeOut(duration: 0.22)) { model.toggle(x) } } label: {
                Check(on: done, ring: show.ring.str.map(Palette.color))
            }
            .buttonStyle(PressStyle()).disabled(!show.canEdit.bool).opacity(show.canEdit.bool ? 1 : 0.45).padding(.top, 1)
            Button { model.card = x } label: {
                VStack(alignment: .leading, spacing: 2) {
                    Text(x.title.string).font(.system(size: 17)).strikethrough(done, color: Look.text3).foregroundStyle(done ? Look.text3 : Look.text)
                    if !x.notes.string.isEmpty {
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
        .padding(.horizontal, 8).padding(.vertical, 11)
        .background(hover ? Look.hover : .clear, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(alignment: .bottom) { if !last { Rectangle().fill(Look.line).frame(height: 1).padding(.leading, 44) } }
        .onHover { hover = $0 }
        .opacity(show.finishing.bool ? 0.35 : 1)
        .animation(.easeOut(duration: 0.5).delay(0.25), value: show.finishing.bool)
    }
}

/// The task's card (the kit's detail card): when, its list, notes; Edit (⌘E), Delete, Complete.
struct TaskCard: View {
    @Bindable var model: TasksModel
    let x: J
    let lists: [J]

    var body: some View {
        let show = x.show
        let list = lists.first { $0.id == x.list.str }
        let rows = [show.when.str != nil ? "when" : nil, list != nil ? "list" : nil, x.notes.string.isEmpty ? nil : "notes"].compactMap { $0 }
        DetailSheet(title: x.title.string, onClose: { model.card = nil }) {
            if show.canEdit.bool {
                IconButton("edit", size: 19, help: "Edit (⌘E)") { model.card = nil; model.editing = x }.keyboardShortcut("e")
            }
        } content: {
            VStack(spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.offset) { i, r in
                    let last = i == rows.count - 1
                    switch r {
                    case "when":
                        IconRow("clock", last: last) { Text(show.when.string).foregroundStyle(show.late.bool ? Look.danger : Look.text) }
                    case "list":
                        IconRow("list", last: last) {
                            Circle().fill(Palette.color(list?.color.string ?? "blue")).frame(width: 10, height: 10)
                            Text(list?.name.string ?? "")
                        }
                    default:
                        IconRow("notes", last: last) { Text(x.notes.string).textSelection(.enabled).padding(.vertical, 10).frame(maxWidth: .infinity, alignment: .leading) }
                    }
                }
            }
            .padding(.top, 6)
        } footer: {
            if show.canEdit.bool {
                HStack {
                    Button("Delete", role: .destructive) { model.remove(x) }.buttonStyle(UuobButton(.danger, small: true)).keyboardShortcut(.delete, modifiers: .command)
                    Spacer()
                    Button(x.done.bool ? "Not done" : "Complete") { model.card = nil; model.toggle(x) }.buttonStyle(UuobButton(.primary, small: true))
                }
                .padding(.top, 18)
            }
        }
    }
}

/// The task's editor (the kit's editor): the title, its list under it, notes, date, time. Nothing changes until Save.
struct TaskEditor: View {
    @Bindable var model: TasksModel
    let x: J
    let lists: [J]
    @State private var title = ""
    @State private var notes = ""
    @State private var due: String?
    @State private var time: String?
    @State private var list = ""

    var body: some View {
        let writable = lists.filter { $0.canWrite.value == nil || $0.canWrite.bool }
        EditSheet(placeholder: "Task", title: $title, canSave: !title.trimmingCharacters(in: .whitespaces).isEmpty,
                  onCancel: { model.editing = nil }, onSave: save, onDelete: { model.remove(x) }) {
            HStack(spacing: 6) {
                Ico("list", size: 15).foregroundStyle(Look.text2)
                Menu {
                    Button("No list") { list = "" }
                    ForEach(writable) { l in Button(l.name.string) { list = l.id } }
                } label: {
                    HStack(spacing: 4) {
                        Text(writable.first { $0.id == list }?.name.string ?? "No list").font(.system(size: 14, weight: .semibold))
                        Ico("chevron", size: 12).rotationEffect(.degrees(90)).foregroundStyle(Look.text2)
                    }
                    .contentShape(Rectangle())
                }
                .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
            }
        } content: {
            VStack(spacing: 0) {
                IconRow("notes") {
                    TextField("Notes", text: $notes, axis: .vertical).textFieldStyle(.plain).lineLimit(1...6).padding(.vertical, 10)
                }
                IconRow("calendar") {
                    if let d = due.flatMap({ ISO8601DateFormatter.day.date(from: $0) }) {
                        DatePicker("", selection: Binding(get: { d }, set: { due = ISO8601DateFormatter.day.string(from: $0) }), displayedComponents: .date)
                            .labelsHidden().datePickerStyle(.field)
                        Spacer()
                        IconButton("close", size: 16, help: "No date") { due = nil }
                    } else {
                        Button("Add date") { due = today() }.buttonStyle(.plain).foregroundStyle(Look.text3)
                        Spacer()
                    }
                }
                IconRow("clock", last: true) {
                    TimeField(value: $time, hour12: model.settings.hour12, optional: true, placeholder: "No time").frame(width: 120)
                    Spacer()
                }
            }
        }
        .onAppear {
            title = x.title.string
            notes = x.notes.string
            due = x.due.str
            time = x.time.str
            list = x.list.str ?? ""
        }
    }

    private func save() {
        var y = (model.current(x).value as? [String: Any]) ?? [:]
        y["title"] = title.trimmingCharacters(in: .whitespaces)
        y["notes"] = notes
        y["due"] = due ?? NSNull()
        y["time"] = time ?? NSNull()
        model.editing = nil
        if list != (x.list.str ?? "") {
            model.move(J(y), to: list.isEmpty ? nil : list, edited: true)
        } else {
            model.save(J(y))
        }
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

/// New list / edit a list (the kit's editor: name, colour, sharing), or a card for one shared with you.
struct ListSheet: View {
    @Bindable var model: TasksModel
    let start: J
    @State private var name = ""
    @State private var color = "blue"

    var body: some View {
        let id = start.id.isEmpty ? nil : start.id
        Group {
            if !start.from.isNull {
                DetailSheet(title: start.name.string, onClose: { model.listSheet = nil }) {
                    EmptyView()
                } content: {
                    IconRow("people", last: true) {
                        Text("\(start.from.name.string) shares this list with you. " + (start.canWrite.bool ? "You can both change it." : "You can view it, but not change it."))
                            .fixedSize(horizontal: false, vertical: true).padding(.vertical, 10)
                    }
                    .padding(.top, 6)
                } footer: {
                    Button("Leave list", role: .destructive) { if let id { model.deleteList(id) } }.buttonStyle(UuobButton(.danger, small: true)).padding(.top, 18)
                }
            } else {
                EditSheet(placeholder: "List name", title: $name, isNew: id == nil, canSave: !name.trimmingCharacters(in: .whitespaces).isEmpty,
                          saveLabel: id == nil ? "Create" : nil,
                          onCancel: { model.listSheet = nil },
                          onSave: { model.saveList(id: id, name: name, color: color, created: start.created.int64) },
                          onDelete: id.map { i in { model.deleteList(i) } }) {
                    VStack(spacing: 0) {
                        IconRow("list", last: id == nil) {
                            HStack(spacing: 10) {
                                ForEach(Palette.names, id: \.self) { c in
                                    Button { color = c } label: {
                                        Circle().fill(Palette.color(c)).frame(width: 26, height: 26)
                                            .overlay(Circle().strokeBorder(Look.elevated, lineWidth: c == color ? 3 : 0))
                                            .overlay(Circle().strokeBorder(Look.text2, lineWidth: c == color ? 2 : 0).padding(-4))
                                    }
                                    .buttonStyle(PressStyle()).uuobHint(c)
                                }
                            }
                            .padding(.vertical, 8)
                        }
                        if let id {
                            IconRow("share", last: true) {
                                Button(start.sharedWith.array.isEmpty ? "Share list…" : start.note.string) { model.share(id) }.buttonStyle(.plain)
                                Spacer()
                            }
                        }
                    }
                }
                .uuobTint(Palette.color(color))
            }
        }
        .onAppear {
            name = start.name.string
            color = start.color.str ?? "blue"
        }
    }
}

/// Settings on the toggle's line at the sidebar's foot; closes the sidebar
/// shown over a narrow window first.
struct SettingsFoot: View {
    let open: () -> Void
    @Environment(\.uuobClosePeek) private var closePeek
    var body: some View {
        SidebarRow(title: "Settings") { Ico("settings", size: 19).foregroundStyle(Look.text2) } action: { closePeek(); open() }
            .uuobHint("Settings (⌘,)")
    }
}
