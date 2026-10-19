# 泥深

你的深大，你的生活！

功能列表：

- [x] 更好用的选课系统
- [x] 选课+培养方案检查
- [x] 课程时间表
- [ ] 更好用的成绩查询
- [x] 自动校园网登录(区分教工区、宿舍区，可配置登录选项)
- [x] 校园卡与缴费(电费+网费)快捷服务、电费欠费提醒
- [x] UOOC自动刷课(自动签到、AI答题、进度检查)
- [x] 计软合集-OJ管理(支持自动验证码、自定义OJ、CPH-NG支持、Markdown导出支持)

以下功能正在快速迭代中，暂不保证稳定性：

- UOOC AI答题
- UOOC 答案共享
- 选课抢课(等寒假开平台才能测试)

另外，软件内置了一个验证码识别模型(~60MB)，支持自动识别验证码，减少手动输入验证码的麻烦。

## Arch Linux / makepkg

需要已安装 `base-devel`，其余依赖可由 `makepkg -s` 安装。

请在独立目录保存并运行 `PKGBUILD`，不要直接在本仓库根目录运行 makepkg。`makepkg --cleanbuild` 会删除打包目录下的 `src/`，而本仓库同名目录中存放了前端源码。

```bash
mkdir -p ~/build/your-szu-life-package
cd ~/build/your-szu-life-package
curl -fLO https://raw.githubusercontent.com/rinnein/your-szu-life/main/PKGBUILD
LC_ALL=C.UTF-8 makepkg -si
```

该 PKGBUILD 从 GitHub 获取最新源码，不包含其他本地工作目录中的未提交改动。打包时禁用 GCC LTO，避免内置 SQLite 等 C 库的 LTO 对象与 Rust LLD 链接器不兼容。

安装资源包含中文路径。使用 `LC_ALL=C.UTF-8` 可避免 bsdtar 的 `Can't translate pathname ... to UTF-8` 警告；仅设置 `LANG` 不够，因为 makepkg 在归档阶段会覆盖它。如果只需重新归档已有构建结果，可运行：

```bash
LC_ALL=C.UTF-8 makepkg --repackage --force --log
```

### NVIDIA / Wayland 启动问题

如果启动时出现 `Error 71 (Protocol error) dispatching to Wayland display`，可先尝试禁用 NVIDIA 显式同步，保留 DMABUF 渲染路径：

```bash
env -u WEBKIT_DISABLE_DMABUF_RENDERER \
    -u WEBKIT_DISABLE_COMPOSITING_MODE \
    __NV_DISABLE_EXPLICIT_SYNC=1 your-szu-life
```

该方式已在本机 NVIDIA / KDE Wayland 环境验证可正常运行。若仍有问题，可参考 [Tauri 的 Linux 图形问题说明](https://v2.tauri.app/develop/debug/linux-graphics/)。禁用 DMABUF 可能降低渲染性能，因此不作为所有用户的默认设置。
