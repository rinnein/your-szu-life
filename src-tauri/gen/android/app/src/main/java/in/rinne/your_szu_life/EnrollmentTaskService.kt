package `in`.rinne.your_szu_life

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat

object EnrollmentTaskBridge {
  @Volatile internal var count = 0
  @Volatile internal var generation = 0L
  @JvmStatic external fun nativeControl(action: Int, generation: Long)

  @JvmStatic fun update(context: Context, activeCourses: Int, runGeneration: Long): Boolean = try {
    val app = context.applicationContext
    count = activeCourses
    generation = runGeneration
    val intent = Intent(app, EnrollmentTaskService::class.java).putExtra("courses", activeCourses).putExtra("generation", runGeneration)
    if (activeCourses == 0) app.stopService(intent)
    else if (Build.VERSION.SDK_INT >= 26) app.startForegroundService(intent)
    else app.startService(intent)
    true
  } catch (_: Exception) {
    count = 0
    false
  }

  fun resumed() { runCatching { nativeControl(2, 0) } }
}

/** A user-visible data-sync service. Android 15+ timeouts pause Rust work as well. */
class EnrollmentTaskService : Service() {
  private var runGeneration = 0L
  private var wakeLock: PowerManager.WakeLock? = null

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val requestGeneration = intent?.getLongExtra("generation", 0) ?: 0L
    if (requestGeneration != EnrollmentTaskBridge.generation) return START_NOT_STICKY
    runGeneration = requestGeneration
    if (intent?.action == "stop") {
      EnrollmentTaskBridge.count = 0
      EnrollmentTaskBridge.nativeControl(0, runGeneration)
      stopSelf()
      return START_NOT_STICKY
    }
    // An update queued before a pause must not resurrect the foreground service.
    if (EnrollmentTaskBridge.count <= 0) { stopSelf(); return START_NOT_STICKY }
    try {
      val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
      if (Build.VERSION.SDK_INT >= 26) {
        manager.createNotificationChannel(NotificationChannel("enrollment-tasks", "选课 学习任务", NotificationManager.IMPORTANCE_LOW))
      }
      val open = PendingIntent.getActivity(this, 4201,
        Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
      val stop = PendingIntent.getService(this, 4202,
        Intent(this, EnrollmentTaskService::class.java).setAction("stop").putExtra("generation", runGeneration),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
      val notification = NotificationCompat.Builder(this, "enrollment-tasks")
        .setSmallIcon(R.mipmap.ic_launcher).setContentTitle("选课 后台任务运行中")
        .setContentText("${EnrollmentTaskBridge.count} 门课程正在处理，点击打开泥深")
        .setContentIntent(open).setOngoing(true).setOnlyAlertOnce(true)
        .addAction(0, "停止全部", stop).build()
      if (Build.VERSION.SDK_INT >= 29) startForeground(4201, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
      else startForeground(4201, notification)
      if (wakeLock?.isHeld != true) {
        wakeLock = (getSystemService(Context.POWER_SERVICE) as PowerManager)
          .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "$packageName:enrollment").apply {
            setReferenceCounted(false)
            acquire(6 * 60 * 60 * 1000L)
          }
      }
    } catch (_: Exception) {
      if (runGeneration == EnrollmentTaskBridge.generation) {
        EnrollmentTaskBridge.count = 0
        EnrollmentTaskBridge.nativeControl(1, runGeneration)
      }
      stopSelf()
    }
    // Recovery requires the app's cookie/identity checks, never a service-only restart.
    return START_NOT_STICKY
  }

  override fun onTimeout(startId: Int, fgsType: Int) {
    if (runGeneration == EnrollmentTaskBridge.generation) {
      EnrollmentTaskBridge.count = 0
      EnrollmentTaskBridge.nativeControl(1, runGeneration)
    }
    stopSelf()
  }

  override fun onDestroy() {
    wakeLock?.let { if (it.isHeld) it.release() }
    wakeLock = null
    if (EnrollmentTaskBridge.count > 0 && runGeneration == EnrollmentTaskBridge.generation) {
      EnrollmentTaskBridge.count = 0
      EnrollmentTaskBridge.nativeControl(1, runGeneration)
    }
    super.onDestroy()
  }
}
