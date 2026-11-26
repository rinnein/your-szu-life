# Linux 安装说明

泥深提供 Linux x64 的 deb 与 AppImage；AUR 包由 CI 从 canary 同步。专家按发行版分流，**Debian/Arch 的命令由用户执行，agent 不执行 sudo/AUR**。

## Debian / Ubuntu / Linux Mint 等

`szu-life install` 会返回一个 bash 脚本，包含：

1. `mktemp -d` 临时目录与 `trap` 清理；
2. `curl`（或 `wget`）下载最新正式版 deb，主源失败时回退备用源；
3. `sudo apt install -y <deb>`（没有 `apt` 时用 `apt-get`）。

把脚本原样交给用户执行。deb 会安装到 `/usr/bin/your-szu-life`、`/usr/share/applications/your-szu-life.desktop`，依赖由 apt 解析。

用户执行后验证：

```bash
szu-life status
# 或手动
test -x /usr/bin/your-szu-life
dpkg-query -S /usr/bin/your-szu-life
```

如果用户使用 Debian 系但不想 sudo，可显式要求 AppImage：

```bash
szu-life install --format appimage --yes
```

## Arch / Manjaro / EndeavourOS / CachyOS 等

`szu-life install` 会返回 AUR 安装代码块：

```bash
if command -v yay >/dev/null 2>&1; then
  yay -S --needed your-szu-life-bin
elif command -v paru >/dev/null 2>&1; then
  paru -S --needed your-szu-life-bin
else
  tmp_dir="$(mktemp -d)"
  git clone https://aur.archlinux.org/your-szu-life-bin.git "$tmp_dir/your-szu-life-bin"
  (cd "$tmp_dir/your-szu-life-bin" && makepkg -si)
fi
```

- 实际 AUR 包名是 `your-szu-life-bin`（provides `your-szu-life`）。
- AUR 包由 CI 跟随 **canary**；需要正式版时用 deb 或 AppImage。
- 用户执行后验证：`pacman -Qo /usr/bin/your-szu-life`。

## 其它发行版（Fedora、openSUSE、NixOS、SteamOS、Bazzite 等）

专家会在征得同意后运行 `szu-life install --yes`，把 AppImage 安装到 XDG 用户目录，**不需要 sudo**：

| 内容       | 路径                                                                                |
| ---------- | ----------------------------------------------------------------------------------- |
| AppImage   | `${XDG_DATA_HOME:-$HOME/.local/share}/your-szu-life/your-szu-life.AppImage`         |
| 启动包装   | `${XDG_BIN_HOME:-$HOME/.local/bin}/your-szu-life`                                   |
| 桌面项     | `${XDG_DATA_HOME:-$HOME/.local/share}/applications/your-szu-life.desktop`           |
| 图标       | `${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps/your-szu-life.png` |
| 安装元数据 | `${XDG_DATA_HOME:-$HOME/.local/share}/in.rinne.your-szu-life/install.json`          |

- 桌面项带 `Exec="<launcher>" %u` 与 `MimeType=x-scheme-handler/your-szu-life;`。
- 如果系统没有 `libfuse2`，启动包装会自动设置 `APPIMAGE_EXTRACT_AND_RUN=1`。
- 安装器会尝试 `xdg-mime default` 与 `update-desktop-database`；失败不影响应用启动后的运行期协议注册。
- 卸载：删除 AppImage、启动包装、桌面项、图标与 `install.json`。

## 通用注意

- 安装前必须征得用户同意。
- 不要执行 `curl | sh`；先下载、校验魔数/大小（CNB 提供 SHA-256 时也校验），再安装。
- 下载源顺序：CNB → GitHub 加速（gh-proxy/ghfast）→ GitHub 直连。
- 用户在受限网络/代理环境时，`curl`/`wget` 会继承系统代理；失败时先检查网络与代理。
