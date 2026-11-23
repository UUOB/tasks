package app.uuob.tasks

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import app.uuob.kit.DetailSheet
import app.uuob.kit.EditSheet
import app.uuob.kit.EmptyState
import app.uuob.kit.IconRow
import app.uuob.kit.RailButton
import app.uuob.kit.RailIcon
import app.uuob.kit.RowsLabel
import app.uuob.kit.Shortcuts
import app.uuob.kit.ShortcutsHost
import app.uuob.kit.SidebarRow
import app.uuob.kit.SidebarScaffold
import app.uuob.kit.LocalDrawerMode
import app.uuob.kit.Toolbar
import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.scale
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.uuob.kit.Btn
import app.uuob.kit.BtnKind
import app.uuob.kit.Accent
import app.uuob.kit.Check
import app.uuob.kit.Colors
import app.uuob.kit.Empty
import app.uuob.kit.Icon24
import app.uuob.kit.IconBtn
import app.uuob.kit.LargeTitle
import app.uuob.kit.Pill
import app.uuob.kit.SectionLabel
import app.uuob.kit.Segmented
import app.uuob.kit.SettingsGroup
import app.uuob.kit.SettingsRow
import app.uuob.kit.SettingsSheet
import app.uuob.kit.Stack
import app.uuob.kit.TimeField
import app.uuob.kit.Tokens
import app.uuob.kit.UpdateButton
import app.uuob.kit.Updater
import app.uuob.kit.Uuob
import app.uuob.kit.UuobSettings
import app.uuob.kit.WebField
import app.uuob.kit.WebSheet
import app.uuob.kit.objects
import app.uuob.kit.text
import app.uuob.tasks.core.alertsOn
import app.uuob.tasks.core.moveOldLists
import app.uuob.tasks.core.nextColor
import app.uuob.tasks.core.quickPreview
import app.uuob.tasks.core.quickTask
import app.uuob.tasks.core.screen
import app.uuob.tasks.core.stored
import app.uuob.tasks.core.toggled
import app.uuob.tasks.core.upcoming
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset

private val COLORS = listOf("blue", "red", "orange", "yellow", "green", "teal", "purple", "pink")

/**
 * Tasks' state: every task, list and preference, kept live from UUOB. What
 * the screens show comes from the core (core/src/lib.rs, `screen`); this
 * only reads, writes and remembers what the user is looking at.
 */
class TasksModel(private val ctx: Context) {
  private val client get() = Uuob.client
  var tasks by mutableStateOf("[]")
  var containers by mutableStateOf("[]")
  var infos by mutableStateOf("[]")
  var alerts by mutableStateOf(true)
  var tasksKnown by mutableStateOf(false)
  var alertsKnown by mutableStateOf(false)
  var view by mutableStateOf("today")
  /** The phone's drawer (the sidebar), open or not. */
  var drawerOpen by mutableStateOf(false)
  var finishing by mutableStateOf(setOf<String>())
  /** The task whose card is shown, and the one being edited (as the screen drew them). */
  var card by mutableStateOf<JSONObject?>(null)
  var editing by mutableStateOf<JSONObject?>(null)
  /** Asks the quick field for the keyboard (New task: Ctrl+N or C). */
  var quickFocus by mutableIntStateOf(0)
  var listSheet by mutableStateOf<JSONObject?>(null)
  var settingsOpen by mutableStateOf(false)
  var hourTick by mutableIntStateOf(0)
  private var reloading: Job? = null

  /** The app's first container: tasks in no list. */
  private val main get() = objects(containers).firstOrNull { it.optBoolean("main") }?.optString("id") ?: "tasks"
  private fun containerArg(c: String?) = if (c == null || c == main) null else c

  val screen by derivedStateOf {
    val state = JSONObject()
      .put("tasks", JSONArray(tasks)).put("containers", JSONArray(containers)).put("infos", JSONArray(infos))
      .put("view", view).put("finishing", JSONArray(finishing.toList())).put("hour12", UuobSettings.hour12)
    JSONObject(screen(state.toString()))
  }

  /** What is coming up, for alerts: tasks due by two weeks from now. */
  fun upcomingList(): String = upcoming(
    JSONObject().put("tasks", JSONArray(tasks)).put("containers", JSONArray(containers)).put("infos", JSONArray(infos)).put("alerts", alerts).toString(),
  )

  suspend fun start() {
    // What was seen last, at once; then UUOB's.
    tasks = client.cachedAll("tasks")
    infos = client.cachedAll("list")
    client.cachedContainers()?.let { containers = it }
    client.cached("prefs", null)?.let { alerts = alertsOn(it) }
    Uuob.scope.launch { runCatching { moveOldLists(client) } }
    reload()
    // Changes in UUOB, and UUOB coming back. (While it is away, each failed
    // call says so; reloading on those would never end.)
    Uuob.events.collect { e ->
      if (e.optString("type") == "changed" || (e.optString("type") == "state" && e.optString("state") == "connected")) scheduleReload()
    }
  }

  private fun scheduleReload() {
    reloading?.cancel()
    reloading = Uuob.scope.launch { delay(150); reload() }
  }

  suspend fun reload() {
    runCatching { containers = client.containers() }
    val all = JSONObject(client.all("tasks"))
    tasks = all.getJSONArray("records").toString()
    if (all.optBoolean("complete")) tasksKnown = true
    infos = JSONObject(client.all("list")).getJSONArray("records").toString()
    runCatching {
      alerts = alertsOn(client.load("prefs", null))
      alertsKnown = true
    }
  }

  private fun replace(x: JSONObject) {
    val id = x.optString("id")
    tasks = JSONArray(objects(tasks).map { if (it.optString("id") == id) x else it }).toString()
  }

  fun go(id: String) {
    view = id
    drawerOpen = false
  }

  /** New task: the quick field, ready for typing (Today, where a view has none). */
  fun newTask() {
    if (!screen.getJSONObject("current").optBoolean("canAdd")) view = "today"
    drawerOpen = false
    quickFocus++
  }

  fun add(text: String, currentList: String?) {
    val record = quickTask(text, view)
    if (record == "null") return
    Uuob.scope.launch { runCatching { client.put("tasks", containerArg(currentList), null, record) } }
  }

  fun toggle(x: JSONObject) {
    if (!x.getJSONObject("show").optBoolean("canEdit")) return
    val next = JSONObject(toggled(x.toString()))
    val id = x.optString("id")
    if (next.optBoolean("done")) {
      finishing = finishing + id
      Uuob.scope.launch { delay(700); finishing = finishing - id }
    }
    save(next)
  }

  fun save(x: JSONObject) {
    // Only a task still there: text typed into an editor whose task was just
    // deleted or moved must not bring the old copy back.
    val id = x.optString("id")
    val container = x.text("container")
    if (objects(tasks).none { it.optString("id") == id && it.text("container") == container }) return
    replace(x)
    Uuob.scope.launch { runCatching { client.put("tasks", containerArg(container), null, stored(x.toString())) } }
  }

  fun remove(x: JSONObject) {
    card = null
    editing = null
    val id = x.optString("id")
    tasks = JSONArray(objects(tasks).filter { it.optString("id") != id }).toString()
    Uuob.scope.launch { runCatching { client.delete("tasks", containerArg(x.text("container")), id) } }
  }

  /** Moving a task to another list moves it to that list's container. */
  fun move(x: JSONObject, list: String?) {
    val to = list ?: main
    val from = x.text("container") ?: main
    if (to == from) return
    replace(JSONObject(x.toString()).put("container", to))
    Uuob.scope.launch {
      runCatching {
        client.put("tasks", containerArg(to), null, stored(x.toString()))
        client.delete("tasks", containerArg(from), x.optString("id"))
      }
    }
  }

  fun newList() {
    val lists = screen.getJSONArray("lists").length()
    listSheet = JSONObject().put("name", "").put("color", nextColor(lists.toUInt())).put("created", System.currentTimeMillis())
  }

  fun saveList(sheet: JSONObject) {
    val name = sheet.optString("name").trim()
    if (name.isEmpty()) return
    listSheet = null
    val info = JSONObject().put("color", sheet.optString("color")).put("created", sheet.optLong("created"))
    Uuob.scope.launch {
      runCatching {
        val id = sheet.text("id")
        if (id != null) {
          val was = objects(screen.getJSONArray("lists").toString()).firstOrNull { it.optString("id") == id }?.optString("name")
          if (name != was) client.renameContainer(id, name)
          client.put("list", id, "info", info.toString())
        } else {
          val list = client.createContainer(name, null)
          info.put("created", System.currentTimeMillis())
          client.put("list", list, "info", info.toString())
          view = list
        }
      }
    }
  }

  /** Deletes the list everywhere; a list someone shared with you is only left. */
  fun deleteList(id: String) {
    listSheet = null
    view = "today"
    Uuob.scope.launch { runCatching { client.deleteContainer(id) } }
  }

  fun share(id: String) {
    listSheet = null
    Uuob.scope.launch { runCatching { Uuob.share(ctx, id) } }
  }

  fun changeAlerts(on: Boolean) {
    alerts = on
    Uuob.scope.launch { runCatching { client.put("prefs", null, "alerts", JSONObject().put("on", on).toString()) } }
  }
}

@Composable
fun TasksScreen(model: TasksModel, updater: Updater) {
  LaunchedEffect(Unit) { model.start() }
  // Written again whenever tasks or the setting change, and every hour, as the window moves on.
  LaunchedEffect(Unit) { while (true) { delay(3_600_000); model.hourTick++ } }
  val upcoming = if (model.tasksKnown && model.alertsKnown) model.upcomingList() else null
  LaunchedEffect(upcoming, model.hourTick) {
    if (upcoming == null) return@LaunchedEffect
    delay(1500)
    runCatching { Uuob.client.upcoming(model.upcomingList()) }
  }
  val s = model.screen
  // The list or view on screen lends its colour to everything in it: pluses, ticks, the field's edge, Save.
  val shown = s.optJSONObject("current")?.optString("color")
  val accent = if (shown.isNullOrEmpty() || shown == "gray") MaterialTheme.colorScheme.primary else Colors.of(shown)
  ShortcutsHost(tasksShortcuts(model)) {
    SidebarScaffold(model.drawerOpen, { model.drawerOpen = it }, sidebar = { Sidebar(model, s, updater, Modifier.weight(1f)) }, rail = { TasksRail(model, s) }) {
      Accent(accent) { MainList(model, s) }
    }
  }
  Accent(accent) {
    model.card?.let { TaskCard(model, it, s) }
    model.editing?.let { TaskEditor(model, it, s) }
  }
  if (model.settingsOpen) {
    SettingsSheet(onClose = { model.settingsOpen = false }) {
      SettingsGroup("Tasks") {
        SettingsRow("Alerts at a task's time", icon = "bell", last = true) { Segmented(listOf("On", "Off"), if (model.alerts) 0 else 1) { model.changeAlerts(it == 0) } }
      }
    }
  }
  model.listSheet?.let { sheet -> ListSheet(model, sheet) }
}

/** The keys (docs/design.md): the shared ones, New list, the four views. */
private fun tasksShortcuts(model: TasksModel) = Shortcuts(
  settings = { model.settingsOpen = true },
  new = "New task" to { model.newTask() },
  refresh = { Uuob.scope.launch { runCatching { model.reload() } } },
  extra = listOf(Shortcuts.Extra("New list", "Ctrl+Shift+N", Key.N, ctrl = true, shift = true) { model.newList() }) +
    listOf("Today" to "today", "Upcoming" to "upcoming", "All" to "all", "Completed" to "completed").zip(listOf(Key.One, Key.Two, Key.Three, Key.Four)).mapIndexed { i, (v, k) ->
      Shortcuts.Extra(v.first, "Ctrl+${i + 1}", k, true) { model.go(v.second) }
    },
)

/** A colour as the core names it ("gray" is `--text-2`). */
@Composable
private fun tint(name: String?): Color = if (name == "gray") Tokens.text2 else Colors.of(name)

/** `.circle`: a coloured disc with a white icon. */
@Composable
private fun Circle(color: Color, size: Int, icon: String, iconSize: Int, stroke: Float = 2.2f) {
  Box(Modifier.size(size.dp).clip(CircleShape).background(color), contentAlignment = Alignment.Center) {
    Icon24(icon, size = iconSize.dp, stroke = stroke, tint = Color.White)
  }
}

/** The sidebar (the kit's): the views, the lists, New list, Settings. */
@Composable
private fun Sidebar(model: TasksModel, s: JSONObject, updater: Updater, modifier: Modifier) {
  Column(modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(2.dp)) {
    Row(Modifier.fillMaxWidth().padding(bottom = 4.dp), horizontalArrangement = Arrangement.End) { UpdateButton(updater) }
    objects(s.getJSONArray("smart").toString()).forEach { sm ->
      SidebarRow(sm.optString("name"), model.view == sm.optString("id"), { model.go(sm.optString("id")) }, leading = { Circle(tint(sm.optString("color")), 22, sm.optString("icon"), 13) }) {
        Text("${sm.optInt("count")}", fontSize = 14.sp)
      }
    }
    val lists = objects(s.getJSONArray("lists").toString())
    if (lists.isNotEmpty()) RowsLabel("My lists", Modifier.padding(start = 10.dp, bottom = 4.dp))
    lists.forEach { l ->
      SidebarRow(l.optString("name"), model.view == l.optString("id"), { model.go(l.optString("id")) }, leading = { Circle(Colors.of(l.optString("color")), 22, "list", 12, 2.4f) }) {
        if (l.optBoolean("shared")) Icon24("people", size = 15.dp, tint = Tokens.text3)
        Text("${l.optInt("count")}", fontSize = 14.sp)
      }
    }
    Spacer(Modifier.height(4.dp))
    SidebarRow("New list", onClick = { model.drawerOpen = false; model.newList() }, leading = { Icon24("plus", size = 18.dp, stroke = 2f, tint = Tokens.text2) })
  }
  SidebarRow("Settings", onClick = { model.drawerOpen = false; model.settingsOpen = true }, leading = { Icon24("settings", size = 19.dp, tint = Tokens.text2) })
}

/** Folded on a tablet: the views' and lists' circles, New list, Settings. */
@Composable
private fun ColumnScope.TasksRail(model: TasksModel, s: JSONObject) {
  Column(Modifier.weight(1f).verticalScroll(rememberScrollState()), horizontalAlignment = Alignment.CenterHorizontally) {
    objects(s.getJSONArray("smart").toString()).forEach { sm ->
      RailButton(sm.optString("name"), model.view == sm.optString("id"), { model.go(sm.optString("id")) }) { Circle(tint(sm.optString("color")), 24, sm.optString("icon"), 14) }
    }
    val lists = objects(s.getJSONArray("lists").toString())
    if (lists.isNotEmpty()) Box(Modifier.padding(vertical = 6.dp).width(28.dp).height(1.dp).background(Tokens.line))
    lists.forEach { l ->
      RailButton(l.optString("name"), model.view == l.optString("id"), { model.go(l.optString("id")) }) { Circle(Colors.of(l.optString("color")), 22, "list", 12, 2.4f) }
    }
    RailIcon("plus", "New list") { model.newList() }
  }
  RailIcon("settings", "Settings") { model.settingsOpen = true }
}

@Composable
private fun MainList(model: TasksModel, s: JSONObject) {
  val current = s.getJSONObject("current")
  val list = current.optJSONObject("list")
  val groups = objects(s.getJSONArray("groups").toString())
  BoxWithConstraints(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
    val phone = LocalDrawerMode.current
    val wide = maxWidth >= 720.dp
    LazyColumn(
      Modifier.fillMaxSize().safeDrawingPadding().imePadding(),
      contentPadding = PaddingValues(start = if (wide) 28.dp else 14.dp, end = if (wide) 28.dp else 14.dp, top = if (wide) 26.dp else 12.dp, bottom = 80.dp),
      horizontalAlignment = Alignment.CenterHorizontally,
    ) {
      item(key = "header") {
        Column(Modifier.widthIn(max = 760.dp).fillMaxWidth()) {
          val color = current.optString("color")
          val title = buildAnnotatedString {
            withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = if (color == "gray") MaterialTheme.colorScheme.onSurface else Colors.of(color))) { append(current.optString("name")) }
            withStyle(SpanStyle(fontWeight = FontWeight.Normal, color = Tokens.text3)) { append(" ${s.optInt("count")}") }
          }
          Toolbar(title, compact = !wide, menu = if (phone) ({ model.drawerOpen = true }) else null) {
            if (list != null) {
              val from = list.optJSONObject("from") != null
              IconBtn(if (from) "people" else "edit", { model.listSheet = JSONObject(list.toString()) }, size = 19.dp, description = if (from) "About this list" else "Edit list")
              if (!from) IconBtn("share", { model.share(list.optString("id")) }, size = 19.dp, description = "Share list")
            }
          }
          val note = list?.optString("note").orEmpty()
          if (note.isNotEmpty()) {
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(bottom = 14.dp).offset(y = (-6).dp)) {
              Icon24("people", size = 15.dp, tint = Tokens.text2); Spacer(Modifier.width(6.dp)); Text(note, fontSize = 13.sp, color = Tokens.text2)
            }
          }
          if (current.optBoolean("canAdd")) QuickAdd(model, list?.optString("id"))
        }
      }
      groups.forEach { g ->
        val key = g.optString("key")
        if (key.isNotEmpty()) item(key = "day-$key") {
          Text(key, fontWeight = FontWeight.Bold, fontSize = 15.sp, modifier = Modifier.widthIn(max = 760.dp).fillMaxWidth().padding(start = 4.dp, top = 22.dp, bottom = 6.dp).animateItem())
        }
        val items = objects(g.getJSONArray("items").toString())
        itemsIndexed(items, key = { _, it -> it.optString("container") + "/" + it.optString("id") }) { i, x ->
          Column(Modifier.widthIn(max = 760.dp).fillMaxWidth().animateItem()) {
            TaskRow(model, x)
            if (i < items.size - 1) Box(Modifier.padding(start = 46.dp).fillMaxWidth().height(1.dp).background(Tokens.line))
          }
        }
      }
      s.optJSONObject("empty")?.let { e -> item(key = "empty") { EmptyState("check", e.optString("headline"), Modifier.animateItem()) } }
    }
  }
}

/** `.quick`: the field to add a task, with the date it will get. */
@Composable
private fun QuickAdd(model: TasksModel, list: String?) {
  var draft by remember(model.view) { mutableStateOf("") }
  val preview = if (draft.isBlank()) null else quickPreview(draft, UuobSettings.hour12)
  val src = remember { MutableInteractionSource() }
  val focused by src.collectIsFocusedAsState()
  val focus = remember { FocusRequester() }
  LaunchedEffect(model.quickFocus) { if (model.quickFocus > 0) runCatching { focus.requestFocus() } }
  Row(
    Modifier.fillMaxWidth().padding(bottom = 16.dp)
      .then(if (focused) Modifier.border(4.dp, Tokens.accentSoft, RoundedCornerShape(18.dp)).padding(4.dp) else Modifier)
      .height(54.dp).clip(RoundedCornerShape(14.dp)).background(MaterialTheme.colorScheme.surface)
      .border(1.dp, if (focused) MaterialTheme.colorScheme.primary else Tokens.line, RoundedCornerShape(14.dp)).padding(horizontal = 16.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
  ) {
    Icon24("plus", size = 20.dp, stroke = 2.2f, tint = MaterialTheme.colorScheme.primary)
    BasicTextField(
      draft, { draft = it }, Modifier.weight(1f).focusRequester(focus), singleLine = true, interactionSource = src,
      textStyle = TextStyle(fontSize = 17.sp, color = MaterialTheme.colorScheme.onSurface), cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
      keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
      keyboardActions = KeyboardActions(onDone = { if (draft.isNotBlank()) { model.add(draft, list); draft = "" } }),
      decorationBox = { inner -> Box { if (draft.isEmpty()) Text("New task — try “call mom fri 5pm”", fontSize = 17.sp, color = Tokens.text3, maxLines = 1, overflow = TextOverflow.Ellipsis); inner() } },
    )
    AnimatedVisibility(preview != null, enter = fadeIn(tween(120)), exit = fadeOut(tween(120))) {
      Text(preview ?: "", fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = MaterialTheme.colorScheme.primary, maxLines = 1,
        modifier = Modifier.clip(Pill.shape).background(Tokens.accentSoft).padding(horizontal = 10.dp, vertical = 4.dp))
    }
  }
}

@Composable
private fun TaskRow(model: TasksModel, x: JSONObject) {
  val show = x.getJSONObject("show")
  val done = x.optBoolean("done")
  val canEdit = show.optBoolean("canEdit")
  val fade by animateFloatAsState(if (show.optBoolean("finishing")) 0.35f else 1f, tween(500, delayMillis = 250), label = "finish")
  val ring = if (show.isNull("ring")) null else Colors.of(show.optString("ring"))
  Row(
    Modifier.fillMaxWidth().alpha(fade).clip(RoundedCornerShape(10.dp)).clickable { model.card = x }.padding(horizontal = 6.dp, vertical = 12.dp),
    horizontalArrangement = Arrangement.spacedBy(14.dp),
  ) {
    Check(done, ring ?: MaterialTheme.colorScheme.primary, { model.toggle(x) }, Modifier.padding(top = 1.dp), enabled = canEdit)
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
      Text(x.optString("title"), fontSize = 17.sp, color = if (done) Tokens.text3 else MaterialTheme.colorScheme.onSurface, textDecoration = if (done) TextDecoration.LineThrough else null)
      val notes = x.text("notes").orEmpty()
      if (notes.isNotEmpty()) Text(notes, fontSize = 14.sp, color = Tokens.text2, maxLines = 1, overflow = TextOverflow.Ellipsis)
      val whenText = show.text("when")
      val listName = show.text("listName")
      if (whenText != null || listName != null) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
          if (whenText != null) {
            val late = show.optBoolean("late")
            Text(whenText, fontSize = 14.sp, fontWeight = if (late) FontWeight.Medium else FontWeight.Normal, color = if (late) MaterialTheme.colorScheme.error else Tokens.text2)
          }
          if (listName != null) Text(listName, fontSize = 14.sp, fontWeight = FontWeight.Medium, color = Colors.of(show.text("listColor")))
        }
      }
    }
  }
}

/** The task's card (the kit's detail card): when, its list, notes; Edit, Delete, Complete. */
@Composable
private fun TaskCard(model: TasksModel, x: JSONObject, s: JSONObject) {
  val show = x.getJSONObject("show")
  val canEdit = show.optBoolean("canEdit")
  val close = { model.card = null }
  val list = objects(s.getJSONArray("lists").toString()).firstOrNull { it.optString("id") == x.text("list") }
  val notes = x.text("notes").orEmpty()
  val rows = listOfNotNull(show.text("when")?.let { "when" }, list?.let { "list" }, notes.ifEmpty { null }?.let { "notes" })
  DetailSheet(x.optString("title"), close, actions = { if (canEdit) IconBtn("edit", { model.card = null; model.editing = x }, size = 19.dp, description = "Edit") }) {
    Spacer(Modifier.height(6.dp))
    rows.forEachIndexed { i, r ->
      val last = i == rows.size - 1
      when (r) {
        "when" -> IconRow("clock", last = last) {
          val late = show.optBoolean("late")
          Text(show.optString("when"), fontSize = 15.sp, color = if (late) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface)
        }
        "list" -> IconRow("list", last = last) {
          Box(Modifier.size(10.dp).clip(CircleShape).background(Colors.of(list!!.optString("color"))))
          Text(list!!.optString("name"), fontSize = 15.sp)
        }
        else -> IconRow("notes", last = last, top = true) { Text(notes, fontSize = 15.sp, modifier = Modifier.padding(vertical = 8.dp)) }
      }
    }
    if (canEdit) {
      Row(Modifier.fillMaxWidth().padding(top = 18.dp), verticalAlignment = Alignment.CenterVertically) {
        Btn("Delete", { model.remove(x) }, kind = BtnKind.Danger)
        Spacer(Modifier.weight(1f))
        Btn(if (x.optBoolean("done")) "Not done" else "Complete", { model.card = null; model.toggle(x) }, kind = BtnKind.Primary)
      }
    }
  }
}

/** The task's editor (the kit's editor): the title, its list under it, notes, date, time. Nothing changes until Save. */
@Composable
private fun TaskEditor(model: TasksModel, x: JSONObject, s: JSONObject) {
  var title by remember(x) { mutableStateOf(x.optString("title")) }
  var notes by remember(x) { mutableStateOf(x.text("notes").orEmpty()) }
  var due by remember(x) { mutableStateOf(x.text("due")) }
  var time by remember(x) { mutableStateOf(x.text("time")) }
  var list by remember(x) { mutableStateOf(x.text("list") ?: "") }
  val lists = objects(s.getJSONArray("lists").toString()).filter { it.optBoolean("canWrite") }
  val close = { model.editing = null }
  EditSheet(
    title, { title = it }, placeholder = "Task", canSave = title.isNotBlank(), onCancel = close,
    onSave = {
      val y = JSONObject(x.toString()).put("title", title.trim()).put("notes", notes).put("due", due ?: JSONObject.NULL).put("time", time ?: JSONObject.NULL)
      model.editing = null
      if (list != (x.text("list") ?: "")) model.move(y, list.ifEmpty { null }) else model.save(y)
    },
    onDelete = { model.remove(x) },
    under = {
      Icon24("list", size = 16.dp, tint = Tokens.text2)
      Select(listOf("" to "No list") + lists.map { it.optString("id") to it.optString("name") }, list, plain = true) { list = it }
    },
  ) {
    IconRow("notes", top = true) { WebField(notes, { notes = it }, Modifier.weight(1f), placeholder = "Notes", singleLine = false, minHeight = 46.dp, border = false, textStyle = TextStyle(fontSize = 15.sp, color = MaterialTheme.colorScheme.onSurface)) }
    IconRow("calendar") { DateField(due, plain = true) { due = it } }
    IconRow("clock", last = true) { TimeField(time, { time = it }, Modifier.width(110.dp), placeholder = "No time", optional = true, plain = true, atStart = true); Spacer(Modifier.weight(1f)) }
  }
}

/** A `<select>`: the choice shown in a field (or plain text), a menu to change it. */
@Composable
private fun Select(options: List<Pair<String, String>>, current: String, plain: Boolean = false, onPick: (String) -> Unit) {
  var menu by remember { mutableStateOf(false) }
  Box {
    Row(
      (if (plain) Modifier.clip(RoundedCornerShape(8.dp)).clickable { menu = true }.padding(horizontal = 4.dp, vertical = 6.dp)
      else Modifier.fillMaxWidth().height(46.dp).clip(Tokens.field).background(Tokens.surface2).border(1.dp, Tokens.line, Tokens.field).clickable { menu = true }.padding(horizontal = 14.dp)),
      verticalAlignment = Alignment.CenterVertically,
      horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
      Text(options.firstOrNull { it.first == current }?.second ?: "", if (plain) Modifier else Modifier.weight(1f), fontSize = 15.sp, fontWeight = if (plain) FontWeight.SemiBold else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
      Icon24("chevron", Modifier.rotate(90f), size = 14.dp, tint = Tokens.text2)
    }
    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
      options.forEach { (v, l) -> DropdownMenuItem(text = { Text(l) }, onClick = { menu = false; onPick(v) }) }
    }
  }
}

/** A date, "YYYY-MM-DD" or none, picked from a calendar. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DateField(value: String?, plain: Boolean = false, onChange: (String?) -> Unit) {
  var picking by remember { mutableStateOf(false) }
  Box(
    (if (plain) Modifier.fillMaxWidth().clickable { picking = true }.padding(vertical = 8.dp)
    else Modifier.fillMaxWidth().height(46.dp).clip(Tokens.field).background(Tokens.surface2).border(1.dp, Tokens.line, Tokens.field).clickable { picking = true }.padding(horizontal = 14.dp)),
    contentAlignment = Alignment.CenterStart,
  ) { Text(value?.let { app.uuob.client.friendlyDate(it) } ?: "No date", fontSize = 15.sp, color = if (value == null) Tokens.text3 else MaterialTheme.colorScheme.onSurface, maxLines = 1) }
  if (picking) {
    val millis = value?.let { LocalDate.parse(it).atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli() }
    val state = rememberDatePickerState(initialSelectedDateMillis = millis)
    DatePickerDialog(
      onDismissRequest = { picking = false },
      confirmButton = { TextButton(onClick = { picking = false; state.selectedDateMillis?.let { onChange(Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate().toString()) } }) { Text("OK") } },
      dismissButton = { TextButton(onClick = { picking = false; onChange(null) }) { Text("No date") } },
    ) { DatePicker(state) }
  }
}

/** Add or edit a list (the kit's editor): its name, colour and sharing; or, for a list someone shared, a card with Leave list. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ListSheet(model: TasksModel, start: JSONObject) {
  var name by remember(start) { mutableStateOf(start.optString("name")) }
  var color by remember(start) { mutableStateOf(start.optString("color", "blue")) }
  val id = start.text("id")
  val from = start.optJSONObject("from")
  val close = { model.listSheet = null }
  if (from != null) {
    DetailSheet(name, close) {
      Spacer(Modifier.height(6.dp))
      IconRow("people", last = true) {
        Text("${from.optString("name")} shares this list with you. " + if (start.optBoolean("canWrite")) "You can both change it." else "You can view it, but not change it.", fontSize = 15.sp)
      }
      Btn("Leave list", { model.deleteList(id!!) }, Modifier.padding(top = 18.dp), kind = BtnKind.Danger)
    }
    return
  }
  // The sheet takes the colour being chosen, so Save shows what the list will look like.
  Accent(Colors.of(color)) {
    EditSheet(
      name, { name = it }, placeholder = "List name", isNew = id == null, canSave = name.isNotBlank(), onCancel = close,
      onSave = { model.saveList(JSONObject(start.toString()).put("name", name).put("color", color)) },
      onDelete = id?.let { { model.deleteList(it) } },
    ) {
      IconRow("list", last = id == null) {
        FlowRow(Modifier.weight(1f).padding(vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
          COLORS.forEach { c ->
            val on = c == color
            Box(
              Modifier.size(28.dp)
                .then(if (on) Modifier.border(2.dp, Tokens.text2, CircleShape).padding(4.dp) else Modifier)
                .clip(CircleShape).background(Colors.of(c)).clickable { color = c },
            )
          }
        }
      }
      if (id != null) {
        val shared = (start.optJSONArray("sharedWith")?.length() ?: 0) > 0
        IconRow("share", Modifier.clickable { model.share(id) }, last = true) {
          Text(if (shared) start.optString("note") else "Share list…", fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
      }
    }
  }
}
