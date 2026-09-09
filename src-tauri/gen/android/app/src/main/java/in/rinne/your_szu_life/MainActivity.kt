package `in`.rinne.your_szu_life

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import io.crates.keyring.Keyring

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    Keyring.initializeNdkContext(applicationContext)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    CampusNetworkBridge.bindActivity(this)
  }

  override fun onStart() {
    super.onStart()
    CampusNetworkBridge.bindActivity(this)
  }

  override fun onResume() {
    super.onResume()
    CampusNetworkBridge.bindActivity(this)
  }

  override fun onDestroy() {
    CampusNetworkBridge.unbindActivity(this)
    super.onDestroy()
  }
}
