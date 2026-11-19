package `in`.rinne.your_szu_life

import android.os.Bundle

class MainActivity : ApplicationBackActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    AppUpdateBridge.cleanTemporaryApks(this)
    CampusNetworkBridge.bindActivity(this)
    PermissionBridge.bindActivity(this)
  }

  override fun onStart() {
    super.onStart()
    CampusNetworkBridge.bindActivity(this)
    PermissionBridge.bindActivity(this)
  }

  override fun onResume() {
    super.onResume()
    CampusNetworkBridge.bindActivity(this)
    PermissionBridge.bindActivity(this)
  }

  override fun onDestroy() {
    CampusNetworkBridge.unbindActivity(this)
    PermissionBridge.unbindActivity(this)
    super.onDestroy()
  }
}
