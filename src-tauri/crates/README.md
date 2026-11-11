# Rust workspace crate 地图

`src-tauri` 既是 workspace 根，也是 app 包自己的目录。这样 `Cargo.lock` 仍在
`src-tauri/Cargo.lock`、`target/` 仍在 `src-tauri/target`，Android 的
`gen/android/buildSrc`（按 `rootDirRel = "../../../"` 找
`libyour_szu_life_lib.so`）和 Gradle（读 `../../Cargo.lock`）都不用改。

## 依赖方向（从下到上）

```
szu-build-support        build.rs 复用逻辑（平台 cfg、.env 注入），不参与运行
szu-core                 平台基础 / 凭据与统一认证 / 网络代理 / 校园网 / 电费 / 通知 / 向导
  ├─ szu-download        aria2 进程与 RPC、统一下载目录
  │    └─ szu-runtime    本地模型下载校验、OCR、向量模型
  ├─ szu-app-content     公告、贡献者、应用内更新
  ├─ szu-enrollment      抢课与学业门户
  ├─ szu-jxxk            监察巡课（回放、拉流、下载）
  ├─ szu-lms             畅课
  ├─ szu-oj              OJ 刷题伴侣
  ├─ szu-uooc            UOOC
  ├─ szu-todos           待办聚合（依赖 oj / lms / uooc）
  └─ szu-mesh            EasyTier 组网 + 设置同步（依赖 uooc）
your-szu-life (src/)     app：组合根、mcp 工具胶水、桌面托盘、命令名守卫测试
```

方向只能从上往下依赖。新增功能时先判断它属于哪个已有 crate；确实要开新 crate
时，也必须落在同一个方向上，不要制造循环。

## 约定

- **这些 crate 不是公共 API。** `pub` 只是为了跨 crate 编译；不要在 crate 之外
  新增对内部类型的直接依赖，也不要把它当成稳定接口。
- **模块目录结构原样保留**：`szu-uooc/src/uooc/...` + `lib.rs` 里 `pub mod uooc;`。
  这样 crate 内部的 `crate::uooc::…`、`super::…` 路径不用改。
- **`cfg(desktop)` / `cfg(mobile)` / `cfg(dev)` 必须自己声明**：`tauri-build` 只给
  它自己那个 crate 打印这些 cfg。用到它们的 crate 要加 `build.rs` 调用
  `szu_build_support::emit_platform_cfgs()`（见 `szu-core` / `szu-oj` / `szu-todos`）。
- **编译期环境变量**：`.env` 里的 `SZU_LIFE_*` / `APTABASE_APP_KEY` 由
  `szu_build_support::emit_embedded_env()` 注入，依赖 crate 不会自动继承 app 的
  `cargo:rustc-env`。
- **`include_str!` 跨仓库根**用 `concat!(env!("CARGO_MANIFEST_DIR"), "/../../../…")`，
  不要写死相对层数。
- **应用版本**：唯一来源是仓库根 `package.json`（`tauri.conf.json` 的 `version` 指向它）。
  运行时需要版本时用 `package_info().version`；Cargo 的 `[workspace.package].version`
  必须与之一致，app 的 `build.rs` 会在构建期报错。
- **命令注册**：`#[tauri::command]` 跟随模块留在各自 crate 里，app 的
  `generate_handler!` 用完整路径引用；`build.rs` 的 `AppManifest` 命令表与
  `permissions/default.toml` 仍由 `src/tests.rs` 的命令名守卫测试交叉校验。
- **数据库迁移跟随模块**：各功能 crate 的 `migrations.rs` 只放自己的表结构；
  通用句柄工厂在 `szu_core::migrations::db`。
- **测试辅助**（`storage::testing`、`reset_registry`、`TEST_ENV_LOCK`）不能挂在
  `cfg(test)` 上，否则兄弟 crate 的测试看不到；`szu-runtime` 另提供 `testing`
  feature 供 `szu-oj` 的 dev-dependencies 打开。

## 常用命令

```sh
# 整个 workspace 的检查与测试（会跑所有成员 crate 的测试）
cargo check --workspace --manifest-path src-tauri/Cargo.toml
cargo test  --workspace --manifest-path src-tauri/Cargo.toml

# 只验证某一块（增量构建最快的方式）
cargo check -p szu-uooc
cargo test  -p szu-uooc
```

注意：`cargo test --manifest-path src-tauri/Cargo.toml`（不带 `--workspace`）只跑
app 包自己的测试；功能模块的测试在各自 crate 里。
