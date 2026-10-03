use super::model::LoginFormPrefill;

pub(super) fn auth_probe_script(
    generation: u64,
    budget: std::time::Duration,
    operation: &str,
) -> String {
    let source = include_str!("probe-page.js").replacen("export default ", "", 1);
    let options = serde_json::json!({ "generation": generation.to_string(), "budgetMs": budget.as_millis() as u64, "operation": operation });
    format!("({source})({options})")
}

pub(super) fn auth_login_capture_script(
    allow_qr_login: bool,
    prefill: Option<&LoginFormPrefill>,
    generation: u64,
    document: u64,
    automatic: bool,
) -> String {
    let source = include_str!("page.js").replacen("export default ", "", 1);
    let options = serde_json::json!({
        "allowQrLogin": allow_qr_login,
        "prefill": prefill,
        "generation": generation.to_string(),
        "documentId": document.to_string(),
        "automatic": automatic,
    });
    format!("({source})({options})")
}
