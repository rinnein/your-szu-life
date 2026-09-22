import java.io.FileInputStream
import java.util.Properties

private fun rustlsPlatformVerifierMavenDir(): File {
    val manifestPath = project.rootDir.resolve("../../Cargo.toml")
    val metadata = providers.exec {
        workingDir(project.rootDir)
        commandLine(
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            "aarch64-linux-android",
            "--manifest-path",
            manifestPath.absolutePath,
        )
    }.standardOutput.asText.get()
    val path = Regex(
        "\"manifest_path\"\\s*:\\s*\"([^\"]*rustls-platform-verifier-android[^\"]*)\"",
    )
        .find(metadata)
        ?.groupValues
        ?.getOrNull(1)
        ?: error("rustls-platform-verifier-android package was not found in cargo metadata")
    return File(path).parentFile.resolve("maven")
}

repositories {
    maven {
        url = uri(rustlsPlatformVerifierMavenDir())
        metadataSources { mavenPom(); artifact() }
    }
}

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

val releaseSigningPropertiesFile = rootProject.file("keystore.properties")
val releaseSigningProperties = Properties().apply {
    if (releaseSigningPropertiesFile.exists()) {
        FileInputStream(releaseSigningPropertiesFile).use { load(it) }
    }
}

fun releaseSigningProperty(name: String): String =
    releaseSigningProperties.getProperty(name)
        ?: error("Missing $name in ${releaseSigningPropertiesFile.path}")

android {
    compileSdk = 36
    namespace = "in.rinne.your_szu_life"
    signingConfigs {
        if (releaseSigningPropertiesFile.exists()) {
            create("release") {
                keyAlias = releaseSigningProperty("keyAlias")
                keyPassword = releaseSigningProperty("keyPassword")
                storeFile = rootProject.file(releaseSigningProperty("storeFile"))
                storePassword = releaseSigningProperty("storePassword")
            }
        }
    }
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "in.rinne.your_szu_life"
        minSdk = 24
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "false"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            isMinifyEnabled = true
            if (releaseSigningPropertiesFile.exists()) {
                signingConfig = signingConfigs.getByName("release")
            }
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    implementation("rustls:rustls-platform-verifier:0.1.1")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")
