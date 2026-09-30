use super::model::LoginFormPrefill;

pub(super) fn auth_probe_script() -> &'static str {
    r#"(()=>{window.__SZU_AUTH_PROBE__=null;(async()=>{try{const r=await fetch('/personalInfo/common/getUserConf',{credentials:'include'});const t=await r.text();window.__SZU_AUTH_PROBE__={ok:r.ok,status:r.status,url:r.url,text:t}}catch(e){window.__SZU_AUTH_PROBE__={ok:false,error:String(e)}}})();return null})()"#
}

pub(super) fn auth_probe_result_script() -> &'static str {
    "(()=>{const value=window.__SZU_AUTH_PROBE__||null;window.__SZU_AUTH_PROBE__=null;return value})()"
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
