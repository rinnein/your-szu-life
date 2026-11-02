/// 前端进入/离开功能页面时上报，后端轮询据此选择第一档频率。
///
/// 按 WebView label 做引用计数：多个窗口同时打开同一模块时，关闭其中一个
/// 不会把模块错误地降回前台档。
#[tauri::command]
pub(crate) fn set_active_module(
    window: tauri::WebviewWindow,
    module: String,
    active: bool,
) {
    super::set_module_active(&module, window.label(), active);
}
