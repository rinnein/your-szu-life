package io.crates.keyring

import android.content.Context

/** Initializes ndk-context for android-native-keyring-store before Rust accesses credentials. */
class Keyring {
  companion object {
    init {
      System.loadLibrary("your_szu_life_lib")
    }

    external fun initializeNdkContext(context: Context)
  }
}
