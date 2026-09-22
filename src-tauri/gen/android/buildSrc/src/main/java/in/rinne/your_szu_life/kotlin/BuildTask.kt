import java.io.File
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction

abstract class BuildTask : DefaultTask() {
    @Input
    var rootDirRel: String? = null
    @Input
    var target: String? = null
    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
        val root = File(project.projectDir, rootDirRel ?: throw GradleException("rootDirRel cannot be null"))
        val targetName = target ?: throw GradleException("target cannot be null")
        val isRelease = release ?: throw GradleException("release cannot be null")
        val (rustTarget, abi) = targetSpec(targetName)
        val profile = if (isRelease) "release" else "debug"
        val source = File(root, "target/$rustTarget/$profile/libyour_szu_life_lib.so")
        if (!source.isFile) {
            throw GradleException(
                "Rust Android library is missing: ${source.absolutePath}. " +
                    "Run `vpr tauri android build` so the Tauri CLI can build it before Gradle.",
            )
        }
        val destination = File(
            project.projectDir,
            "src/main/jniLibs/$abi/libyour_szu_life_lib.so",
        )
        if (!destination.isFile || destination.canonicalFile != source.canonicalFile) {
            throw GradleException(
                "Android JNI library for $abi does not point to the $profile Tauri " +
                    "artifact ${source.absolutePath}: ${destination.absolutePath}. " +
                    "Run `vpr tauri android build` instead of invoking Gradle alone.",
            )
        }
        project.logger.info("Using Tauri-built Android JNI library for $abi: ${source.absolutePath}")
    }

    private fun targetSpec(target: String): Pair<String, String> = when (target) {
        "aarch64" -> "aarch64-linux-android" to "arm64-v8a"
        "armv7" -> "armv7-linux-androideabi" to "armeabi-v7a"
        "i686" -> "i686-linux-android" to "x86"
        "x86_64" -> "x86_64-linux-android" to "x86_64"
        else -> throw GradleException("Unsupported Android Rust target: $target")
    }
}
