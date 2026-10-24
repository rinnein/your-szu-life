use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const CONTENT_ENV_VARS: [&str; 2] = ["SZU_LIFE_BASE_URL", "SZU_LIFE_FALLBACK_BASE_URL"];

/// Tauri CLI 只把 `bundle.externalBin` 打进桌面端安装包，Android 侧不会处理它。
/// Android 的 `nativeLibraryDir` 只包含 APK 内 `lib/<abi>/` 的条目，所以这里把侧车
/// 二进制以 `libaria2c.so` 的形态放进 `jniLibs` 由 Gradle 打包；运行时 `dlopen`
/// 取回嵌入的可执行文件，落到应用私有目录后再执行（Android 10+ 只允许执行私有目录
/// 下的文件，`lib/` 是只读映射，不能直接 `execve`）。
const ARIA2C_ANDROID_LIB_NAME: &str = "libaria2c.so";
/// 嵌入共享库时使用的固定文件名，决定导出符号名 `_binary_payload_bin_*`。
const PAYLOAD_FILE_NAME: &str = "payload.bin";

fn load_dotenv(path: &std::path::Path) -> HashMap<String, String> {
    if !path.is_file() {
        return HashMap::new();
    }
    dotenvy::from_path_iter(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        .collect::<Result<HashMap<_, _>, _>>()
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn configured_value(name: &str, dotenv: &HashMap<String, String>) -> Option<String> {
    match env::var_os(name) {
        Some(value) => value.into_string().ok(),
        None => dotenv.get(name).cloned(),
    }
}

const WIN_32_URL: &str = "https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0-win-32bit-build1.zip";
const WIN_64_URL: &str = "https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0-win-64bit-build1.zip";
const ANDROID_AARCH64_URL: &str = "https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0-aarch64-linux-android-build1.zip";

fn prepare_aria2c_binary(manifest_dir: &Path) {
    let target = env::var("TARGET").unwrap_or_else(|_| "x86_64-unknown-linux-gnu".into());
    let binaries_dir = manifest_dir.join("binaries");
    fs::create_dir_all(&binaries_dir).expect("failed to create binaries dir");

    let is_windows = target.contains("windows");
    let ext = if is_windows { ".exe" } else { "" };
    let dest_name = format!("aria2c-{target}{ext}");
    let dest_path = binaries_dir.join(&dest_name);

    if dest_path.is_file() {
        if target.contains("android") {
            stage_android_sidecar_lib(manifest_dir, &target, &dest_path);
        }
        return;
    }

    if target.contains("linux") && !target.contains("android") {
        let system_paths = ["/usr/bin/aria2c", "/usr/local/bin/aria2c"];
        let mut found = None;
        for p in system_paths {
            if Path::new(p).is_file() {
                found = Some(PathBuf::from(p));
                break;
            }
        }
        if found.is_none() {
            if let Ok(output) = std::process::Command::new("which").arg("aria2c").output() {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !path_str.is_empty() && Path::new(&path_str).is_file() {
                        found = Some(PathBuf::from(path_str));
                    }
                }
            }
        }

        if let Some(src) = found {
            fs::copy(&src, &dest_path)
                .unwrap_or_else(|e| panic!("failed to copy {}: {e}", src.display()));
            #[cfg(unix)]
            {
                let mut perms = fs::metadata(&dest_path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dest_path, perms).unwrap();
            }
            return;
        } else {
            panic!("Cannot find aria2c on system. Please install aria2 via your package manager (e.g. sudo apt install aria2).");
        }
    }

    let download_url = if target.contains("windows") {
        if target.contains("i686") {
            WIN_32_URL
        } else {
            WIN_64_URL
        }
    } else if target.contains("aarch64") && target.contains("android") {
        ANDROID_AARCH64_URL
    } else {
        println!("cargo:warning=No official aria2c binary for target {target}, creating placeholder");
        let _ = File::create(&dest_path);
        return;
    };

    println!("cargo:warning=Downloading aria2c for {target} from {download_url}...");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("failed to create reqwest client");

    match client.get(download_url).send().and_then(|r| r.error_for_status()) {
        Ok(resp) => {
            let bytes = resp.bytes().expect("failed to read response bytes");
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("failed to read zip archive");
            let target_file_name = if is_windows { "aria2c.exe" } else { "aria2c" };
            let mut extracted = false;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).expect("zip entry error");
                if file.name().ends_with(target_file_name) {
                    let mut out = File::create(&dest_path).expect("failed to create dest file");
                    std::io::copy(&mut file, &mut out).expect("failed to write binary");
                    extracted = true;
                    break;
                }
            }
            if !extracted {
                panic!("binary {target_file_name} not found in zip from {download_url}");
            }
            #[cfg(unix)]
            if !is_windows {
                let mut perms = fs::metadata(&dest_path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dest_path, perms).unwrap();
            }
        }
        Err(e) => {
            println!("cargo:warning=Failed to download aria2c for {target}: {e}");
            let _ = File::create(&dest_path);
        }
    }

    // 下载成功和下载失败（占位文件）都会走到这里；占位文件必须直接跳过，
    // 否则后面的 soname 补写会因为「不是有效 ELF」而让整个构建失败。
    if target.contains("android") && dest_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        stage_android_sidecar_lib(manifest_dir, &target, &dest_path);
    }
}

/// Android 的 ABI 目录名（`jniLibs/<abi>/`）与 Rust target 三元的对应关系。
fn android_abi_for_target(target: &str) -> Option<&'static str> {
    if target.starts_with("aarch64") {
        Some("arm64-v8a")
    } else if target.starts_with("armv7") {
        Some("armeabi-v7a")
    } else if target.starts_with("i686") {
        Some("x86")
    } else if target.starts_with("x86_64") {
        Some("x86_64")
    } else {
        None
    }
}

/// 侧车库的最终落点：`src-tauri/gen/android/app/src/main/jniLibs/<abi>/libaria2c.so`。
///
/// `app/src/main/jniLibs` 是 Android Gradle Plugin 默认就会合并进 APK 的
/// `lib/<abi>/` 目录，因此无需改动 Tauri 自动生成的 `app/build.gradle.kts`。
fn android_sidecar_lib_path(manifest_dir: &Path, target: &str) -> Option<PathBuf> {
    Some(
        manifest_dir
            .join("gen/android/app/src/main/jniLibs")
            .join(android_abi_for_target(target)?)
            .join(ARIA2C_ANDROID_LIB_NAME),
    )
}

fn stage_android_sidecar_lib(manifest_dir: &Path, target: &str, sidecar: &Path) {
    let Some(dest) = android_sidecar_lib_path(manifest_dir, target) else {
        println!(
            "cargo:warning=暂不支持为 {target} 打包 Android 侧车二进制，APK 内将不包含 {ARIA2C_ANDROID_LIB_NAME}"
        );
        return;
    };

    let Ok(source_meta) = sidecar.metadata() else {
        println!("cargo:warning=读取 {} 失败，跳过", sidecar.display());
        return;
    };
    if source_meta.len() == 0 {
        // 下载失败时 build.rs 只留下了空占位文件，说明当前离线/无网络，
        // 此时不写入残缺的库，宁可让运行时明确报「未找到 aria2c」。
        println!(
            "cargo:warning={} 是空占位文件，APK 内将不包含 {ARIA2C_ANDROID_LIB_NAME}",
            sidecar.display()
        );
        return;
    }

    // 目标比源文件新才复用，避免改了侧车源之后仍然打包旧内容。
    if let (Ok(dest_meta), Ok(src_time)) = (dest.metadata(), source_meta.modified()) {
        if dest_meta.len() > 0 && matches!(dest_meta.modified(), Ok(dest_time) if dest_time >= src_time) {
            return;
        }
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
    }

    let bytes = build_android_sidecar_lib(sidecar, target, ARIA2C_ANDROID_LIB_NAME).unwrap_or_else(|| {
        panic!(
            "无法生成 Android 侧车库 {ARIA2C_ANDROID_LIB_NAME}：\
             未找到 Android NDK 的 ld.lld（请设置 ANDROID_NDK_HOME 或安装 NDK）。\
             Tauri CLI 不会为 Android 打包 `bundle.externalBin`，该库是把 {} 嵌入 APK \
             的唯一途径，因此这里不能跳过。",
            sidecar.display()
        )
    });

    fs::write(&dest, bytes).unwrap_or_else(|e| panic!("failed to write {}: {e}", dest.display()));

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&dest).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms).unwrap();
    }

    println!(
        "cargo:warning=已为 Android 准备侧车库 {}（来源 {}）",
        dest.display(),
        sidecar.display()
    );
    println!("cargo:rerun-if-changed={}", sidecar.display());
}

/// 生成 Android 可加载的侧车库 `libaria2c.so`。
///
/// Android 只允许执行应用私有目录下的文件，而 APK 里的 `lib/<abi>/*.so` 是只读映射，
/// 因此侧车二进制不能直接 `execve`。做法是把它嵌进一个最小的共享库，运行时
/// `dlopen` 后复制到私有目录再执行。
///
/// 为什么不用 `objcopy` 打补丁：`.dynamic` 之后紧跟 `.dynstr`/`.dynsym`，没有空隙容纳
/// 新字符串；`llvm-objcopy --rename-section` 只能把字符串写进 `.shstrtab`，而该段位于
/// 文件末尾、无法前移到 `.dynstr` 处，所以这条路走不通。`ld.lld`（NDK 自带）则支持
/// `--soname` 与 `-b binary`，可以一步生成带正确 DT_SONAME 的共享库：
///
/// - `--soname=<name>` 生成合法的 DT_SONAME，满足 Android linker 的要求；
/// - 以 `-b binary` 引入原始可执行文件，其内容原样嵌入新库并导出
///   `_binary_payload_bin_start/end/size`，运行时据此还原出 aria2c。
fn build_android_sidecar_lib(payload: &Path, target: &str, lib_name: &str) -> Option<Vec<u8>> {
    let linker = find_android_tool("ld.lld")?;
    let emulation = match target {
        t if t.starts_with("aarch64") => "aarch64linux",
        t if t.starts_with("armv7") => "armelf_linux_eabi",
        t if t.starts_with("i686") => "elf_i386",
        t if t.starts_with("x86_64") => "elf_x86_64",
        _ => return None,
    };

    let work_dir = std::env::temp_dir().join(format!(
        "szu-life-sidecar-{target}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&work_dir);
    fs::create_dir_all(&work_dir).ok()?;

    // 固定文件名，确保导出的符号名恒为 `_binary_payload_bin_*`。
    let payload_copy = work_dir.join(PAYLOAD_FILE_NAME);
    fs::copy(payload, &payload_copy).ok()?;

    let status = Command::new(&linker)
        .current_dir(&work_dir)
        .args([
            "-m",
            emulation,
            "-shared",
            "--no-dynamic-linker",
            "--no-rosegment",
            "-z",
            "notext",
            "-e",
            "0",
            &format!("--soname={lib_name}"),
            "-o",
            lib_name,
            "-b",
            "binary",
            PAYLOAD_FILE_NAME,
        ])
        .status()
        .ok()?;

    let linked = work_dir.join(lib_name);
    if !status.success() {
        let _ = fs::remove_dir_all(&work_dir);
        return None;
    }

    // 只保留运行时需要的符号：`.symtab` 里只有调试信息，剥离后可缩小约 40%。
    // `.dynsym`（含 `_binary_payload_bin_*`）不受影响。
    if let Some(strip) = find_android_tool("llvm-strip") {
        let _ = Command::new(strip)
            .args(["--strip-unneeded", "-o"])
            .arg(work_dir.join("stripped.so"))
            .arg(&linked)
            .status();
    }
    let stripped = work_dir.join("stripped.so");
    let bytes = fs::read(if stripped.is_file() { &stripped } else { &linked }).ok();

    let _ = fs::remove_dir_all(&work_dir);

    bytes.filter(|candidate| {
        Elf64Lite::parse(candidate)
            .and_then(|elf| elf.soname())
            .as_deref()
            == Some(lib_name)
    })
}

/// 极简 ELF64（小端）解析，只取侧车库打包所需的几个字段。
struct Elf64Lite<'a> {
    data: &'a [u8],
    phoff: usize,
    phentsize: usize,
    phnum: usize,
}

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const DT_STRTAB: u64 = 5;
const DT_STRSZ: u64 = 10;
const DT_SONAME: u64 = 14;

impl<'a> Elf64Lite<'a> {
    fn parse(data: &'a [u8]) -> Option<Self> {
        // ELF 魔数 + 64 位 + 小端（Android 各 ABI 均为小端）。
        if data.len() < 64 || &data[..4] != b"\x7fELF" || data[4] != 2 || data[5] != 1 {
            return None;
        }
        Some(Self {
            data,
            phoff: usize::try_from(read_u64(data, 0x20)?).ok()?,
            phentsize: read_u16(data, 0x36)? as usize,
            phnum: read_u16(data, 0x38)? as usize,
        })
    }

    /// 返回 `(p_type, p_offset, p_vaddr, p_filesz)`。
    fn segment(&self, index: usize) -> Option<(u32, u64, u64, u64)> {
        let base = self.phoff.checked_add(index.checked_mul(self.phentsize)?)?;
        if base + 56 > self.data.len() {
            return None;
        }
        Some((
            read_u32(self.data, base)?,
            read_u64(self.data, base + 8)?,
            read_u64(self.data, base + 16)?,
            read_u64(self.data, base + 32)?,
        ))
    }

    fn segments(&self) -> impl Iterator<Item = (u32, u64, u64, u64)> + '_ {
        (0..self.phnum).filter_map(move |index| self.segment(index))
    }

    /// `.dynamic` 段的 `(文件偏移, 长度, 虚拟地址)`。
    fn dynamic_segment(&self) -> Option<(usize, usize, u64)> {
        let (_, p_offset, p_vaddr, p_filesz) = self
            .segments()
            .find(|(p_type, ..)| *p_type == PT_DYNAMIC)?;
        Some((
            usize::try_from(p_offset).ok()?,
            usize::try_from(p_filesz).ok()?,
            p_vaddr,
        ))
    }

    fn dynamic_value(&self, tag: u64) -> Option<u64> {
        let (offset, filesz, _) = self.dynamic_segment()?;
        let bytes = self.data.get(offset..offset.checked_add(filesz)?)?;
        bytes
            .chunks_exact(16)
            .map(|entry| (read_u64(entry, 0).unwrap_or(u64::MAX), read_u64(entry, 8).unwrap_or(0)))
            .find(|(entry_tag, _)| *entry_tag == tag)
            .map(|(_, value)| value)
    }

    /// 虚拟地址 -> 文件偏移（基于第一个 PT_LOAD 的 `p_offset - p_vaddr` 位移）。
    fn vaddr_to_file_offset(&self, vaddr: u64) -> Option<usize> {
        let (_, p_offset, p_vaddr, _) = self
            .segments()
            .find(|(p_type, _, _, _)| *p_type == PT_LOAD)?;
        usize::try_from(p_offset.checked_add(vaddr.checked_sub(p_vaddr)?)?).ok()
    }

    /// 读取 DT_SONAME 指向的字符串。
    ///
    /// `DT_SONAME` 是相对 `DT_STRTAB` 的偏移，而 `DT_STRTAB` 记录的是虚拟地址，
    /// 需要按 PT_LOAD 的 `p_offset - p_vaddr` 换算成文件偏移。
    fn soname(&self) -> Option<String> {
        let soname_offset = usize::try_from(self.dynamic_value(DT_SONAME)?).ok()?;
        let strtab_at = self.dynamic_value(DT_STRTAB)?;
        let strtab_len = usize::try_from(self.dynamic_value(DT_STRSZ)?).ok()?;

        // DT_SONAME 是相对 DT_STRTAB 起始位置的偏移，换算成文件偏移后截取到 NUL。
        let table_file = self.vaddr_to_file_offset(strtab_at)?;
        if soname_offset >= strtab_len {
            return None;
        }
        let string_at = table_file.checked_add(soname_offset)?;
        let limit = table_file.checked_add(strtab_len)?.min(self.data.len());
        let tail = self.data.get(string_at..limit)?;
        let nul = tail.iter().position(|byte| *byte == 0)?;
        if nul == 0 {
            return None;
        }
        std::str::from_utf8(tail.get(..nul)?).ok().map(str::to_owned)
    }
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(offset..offset + 2)?.try_into().ok()?))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(data.get(offset..offset + 8)?.try_into().ok()?))
}

/// 在 NDK（或系统 PATH）中定位 llvm 工具。
fn find_android_tool(name: &str) -> Option<PathBuf> {
    let mut ndk_roots: Vec<PathBuf> = Vec::new();
    if let Some(ndk_home) = env::var_os("ANDROID_NDK_HOME").and_then(|v| v.into_string().ok()) {
        ndk_roots.push(PathBuf::from(ndk_home));
    }
    if let Some(ndk_home) = env::var_os("ANDROID_NDK_ROOT").and_then(|v| v.into_string().ok()) {
        ndk_roots.push(PathBuf::from(ndk_home));
    }
    for sdk_root in [
        env::var("ANDROID_HOME").ok(),
        env::var("ANDROID_SDK_ROOT").ok(),
    ]
    .into_iter()
    .flatten()
    {
        for version_dir in sorted_dirs(&Path::new(&sdk_root).join("ndk")) {
            ndk_roots.push(version_dir);
        }
    }

    let host_triples = ["linux-x86_64", "darwin-x86_64", "darwin-arm64", "windows-x86_64"];
    for root in ndk_roots {
        for host in host_triples {
            let candidate = root.join("toolchains/llvm/prebuilt").join(host).join("bin").join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 兜底：发行版自带的 LLVM 工具链
    for directory in env::split_paths(&env::var_os("PATH")?) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn sorted_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    paths.sort();
    paths.reverse();
    paths
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dotenv_path = manifest_dir.join("../.env");
    let dotenv = load_dotenv(&dotenv_path);
    println!("cargo:rerun-if-changed={}", dotenv_path.display());
    for name in CONTENT_ENV_VARS {
        if let Some(value) = configured_value(name, &dotenv) {
            println!("cargo:rustc-env={name}={value}");
        }
        println!("cargo:rerun-if-env-changed={name}");
    }

    println!("cargo:rerun-if-changed=../package.json");
    println!("cargo:rerun-if-changed=../github-accelerator.json");
    println!("cargo:rerun-if-changed=../huggingface-accelerator.json");
    println!("cargo:rerun-if-changed=resources/ocr/ddddocr/model.json");
    println!("cargo:rerun-if-changed=resources/embedding/bge-small-zh-v1.5/model.json");
    println!("cargo:rerun-if-changed=capabilities/authserver-credential-capture.json");

    prepare_aria2c_binary(&manifest_dir);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_mcp_server_status",
            "set_mcp_server_enabled",
            "enrollment_command",
            "open_academic_portal",
            "open_enrollment_official",
            "get_ocr_model_status",
            "download_ocr_model",
            "update_ocr_model",
            "get_embedding_model_status",
            "download_embedding_model",
            "update_embedding_model",
            "load_oj_image",
            "save_oj_preferences",
            "configure_oj_companion",
            "set_oj_companion_status",
            "activate_oj_companion",
            "submit_oj_companion",
            "get_oj_snapshot",
            "save_oj_site",
            "remove_oj_site",
            "login_oj",
            "get_oj_captcha",
            "auto_recognize_oj_captcha",
            "get_oj_groups",
            "get_oj_contests",
            "get_oj_paper",
            "get_oj_progress",
            "get_oj_ranking",
            "save_oj_draft",
            "get_oj_draft",
            "submit_oj",
            "get_oj_records",
            "get_oj_result",
            "acknowledge_oj_submission",
            "send_oj_companion",
            "refresh_oj",
            "read_oj_notice",
            "get_uooc_snapshot",
            "get_uooc_ai_config",
            "get_uooc_ai_connections",
            "save_uooc_ai_connections",
            "save_uooc_ai_preferences",
            "list_uooc_ai_models",
            "save_uooc_quiz_mode",
            "save_uooc_quiz_answers",
            "control_uooc_quiz",
            "open_uooc_quiz_portal",
            "refresh_uooc_progress",
            "start_uooc_checkin",
            "cancel_uooc_checkin",
            "start_uooc_discussion",
            "refresh_uooc_session",
            "open_uooc_login",
            "logout_uooc",
            "save_uooc_settings",
            "start_uooc_courses",
            "control_uooc_tasks",
            "get_uooc_embedding_status",
            "download_uooc_qa_vectors",
            "search_uooc_qa_questions",
            "sync_uooc_qa_bank",
            "get_uooc_qa_account_status",
            "save_uooc_qa_api_key",
            "reset_uooc_qa_api_key",
            "start_uooc_qa_device_auth",
            "poll_uooc_qa_device_auth",
            "upload_all_uooc_qa_bank",
            "list_uooc_exams",
            "get_uooc_exam_paper",
            "save_uooc_exam_draft",
            "submit_uooc_exam",
            "heartbeat_uooc_exam",
            "list_uooc_quizzes",
            "list_uooc_homeworks",
            "get_uooc_eval_list",
            "submit_uooc_eval",
            "generate_uooc_subjective",
            "open_uooc_task_portal",
            "match_uooc_paper_qa",
            "solve_uooc_paper_ai",
            "fetch_uooc_completed_quiz",
            "get_uooc_local_bank_questions",
            "clear_uooc_local_bank",
            "get_uooc_course_title",
            "build_uooc_local_bank",
            "uooc_qa_http_request",
            "get_app_content_snapshot",
            "refresh_app_content",
            "get_contributors",
            "load_announcement_image",
            "start_in_app_update",
            "get_credential_storage_status",
            "get_settings_snapshot",
            "export_settings",
            "preview_import_settings",
            "import_settings",
            "get_auth_state",
            "refresh_auth_session",
            "get_account_profile",
            "get_user_data",
            "begin_authentication",
            "capture_auth_credentials",
            "control_auth_recovery",
            "confirm_profile",
            "logout",
            "open_network_fee_portal",
            "consume_network_fee_portal_notice",
            "get_electricity_options",
            "get_electricity_settings",
            "save_electricity_settings",
            "query_electricity",
            "get_electricity_status",
            "get_cached_electricity",
            "get_electricity_constants",
            "export_electricity_settings",
            "import_electricity_settings",
            "query_electricity_recharges",
            "set_application_visibility",
            "get_network_overview",
            "refresh_campus_network",
            "request_campus_network_permissions",
            "get_debug_mode",
            "get_debug_logs",
            "get_debug_logs_jsonl",
            "set_debug_mode",
            "clear_debug_logs",
            "get_staff_network_settings",
            "save_staff_network_settings",
            "get_staff_login_status",
            "apply_staff_network_mode",
            "consume_staff_login_failure_notice",
            "get_dorm_network_settings",
            "save_dorm_network_settings",
            "get_dorm_login_status",
            "trigger_dorm_login",
            "logout_campus_network",
            "get_dorm_login_logs",
            "consume_dorm_login_failure_notice",
        ]),
    ))
    .expect("failed to build Tauri ACL metadata");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 侧车库必须导出的载荷符号（与 `src/aria2/binary.rs` 保持一致）。
    const PAYLOAD_SYMBOLS: [&str; 3] = [
        "_binary_payload_bin_start",
        "_binary_payload_bin_end",
        "_binary_payload_bin_size",
    ];

    fn elf_bytes_with_soname(lib_name: &str, payload: &[u8]) -> Option<Vec<u8>> {
        let temp = std::env::temp_dir().join(format!("sidecar-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).ok()?;
        let payload_path = temp.join("aria2c");
        fs::write(&payload_path, payload).ok()?;
        let result = build_android_sidecar_lib(&payload_path, "aarch64-linux-android", lib_name);
        let _ = fs::remove_dir_all(&temp);
        result
    }

    #[test]
    fn android_abi_mapping_covers_all_shipped_abis() {
        assert_eq!(android_abi_for_target("aarch64-linux-android"), Some("arm64-v8a"));
        assert_eq!(android_abi_for_target("armv7-linux-androideabi"), Some("armeabi-v7a"));
        assert_eq!(android_abi_for_target("i686-linux-android"), Some("x86"));
        assert_eq!(android_abi_for_target("x86_64-linux-android"), Some("x86_64"));
        assert_eq!(android_abi_for_target("x86_64-unknown-linux-gnu"), None);
    }

    #[test]
    fn sidecar_lib_path_is_inside_main_jnilibs() {
        let path = android_sidecar_lib_path(Path::new("/app/src-tauri"), "aarch64-linux-android")
            .expect("android target must map to an ABI");
        assert!(
            path.ends_with("gen/android/app/src/main/jniLibs/arm64-v8a/libaria2c.so"),
            "unexpected sidecar path: {}",
            path.display()
        );
    }

    #[test]
    fn soname_is_read_back_from_generated_library() {
        let Some(bytes) = elf_bytes_with_soname(ARIA2C_ANDROID_LIB_NAME, b"aria2c-payload")
        else {
            // 缺少 Android NDK 时无法交叉链接，这类环境下跳过（CI 会安装 NDK）。
            return;
        };
        let elf = Elf64Lite::parse(&bytes).expect("generated library must be a valid ELF64");
        assert_eq!(elf.soname().as_deref(), Some(ARIA2C_ANDROID_LIB_NAME));
        // 原始可执行文件必须整体嵌入，运行时才能还原出可执行的 aria2c。
        assert!(
            bytes
                .windows(b"aria2c-payload".len())
                .any(|window| window == b"aria2c-payload"),
            "payload bytes must be embedded in the sidecar library"
        );
    }

    #[test]
    fn generated_library_reserves_dynamic_slot_for_soname() {
        let Some(bytes) = elf_bytes_with_soname(ARIA2C_ANDROID_LIB_NAME, b"payload") else {
            return;
        };
        let elf = Elf64Lite::parse(&bytes).expect("valid ELF64");
        assert!(
            elf.dynamic_value(DT_SONAME).is_some(),
            "sidecar library must carry a DT_SONAME entry for the Android linker"
        );
        assert!(
            elf.dynamic_value(DT_STRTAB).is_some() && elf.dynamic_value(DT_STRSZ).is_some(),
            "sidecar library must carry DT_STRTAB + DT_STRSZ for DT_SONAME lookup"
        );
        // 运行时通过 dlsym 取这三个符号定位嵌入的载荷。
        for symbol in PAYLOAD_SYMBOLS {
            assert!(
                bytes
                    .windows(symbol.len())
                    .any(|window| window == symbol.as_bytes()),
                "generated library must export {symbol}"
            );
        }
    }
}
