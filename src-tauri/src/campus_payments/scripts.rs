use super::state::Session;
use serde::Serialize;

const PAGE_SCRIPT: &str = include_str!("page.js");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScriptOptions<'a> {
    operation: &'a str,
    generation: String,
    new_document_token: String,
    return_url: &'a str,
    message: &'a str,
    manual: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<&'a str>,
}

pub(super) fn page_script(
    session: &Session,
    operation: &str,
    document_token: Option<&str>,
    credentials: Option<(&str, &str)>,
    expected_url: Option<&str>,
) -> String {
    debug_assert!(credentials.is_none() || operation == "submit");
    debug_assert!(operation != "probe" || expected_url.is_some());
    let options = ScriptOptions {
        operation,
        generation: session.generation.to_string(),
        new_document_token: uuid::Uuid::new_v4().to_string(),
        return_url: session.return_url.as_str(),
        message: session.message(),
        manual: session.is_manual(),
        expected_url,
        document_token,
        account: credentials.map(|(account, _)| account),
        password: credentials.map(|(_, password)| password),
    };
    let source = PAGE_SCRIPT.replacen("export default ", "", 1);
    // JSON serialization, never interpolation of credential text into JS code.
    format!(
        "({source})({})",
        serde_json::to_string(&options).expect("script options serialize")
    )
}

pub(super) fn cancel_script(generation: u64) -> String {
    format!(
        "(() => {{ const flow = window.__szuNetworkFeeFlow; if (flow?.generation === {:?}) {{ flow.cancelled = true; flow.manual = true; flow.dispose?.(); }} }})()",
        generation.to_string(),
    )
}
