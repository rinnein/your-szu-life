# 本科选课与学业查询

`enrollment_command` 是受本地应用 origin、窗口身份和 Tauri capability 约束的类型化命令入口，不依赖可能滞后的 WebView 路径；远程认证窗口没有这项权限。`open_academic_portal` 创建独立认证窗口，Android 使用现有 FeatureActivity 返回桥接。

## 数据与认证

- 安全存储中的 `academic-sessions` 分开保存 WebVPN、校内选课、VPN选课和 eHall 会话；前端事件只包含显示数据。
- `academic.sqlite` 使用 SQLite WAL 和同步事务保存工作区、分页目录和任务阶段，按账户及学期/批次/校区隔离。写请求先保存意图，再访问学校。
- 应用启动不自动开始任务。已保存 cookies 先验证；有效时不重新提交密码。账号切换会暂停任务并切换缓存。
- WebVPN CAS callback 使用网关专用 AuthServer 服务地址。cookies 从原生存储读取，Linux 保留 Soup 的 host/domain 区别，Android 保留 AndroidX 元数据；网关探针成功才保存认证结果。
- 验证码每轮使用新的令牌及对应 cookies；现有 ddddocr 分类模型配合 imageproc 字块定位，识别不确定时支持手动四点输入。DES 使用 RustCrypto，额外适配学校 JS 非标准 PC-1 密钥排列，包含官方脚本比对向量。

## 调度约定

- 每账户串行写入；low/medium/high/ultra 最小间隔为 60/10/1/0.25 秒。
- 各目标分别计数，ultra 明确失败5次降 high，high 明确失败30次降 medium；网络故障与初始化不计数。备选组轮转尝试，成功后停止同组其他目标。
- 自动换课仅使用实时容量，并区分主选/非主选名额：监控 → 退旧待确认 → ultra抢新 → 满员后ultra回抢旧 → 旧课确认恢复后重新监控。
- 普通“立即选课一次”不会自动重试业务失败，也不会恢复其他已暂停任务。
- 超时不代表未执行。未决请求保留并核验已选列表，不直接重发。学校部署脚本提供的处理结果接口由 `contract.rs` 只读解析，不执行远程 JS；若无法识别该接口，仅依赖已选列表确认，不猜测失败或照搬新版 WebSocket 协议。
- Android 前台服务与 UOOC 独立。通知可停止任务；服务超时/退出会暂停，系统强制终止后需用户重新打开应用并继续。

## 参考来源

- https://github.com/Weeye-hua/SZU-Course-Help （本科协议、点选验证码、课程模型）
- https://github.com/airline233/Nuist-xsxk （备选与换课调度思路；不使用其新版接口）
- https://github.com/only9464/HEU-Wisedu （功能组织）
- https://github.com/AwesomeHou/szu-cli （深大学业汇总、课组与明细）
- https://github.com/Youwenqwq/ysu-sdk 与 https://github.com/Youwenqwq/ysu-client （学业重算和进度协议）
- 学校公开部署的 `grablessons.js`、`grablessonsBS.js`、`indexBS.js`、`des.min.js` 用于交叉核对。

## 验证

运行 `vp test src/enrollment`、`cargo test --manifest-path src-tauri/Cargo.toml enrollment::`，并运行项目全量检查。测试数据为合成数据，不包含账户 cookies 或凭据。

最终 APK 还需独立验证：2FA完整回跳、实际选退课、真实容量竞争下的换课循环、学校重启后的恢复，以及锁屏后台、Android返回键与页面按钮。编译和模拟测试不代表这些真实流程已通过。

## 页面与学业树

- `/enrollment/login` 独立处理访问方式和登录；有效会话直接进入 `/enrollment` 查课目录。
- 已选课程、任务、课表和设置分别位于 `/enrollment/selected`、`/enrollment/tasks`、`/enrollment/timetable`、`/enrollment/settings`，共用布局和后台快照。旧 `?tab=` 链接会替换到对应页面。
- 独立功能页建立返回层级，同层导航替换；返回目录使用统一逻辑返回桥接。源校验不会因为原生 URL 尚未同步或进入子路径而拒绝本地请求。
- 学业树使用 React Flow + Dagre 横向布局。根节点和任意真实课组都可打开 HeroUI Table 检索弹窗；课程按代码合并，所有所属课组及冲突值保留。
- 分支展开和课程查询是独立操作。展开时保持操作节点位置，返回学业页面时恢复视口、节点及筛选条件。手机端高级筛选可折叠，表格独立横向滚动。
- `src/enrollment/testing/fixtures.ts` 仅供测试及外部浏览器注入使用，不提供生产演示开关或认证绕过入口。

## 准入、缓存与官网会话

- 认证与选课准入独立保存：登录返回 token 即为认证成功；只有学生信息接口的 `code: "2"` 且 `data: null` 判为准入受限，保留学校原文及已认证会话。重新检查准入不提交密码。
- 登录页可直接进入 `?view=cache`；目录检索与分页仅访问当前应用账户的 SQLite 数据，不测速或调用学校接口。缓存缺失明确标记，写入与任务编辑禁用，仍可停止已有任务。
- 学业画布只显示学校 `KZH` / `FKZH` 定义的真实课组，末级课组不绘制课程节点。课程类别 `KCLBDM` / `KCLBDM_DISPLAY` 保留在检索表格，不再人工插入树层；同名真实课组不按名字合并。
- 官网入口创建独立原生窗口，恢复 cookies 元数据和学校实际返回的初始化数据。token 只在原生层与精确学校页面之间传递，官网无应用业务 IPC 权限。
- 官网直接请求学校，不代理、不串行化官网写操作。原生任务以学校已选列表核验结果，不撤销官网操作；仅暂停依赖关系已无法安全继续的任务。
- 双向同步以会话代次和 cookie 增量处理，旧窗口快照不能覆盖更新会话。官网主动退出会暂停相关任务，必须主动重新登录；不同账户不导入共享会话。
- 官网脚本合成测试：`node --test src-tauri/src/enrollment/official-page.node-test.mjs`。真实账户的官网共享、双端并行操作及最终 APK 原生返回仍须设备验收。

## 会话维护和登录信息

- 独立 Rust 工作线程每 60 秒对已认证会话随机执行学生信息或已选列表只读 POST；准入受限只使用学生信息。单线程、5 秒超时、无积压，不持有选退课操作锁。会话代次、账号、来源或退出状态变化后的旧结果被忽略。保活不提交密码、不启动 OCR。
- Android 无抢课任务时遵循系统调度，不新增前台通知或 WakeLock；抢课时复用现有前台服务。实际会话续期和系统挂起行为需真机与账户验收。
- 登录页批次信息映射学校 `name/typeName/tacticName/beginTime/endTime/active`，仅展示公共信息，不授予准入。在线人数独立 3 秒超时，每 60 秒更新；页面隐藏或离开即停止订阅，失败保留旧值。缓存入口不启动这些请求。
- OCR 目标字裁剪固定为 x=82..132、y=2..14，下方候选采用 Otsu、连通域去噪和投影分割；逐字原图及二值/反色图复识别，只接受唯一的一对一匹配。三次连续结构异常或五次未成功后交给人工。
- 固定合成样本比较：旧实现四点整组正确 0/6、拒识 6/6；新实现正确 3/6、拒识 3/6，未误提交。首次运行六张合计约 5.35 秒（含模型初始化）。这是合成开发样本，不代表真实验证码成功率；详见 `fixtures/captcha/README.md`。

## 验证码点选与自动接管

- 登录页使用同一个验证码组件，图片加载后立即开始逐点识别。每个点位被前端接受并绘制后，才请求下一个目标；第四点无论来自自动还是手动都立即提交。
- 点击图片撤销自动运行，保留已有点位并追加本次手动点位。“自动识别”从下一个目标继续；“重选”和“换一张”停止自动流程并清空点位。提交期间控件禁用。
- `captchaPrepare/captchaPoint/captchaCancel/captchaSubmit` 通过挑战 ID、运行代次和已接受点位绑定；取消不等待 OCR 或选课操作锁。迟到结果丢弃，挑战提交即消费，账号或访问源变化后不能提交旧挑战。
- 前端只接收图片、点位和不透明 ID。挑战令牌、cookies 与密码保留在原生层；离开页面释放挑战图片，已发出的合法登录请求仍按账号和会话代次核验并保存结果。
- 自动重试最多五次、连续三次结构异常转人工。手动验证码拒绝后换图并保持手动模式；网络或密码错误不自动重复提交。
