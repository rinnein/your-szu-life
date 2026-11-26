# Windows 安装说明

泥深提供 Windows x64 的 MSI。安装仍是 stable 优先、无 stable 包时回退 canary。

## 自动安装流程

`szu-life install --yes` 会：

1. 从 CNB（优先）或 GitHub 解析最新 MSI；
2. 下载后用 OLE 魔数与大小校验，CNB/GitHub 提供 SHA-256 时一并校验；
3. 通过 PowerShell `Start-Process -Verb RunAs` 调起 `msiexec /i <msi> /qn /norestart`，**会弹 UAC**；
4. 退出码 0 或 3010 视为成功；在 `Program Files\your-szu-life` 或 `%LOCALAPPDATA%\Programs\your-szu-life` 查找 `your-szu-life.exe`；
5. 在 `HKCU\Software\Classes\your-szu-life` 写入 URL protocol，作为应用启动后运行期注册的兜底；
6. 写入 `%APPDATA%\in.rinne.your-szu-life\install.json`。

## 用户提示

- 安装前说明会弹 UAC；如果用户拒绝，不要重试绕过，让用户重新运行并允许管理员权限。
- 如果已有旧版本在运行（托盘驻留），安装/升级前让用户右键托盘图标 → 退出，避免文件占用。
- 安装完成后用 `szu-life status` 验证 `installed`、版本与 `running`。
- 首次启动可能被 Windows SmartScreen 拦截；这是未签名安装包的预期行为，不要建议关闭系统安全功能。

## 手动兜底

如果 MSI 安装失败，用户可以到 GitHub Releases 或 CNB Releases 手动下载：

```text
https://github.com/rinnein/your-szu-life/releases
https://cnb.cool/rinnein/szu/your-szu-life/-/releases
```

下载 `your-szu-life_windows_x64_<版本>-release.msi`，双击安装。

## 卸载

设置 → 应用 → 已安装的应用 → 泥深 → 卸载；或使用 MSI 的卸载项。`HKCU\Software\Classes\your-szu-life` 可保留，重新安装后会覆盖。
