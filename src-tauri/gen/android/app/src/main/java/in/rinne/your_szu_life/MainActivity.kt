package `in`.rinne.your_szu_life

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import io.crates.keyring.Keyring

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // android-native-keyring-store needs this context before any Rust store access.
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
