package `in`.rinne.your_szu_life

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class PackageReplacedReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent?) {
    if (intent?.action == Intent.ACTION_MY_PACKAGE_REPLACED) {
      AppUpdateBridge.cleanTemporaryApks(context)
      // 应用更新后进程被重启，闹钟仍由系统保留；但课次规格需要按新安装的包重新排一遍，
      // 避免更新期间跨过窗口起点时实时活动没人启动。
      TimetableNotificationBridge.restore(context)
    }
  }
}
