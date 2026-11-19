package app.uuob.tasks

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat

/**
 * Shows an alert as Tasks's own notification. UUOB keeps the time (it runs
 * all along and learns of changes made on other devices) and, when an alert
 * is due, hands it here: extras `key` (tells alerts apart), `title` and
 * `text` ("9:30–10:00 · Room 2"). Anyone can send a broadcast here, so it
 * counts only when it comes from UUOB (`fromUuob`).
 */
class AlertReceiver : BroadcastReceiver() {
  override fun onReceive(ctx: Context, intent: Intent) {
    if (intent.action != "app.uuob.SHOW_ALERT" || !fromUuob(intent)) return
    val key = intent.getStringExtra("key") ?: return
    val nm = ctx.getSystemService(NotificationManager::class.java)
    nm.createNotificationChannel(NotificationChannel("alerts", "Alerts", NotificationManager.IMPORTANCE_HIGH))
    val open = Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    val tap = PendingIntent.getActivity(ctx, key.hashCode(), open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    val n = NotificationCompat.Builder(ctx, "alerts")
      .setSmallIcon(R.drawable.ic_notify)
      .setContentTitle(intent.getStringExtra("title") ?: "")
      .apply { intent.getStringExtra("text")?.takeIf { it.isNotEmpty() }?.let { setContentText(it) } }
      .setCategory(NotificationCompat.CATEGORY_REMINDER)
      .setPriority(NotificationCompat.PRIORITY_HIGH)
      .setContentIntent(tap)
      .setAutoCancel(true)
      .build()
    nm.notify(key.hashCode(), n)
  }
}


/** Did UUOB send this? Its `from` extra is a PendingIntent, and Android itself
 *  says which app created that one. Only apps the user allowed in UUOB are ever
 *  given one, so every other app is kept out (an allowed app could pass one on). */
@Suppress("DEPRECATION")
fun fromUuob(intent: Intent) = intent.getParcelableExtra<PendingIntent>("from")?.creatorPackage == "app.uuob"
