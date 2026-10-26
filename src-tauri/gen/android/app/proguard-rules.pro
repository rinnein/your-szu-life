# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# These methods are called from Rust through the Android bridge and must keep
# their names in minified release builds.
-keep class in.rinne.your_szu_life.CampusNetworkBridge {
    public *;
}
-keep class in.rinne.your_szu_life.WallpaperColorBridge { public *; }
-keep class in.rinne.your_szu_life.AppUpdateBridge { public *; }
-keep class in.rinne.your_szu_life.PackageReplacedReceiver { public *; }
-keep class in.rinne.your_szu_life.AuthCookieBridge { public *; }
-keep class in.rinne.your_szu_life.UoocTaskBridge { public *; }
-keep class in.rinne.your_szu_life.UoocTaskService { public *; }
-keep class in.rinne.your_szu_life.MainActivity {
    public <init>(...);
    public *;
}

# wry invokes these WebView bridge methods from Rust through JNI by name.
# R8 cannot infer those calls from the Kotlin call graph.
-keep class in.rinne.your_szu_life.RustWebView {
    public *;
}
-keep class in.rinne.your_szu_life.RustWebViewClient {
    public *;
}
-keep class in.rinne.your_szu_life.RustWebChromeClient {
    public *;
}

# The Android TLS verifier is called from Rust through JNI, so R8 cannot infer
# these references from the Kotlin call graph.
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }

# Tauri resolves activityName through JNI/Class.forName; keep the shared hooks.
-keep class in.rinne.your_szu_life.ApplicationBackActivity { public *; protected *; }
-keep class in.rinne.your_szu_life.FeatureActivity { public *; }

# Enrollment callbacks are resolved by name from Rust JNI.
-keep class in.rinne.your_szu_life.EnrollmentTaskBridge { public *; }
-keep class in.rinne.your_szu_life.EnrollmentTaskService { public *; }
