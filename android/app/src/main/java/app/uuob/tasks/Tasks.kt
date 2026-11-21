package app.uuob.tasks

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
  var mobileList by mutableStateOf(false)
  var finishing by mutableStateOf(setOf<String>())
  var openId by mutableStateOf<String?>(null)
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
    openId = null
    mobileList = true
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
    openId = null
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
          mobileList = true
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
  // The list or view on screen lends its colour to everything in it: pluses, ticks, the field's edge.
  val shown = s.optJSONObject("current")?.optString("color")
  val accent = if (shown.isNullOrEmpty() || shown == "gray") MaterialTheme.colorScheme.primary else Colors.of(shown)
  BoxWithConstraints(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
    val wide = maxWidth >= 720.dp
    if (wide) {
      Row(Modifier.fillMaxSize()) {
        Box(Modifier.width(290.dp).fillMaxHeight().background(MaterialTheme.colorScheme.surface)) { Sidebar(model, s, wide, updater) }
        Box(Modifier.width(1.dp).fillMaxHeight().background(Tokens.line))
        Accent(accent) { MainList(model, s, wide) }
      }
    } else {
      BackHandler(model.mobileList) { model.mobileList = false }
      if (model.mobileList) Accent(accent) { MainList(model, s, wide) } else Sidebar(model, s, wide, updater)
    }
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

@Composable
private fun Sidebar(model: TasksModel, s: JSONObject, wide: Boolean, updater: Updater) {
  Column(Modifier.fillMaxSize().safeDrawingPadding().padding(horizontal = 16.dp, vertical = if (wide) 28.dp else 24.dp)) {
   Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
    Row(verticalAlignment = Alignment.CenterVertically) {
      if (!wide) LargeTitle("Tasks", Modifier.weight(1f)) else Spacer(Modifier.weight(1f))
      UpdateButton(updater)
    }
    if (!wide) Spacer(Modifier.height(18.dp))
    // The smart views, two by two.
    objects(s.getJSONArray("smart").toString()).chunked(2).forEach { pair ->
      Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.padding(bottom = 10.dp)) {
        pair.forEach { sm ->
          val on = wide && model.view == sm.optString("id")
          val src = remember { MutableInteractionSource() }
          val pressed by src.collectIsPressedAsState()
          val k by animateFloatAsState(if (pressed) 0.97f else 1f, label = "tile")
          Column(
            Modifier.weight(1f).scale(k).clip(RoundedCornerShape(14.dp))
              .background(if (on) tint(sm.optString("color")).copy(alpha = if (Colors.dark) 0.16f else 0.12f) else MaterialTheme.colorScheme.surface)
              .border(1.dp, if (on) tint(sm.optString("color")).copy(alpha = 0.5f) else Tokens.line, RoundedCornerShape(14.dp))
              .clickable(src, null) { model.go(sm.optString("id")) }.padding(12.dp),
          ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
              Circle(tint(sm.optString("color")), 30, sm.optString("icon"), 17)
              Spacer(Modifier.weight(1f))
              Text("${sm.optInt("count")}", fontSize = 24.sp, fontWeight = FontWeight.Bold)
            }
            Spacer(Modifier.height(8.dp))
            Text(sm.optString("name"), fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = Tokens.text2)
          }
        }
      }
    }
    SectionLabel("My lists", Modifier.padding(start = 8.dp, top = 16.dp, bottom = 8.dp))
    val lists = objects(s.getJSONArray("lists").toString())
    lists.forEach { l ->
      val on = wide && model.view == l.optString("id")
      Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp))
          .background(if (on) Colors.of(l.optString("color")).copy(alpha = if (Colors.dark) 0.16f else 0.12f) else Color.Transparent)
          .clickable { model.go(l.optString("id")) }.padding(8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
      ) {
        Circle(Colors.of(l.optString("color")), 26, "list", 14, 2.4f)
        Text(l.optString("name"), fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (l.optBoolean("shared")) Icon24("people", size = 15.dp, tint = Tokens.text3)
        Text("${l.optInt("count")}", fontSize = 13.sp, color = Tokens.text2)
      }
    }
    Btn("Add list", { model.newList() }, Modifier.padding(top = 8.dp), kind = BtnKind.Ghost, icon = "plus")
   }
    Row(Modifier.clip(Pill.shape).clickable { model.settingsOpen = true }.padding(horizontal = 10.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
      Icon24("settings", size = 19.dp, tint = Tokens.text2); Spacer(Modifier.width(8.dp)); Text("Settings", fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = Tokens.text2)
    }
  }
}

@Composable
private fun MainList(model: TasksModel, s: JSONObject, wide: Boolean) {
  val current = s.getJSONObject("current")
  val list = current.optJSONObject("list")
  val groups = objects(s.getJSONArray("groups").toString())
  LazyColumn(
    Modifier.fillMaxSize().safeDrawingPadding().imePadding(),
    contentPadding = PaddingValues(start = if (wide) 48.dp else 16.dp, end = if (wide) 48.dp else 16.dp, top = if (wide) 40.dp else 16.dp, bottom = 80.dp),
    horizontalAlignment = Alignment.CenterHorizontally,
  ) {
    item(key = "header") {
      Column(Modifier.widthIn(max = 760.dp).fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(bottom = 18.dp)) {
          if (!wide) IconBtn("back", { model.mobileList = false }, Modifier.offset(x = (-8).dp))
          val color = current.optString("color")
          Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
              current.optString("name"), fontSize = if (wide) 34.sp else 30.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.8).sp,
              color = if (color == "gray") MaterialTheme.colorScheme.onSurface else Colors.of(color),
              maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
            )
            if (list != null) {
              val from = list.optJSONObject("from") != null
              IconBtn(if (from) "people" else "edit", { model.listSheet = JSONObject(list.toString()) }, size = 18.dp)
              if (!from) IconBtn("share", { model.share(list.optString("id")) }, size = 18.dp)
            }
          }
          Text("${s.optInt("count")}", fontSize = 34.sp, fontWeight = FontWeight.Bold, color = Tokens.text3)
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
      items(objects(g.getJSONArray("items").toString()), key = { it.optString("container") + "/" + it.optString("id") }) { x ->
        Box(Modifier.widthIn(max = 760.dp).fillMaxWidth().animateItem()) { TaskRow(model, x, s) }
      }
    }
    s.optJSONObject("empty")?.let { e -> item(key = "empty") { Empty(e.optString("glyph"), e.optString("headline"), "", Modifier.animateItem()) } }
  }
}

/** `.quick`: the field to add a task, with the date it will get. */
@Composable
private fun QuickAdd(model: TasksModel, list: String?) {
  var draft by remember(model.view) { mutableStateOf("") }
  val preview = if (draft.isBlank()) null else quickPreview(draft, UuobSettings.hour12)
  val src = remember { MutableInteractionSource() }
  val focused by src.collectIsFocusedAsState()
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
      draft, { draft = it }, Modifier.weight(1f), singleLine = true, interactionSource = src,
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
private fun TaskRow(model: TasksModel, x: JSONObject, s: JSONObject) {
  val show = x.getJSONObject("show")
  val id = x.optString("id")
  val open = model.openId == id
  val done = x.optBoolean("done")
  val canEdit = show.optBoolean("canEdit")
  val fade by animateFloatAsState(if (show.optBoolean("finishing")) 0.35f else 1f, tween(500, delayMillis = 250), label = "finish")
  val ring = if (show.isNull("ring")) null else Colors.of(show.optString("ring"))
  val shape = RoundedCornerShape(14.dp)
  Column(
    Modifier.fillMaxWidth().alpha(fade).padding(vertical = if (open) 8.dp else 0.dp)
      .then(if (open) Modifier.shadow(10.dp, shape, ambientColor = Color.Black.copy(alpha = 0.1f), spotColor = Color.Black.copy(alpha = 0.1f)) else Modifier)
      .clip(shape).background(if (open) MaterialTheme.colorScheme.surface else Color.Transparent),
  ) {
    Row(Modifier.fillMaxWidth().padding(12.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
      Check(done, ring ?: MaterialTheme.colorScheme.primary, { model.toggle(x) }, Modifier.padding(top = 1.dp), enabled = canEdit)
      Column(Modifier.weight(1f).clickable(enabled = canEdit, indication = null, interactionSource = remember { MutableInteractionSource() }) { model.openId = if (open) null else id }, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(x.optString("title"), fontSize = 17.sp, color = if (done) Tokens.text3 else MaterialTheme.colorScheme.onSurface, textDecoration = if (done) TextDecoration.LineThrough else null)
        val notes = x.text("notes").orEmpty()
        if (notes.isNotEmpty() && !open) Text(notes, fontSize = 14.sp, color = Tokens.text2, maxLines = 1, overflow = TextOverflow.Ellipsis)
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
    AnimatedVisibility(open, enter = expandVertically(tween(200)) + fadeIn(), exit = shrinkVertically(tween(200)) + fadeOut()) { Editor(model, x, s) }
  }
}

@Composable
private fun Editor(model: TasksModel, x: JSONObject, s: JSONObject) {
  fun with(k: String, v: Any?) = JSONObject(x.toString()).put(k, v ?: JSONObject.NULL)
  Column(Modifier.padding(start = 14.dp, end = 14.dp, bottom = 14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
    CommitField(x.optString("title"), "", single = true) { if (it != x.optString("title")) model.save(with("title", it)) }
    CommitField(x.text("notes").orEmpty(), "Notes", single = false) { if (it != x.text("notes").orEmpty()) model.save(with("notes", it)) }
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
      Stack("Date", Modifier.weight(1f)) { DateField(x.text("due")) { model.save(with("due", it)) } }
      Stack("Time", Modifier.weight(1f)) { TimeField(x.text("time"), { model.save(with("time", it)) }, Modifier.fillMaxWidth(), placeholder = "None", optional = true) }
    }
    val lists = objects(s.getJSONArray("lists").toString()).filter { it.optBoolean("canWrite") }
    Stack("List") {
      Select(listOf("" to "None") + lists.map { it.optString("id") to it.optString("name") }, x.text("list") ?: "") { v -> model.move(x, v.ifEmpty { null }) }
    }
    Row(Modifier.fillMaxWidth().padding(top = 4.dp)) {
      Btn("Delete", { model.remove(x) }, kind = BtnKind.Danger, icon = "trash")
      Spacer(Modifier.weight(1f))
      Btn("Done", { model.openId = null })
    }
  }
}

/** A `<select>`: the choice shown in a field, a menu to change it. */
@Composable
private fun Select(options: List<Pair<String, String>>, current: String, onPick: (String) -> Unit) {
  var menu by remember { mutableStateOf(false) }
  Box {
    Row(
      Modifier.fillMaxWidth().height(46.dp).clip(Tokens.field).background(Tokens.surface2).border(1.dp, Tokens.line, Tokens.field).clickable { menu = true }.padding(horizontal = 14.dp),
      verticalAlignment = Alignment.CenterVertically,
    ) {
      Text(options.firstOrNull { it.first == current }?.second ?: "", Modifier.weight(1f), fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
      Icon24("chevron", Modifier.rotate(90f), size = 16.dp, tint = Tokens.text2)
    }
    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
      options.forEach { (v, l) -> DropdownMenuItem(text = { Text(l) }, onClick = { menu = false; onPick(v) }) }
    }
  }
}

/** A text field that saves when it is left (as `onchange` did). */
@Composable
private fun CommitField(value: String, placeholder: String, single: Boolean, onCommit: (String) -> Unit) {
  var text by remember(value) { mutableStateOf(value) }
  var focused by remember { mutableStateOf(false) }
  // Closing the editor (Done, another task) keeps what was typed.
  val latest by rememberUpdatedState(text)
  val saved by rememberUpdatedState(value)
  val commit by rememberUpdatedState(onCommit)
  DisposableEffect(Unit) { onDispose { if (latest != saved) commit(latest) } }
  WebField(
    text, { text = it },
    Modifier.onFocusChanged { if (focused && !it.isFocused) onCommit(text); focused = it.isFocused },
    placeholder = placeholder, singleLine = single, minHeight = if (single) 46.dp else 90.dp,
  )
}

/** A date, "YYYY-MM-DD" or none, picked from a calendar. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DateField(value: String?, onChange: (String?) -> Unit) {
  var picking by remember { mutableStateOf(false) }
  Box(
    Modifier.fillMaxWidth().height(46.dp).clip(Tokens.field).background(Tokens.surface2).border(1.dp, Tokens.line, Tokens.field).clickable { picking = true }.padding(horizontal = 14.dp),
    contentAlignment = Alignment.CenterStart,
  ) { Text(value?.let { app.uuob.client.friendlyDate(it) } ?: "None", fontSize = 15.sp, color = if (value == null) Tokens.text3 else MaterialTheme.colorScheme.onSurface, maxLines = 1) }
  if (picking) {
    val millis = value?.let { LocalDate.parse(it).atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli() }
    val state = rememberDatePickerState(initialSelectedDateMillis = millis)
    DatePickerDialog(
      onDismissRequest = { picking = false },
      confirmButton = { TextButton(onClick = { picking = false; state.selectedDateMillis?.let { onChange(Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate().toString()) } }) { Text("OK") } },
      dismissButton = { TextButton(onClick = { picking = false; onChange(null) }) { Text("None") } },
    ) { DatePicker(state) }
  }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ListSheet(model: TasksModel, start: JSONObject) {
  var name by remember(start) { mutableStateOf(start.optString("name")) }
  var color by remember(start) { mutableStateOf(start.optString("color", "blue")) }
  val id = start.text("id")
  val from = start.optJSONObject("from")
  // The sheet takes the colour being chosen, so the buttons show what the list will look like.
  Accent(Colors.of(color)) { WebSheet({ model.listSheet = null }) {
    Box(Modifier.fillMaxWidth().padding(bottom = 18.dp), contentAlignment = Alignment.Center) {
      Box(Modifier.shadow(8.dp, CircleShape)) { Circle(Colors.of(color), 72, "list", 30, 2.4f) }
    }
    if (from != null) {
      Text(name, fontSize = 22.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Center)
      Text(
        "Shared by ${from.optString("name")}" + if (start.optBoolean("canWrite")) "" else " · view only",
        color = Tokens.text2, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
      )
      Row(Modifier.fillMaxWidth().padding(top = 22.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Btn("Leave list", { model.deleteList(id!!) }, Modifier.weight(1f), kind = BtnKind.Danger, lg = true)
        Btn("Done", { model.listSheet = null }, Modifier.weight(1f), kind = BtnKind.Primary, lg = true)
      }
      return@WebSheet
    }
    WebField(name, { name = it }, placeholder = "List name", minHeight = 54.dp, textStyle = TextStyle(fontSize = 20.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center))
    FlowRow(Modifier.fillMaxWidth().padding(top = 18.dp, bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterHorizontally), verticalArrangement = Arrangement.spacedBy(12.dp)) {
      COLORS.forEach { c ->
        val on = c == color
        Box(
          Modifier.size(32.dp)
            .then(if (on) Modifier.border(2.dp, Tokens.text2, CircleShape).padding(5.dp) else Modifier)
            .clip(CircleShape).background(Colors.of(c)).clickable { color = c },
        )
      }
    }
    if (id != null) {
      val shared = (start.optJSONArray("sharedWith")?.length() ?: 0) > 0
      Row(
        Modifier.fillMaxWidth().padding(top = 18.dp).height(46.dp).clip(Pill.shape).background(Tokens.surface2).clickable { model.share(id) }.padding(horizontal = 18.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
      ) {
        Icon24("share", size = 18.dp, tint = MaterialTheme.colorScheme.primary)
        Text(if (shared) start.optString("note") else "Share list…", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
      }
    }
    Row(Modifier.fillMaxWidth().padding(top = 22.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
      if (id != null) Btn("Delete", { model.deleteList(id) }, Modifier.weight(1f), kind = BtnKind.Danger, lg = true)
      else Btn("Cancel", { model.listSheet = null }, Modifier.weight(1f), lg = true)
      Btn(if (id != null) "Save" else "Create", { model.saveList(JSONObject(start.toString()).put("name", name).put("color", color)) }, Modifier.weight(1f), kind = BtnKind.Primary, lg = true, enabled = name.isNotBlank())
    }
  }}
}
