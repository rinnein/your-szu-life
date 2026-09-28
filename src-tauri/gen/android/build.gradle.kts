buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:8.11.0")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:1.9.25")
    }
}

allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

tasks.register("clean").configure {
    delete("build")
}


// Tauri 2.12's AppPlugin installs an always-enabled fallback even when
// WryActivity.handleBackNavigation is false. Give this app ownership without
// modifying Cargo's registry or relying on private AndroidX reflection.
project(":tauri-android") {
    val upstream = layout.projectDirectory.file("src/main/java/app/tauri/AppPlugin.kt")
    val generated = layout.buildDirectory.dir("generated/szu-back-navigation")
    val patchAppBack = tasks.register("prepareSzuAppBackNavigation") {
        inputs.file(upstream)
        outputs.dir(generated)
        doLast {
            val source = upstream.asFile.readText().replace("\r\n", "\n")
            val hash = java.security.MessageDigest.getInstance("SHA-256")
                .digest(source.toByteArray()).joinToString("") { "%02x".format(it) }
            check(hash == "087ae5fb739031d886365aeed1c27bde043c5f7d00f892050662bc602e7451b4") {
                "Tauri AppPlugin changed: review the predictive-back compatibility patch before building."
            }
            val patched = source.replace(
                "object : OnBackPressedCallback(true)",
                "object : OnBackPressedCallback(activity !is AppBackNavigationOwner)",
            ) + "\n/** Activity supplies its own state-aware back navigation. */\ninterface AppBackNavigationOwner\n"
            val output = generated.get().file("app/tauri/SzuAppPlugin.kt").asFile
            output.parentFile.mkdirs()
            output.writeText(patched)
        }
    }
    pluginManager.withPlugin("org.jetbrains.kotlin.android") {
        extensions.configure<org.jetbrains.kotlin.gradle.dsl.KotlinAndroidProjectExtension> {
            sourceSets.getByName("main").kotlin {
                srcDir(generated)
            }
        }
        tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
            exclude { it.file == upstream.asFile }
            dependsOn(patchAppBack)
        }
    }
}
