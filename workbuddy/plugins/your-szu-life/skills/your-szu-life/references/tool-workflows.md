# MCP 工具工作流

工具名以当前版本为准；先 `szu_life_list_tools` 或 `szu-life mcp-list`，下面的清单用于快速定位模块。所有写操作先向用户确认。

## 课表与待办

- 课表快照：`timetable_get_snapshot`
- 首页/全部待办：`todos_list`；提醒设置：`todos_get_settings` / `todos_set_settings`
- 打开页面：`open_page`，例如 `your-szu-life://page/study/timetable`、`your-szu-life://page/`

## 成绩与教务

- 成绩：`enrollment_get_grades`
- 学业完成情况：`enrollment_get_academic_progress`
- 成绩明细/已选课程：`enrollment_get_academic_record`、`enrollment_get_selected_courses`
- 选课快照：`enrollment_get_snapshot`；搜课：`enrollment_search_courses`

## 教学评价

- 待评清单：`evaluation_list_pending`（缓存 30 分钟）
- 一键评教：`evaluation_run`（默认 `dryRun=true` 只预览；`dryRun=false` 会真实提交且不可撤销，必须先向用户确认）
- 档位为每题选项分值区间的次高档（4 档时即良好/比较清楚/比较符合/比较满意）；开放题默认「无」

## 宿舍电费

- 余额/度数：`electricity_query_balance`
- 设置与宿舍位置：`electricity_get_settings`；选项：`electricity_get_options`
- 充值记录：`electricity_get_recharges`
- 未配置宿舍时，先引导用户在「生活 → 校园卡与缴费 → 电费」完成宿舍选择。

## 校园网

- 状态：`network_get_overview`
- 宿舍区登录：`network_trigger_dorm_login`
- 教工区登录：`network_apply_staff_login`
- 注销：`network_logout`（先确认）
- 代理：`netproxy_get_settings` / `netproxy_set_settings` / `netproxy_test_proxy`（修改前确认）

## UOOC

- 课程与进度：`uooc_get_courses`
- 刷新进度：`uooc_refresh_progress`
- 签到：`uooc_checkin`（先确认）
- 作业/考试列表：`uooc_list_quizzes_and_exams`
- 题库检索：`uooc_search_qa_bank`
- AI 解题：`uooc_solve_question_with_ai`（先确认；涉及提交时再确认一次）

UOOC 未登录时，`ensure --need uooc` 会打开 `/study/uooc`，让用户点「登录 UOOC」完成官方登录。

## OJ

- 快照/比赛：`oj_get_snapshot`、`oj_get_contests_and_groups`
- 题面：`oj_get_paper`；提交：`oj_submit_code`（先确认）
- 记录/结果：`oj_get_records`、`oj_get_result`
- 助教工作台：`oj_get_managed_contests`、`oj_get_manage_contest`、`oj_get_review_list`、`oj_get_review_detail`、`oj_save_review`、`oj_save_review_comment`、`oj_get_problem_submissions`、`oj_get_ta_submission`

## 畅课 / 签到 / 巡课

- 畅课待办/通知：`lms_get_todos`、`lms_get_notifications`
- 签到：`signin_get_timetable`、`signin_list_pending`、`signin_submit_number`、`signin_submit_radar`；自动化设置：`signin_get_automation_settings` / `signin_set_automation_settings`
- 签到房间分享：`signin_share_number`
- 巡课：`jxxk_list_semesters`、`jxxk_list_courses`、`jxxk_list_lessons`、`jxxk_get_lesson_streams`、`jxxk_get_play_urls`、`jxxk_get_live_stream`、`jxxk_get_download_urls`、`jxxk_list_downloads`、`jxxk_pause_download`、`jxxk_download_resume`、`jxxk_remove_download`

## 设置、同步与设备

- 快照：`settings_get_snapshot`；储存：`settings_get_storage_status`
- 同步：`sync_get_status`、`sync_list_items`
- 自组网：`mesh_get_status`、`mesh_list_devices`、`mesh_get_campus_access`
- 文件目录：`file_manager_get_state`
- 通知：`notification_get_settings` / `notification_set_settings`

## 认证与状态

- 统一认证：`auth_get_status`、`auth_get_profile`、`auth_refresh_session`
- 初始化：`onboarding_get_status`
- 应用更新：`app_check_update`、`app_get_announcements`、`app_set_update_channel`
- 外观：`appearance_get_theme`、`appearance_set_theme`
- OCR/向量：`ocr_recognize_image`、`ocr_get_status`、`embedding_get_vector`、`embedding_get_status`

## 资源快照

- `your-szu-life://data/onboarding/status`
- `your-szu-life://data/auth/profile`
- `your-szu-life://data/uooc/courses`（完整 UOOC 快照，含 session/user）
- `your-szu-life://data/todos`、`your-szu-life://data/enrollment/timetable`、`your-szu-life://data/enrollment/grades`
- `your-szu-life://data/evaluation/pending`（学生评教待评清单）
- `your-szu-life://data/network/status`、`your-szu-life://data/electricity/status`
- `your-szu-life://data/mesh/status`、`your-szu-life://data/sync/status`
- `your-szu-life://data/app/integration`（新版本）

读取资源用 `szu_life_read_resource` 或原生 resources 接口。
