use super::*;

#[test]
fn matches_portal_javascript_reference_vectors_including_special_and_utf16_characters() {
    let config = StaffPortalConfig {
        ip: "172.28.0.8".into(),
        acid: "2".into(),
        mac_auth: true,
    };
    // Generated independently with the reviewed JS charCodeAt/mixing algorithm and Node HMAC/SHA1.
    let vectors = [
    ("Test&+%\"\\pass", "{SRBX1}vzvhOejhZJo3FweWDq5EDsuEl+P7wqFYFZdMkBim9X6whPzj8S0iFHskG1DfAxI7yZ8vcwtWQHgV5V0oCpDV81DLsZXEkp4JHpljcS13MGQPaFcHiJdIjhJlbIAfqDNsMi20rnjb7f/=", "f8a2ad443a95991f3dc5f5ea334eaeee", "afb61a1873101fc92eb183be58d27a295cdc4171"),
    ("päss", "{SRBX1}XnDuu6SuMlGfW7lIurL0cOvPmx8j9lSM8zczT6bHdYZQ6yACwFGdjX8N1zq0aqr1nc3QK/snrHwUJrR1gEFaQOXmpg7aJgJDwtW557jxOylBt4uuXLNbugG0JI24J+0Y", "c661ebcdc280d6ed6334c7635822283e", "6a47925ebc10cf3fa89a957af95f2e7ae1e1dac9"),
    ("密🙂", "{SRBX1}f61bGCy2D5c8lDbui6/m2DZjXAnnEUhR+tPbwYTLngclvHDo6z29p/IVKw++4p2Hj8fCOJP7Yfph8J3r0fnW59RAvIDsVo92HuolLACrbfHckGb3O4x9Jz2NSG4=", "5aaddd4000d68efe676c15b50a6658b1", "8fd623f227dfbc809856a0cb0d4eef476be952b7"),
    ];
    for (password, info, hmd5, checksum) in vectors {
        assert_eq!(
            encode_info(
                "001234",
                password,
                &config,
                "0123456789abcdef0123456789abcdef"
            ),
            info
        );
        let (url, _) = login_url(
            "001234",
            password,
            &config,
            "",
            "0123456789abcdef0123456789abcdef",
            42,
        );
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(params["password"], format!("{{MD5}}{hmd5}"));
        assert_eq!(params["chksum"], checksum);
        assert_eq!(params["info"], info);
        assert_eq!(params["username"], "001234");
        assert_eq!(params["ac_id"], "2");
        assert_eq!(params["double_stack"], "0");
        assert!(!params.contains_key("mac_auth"));
        assert!(!params.contains_key("remember"));
    }
}
#[test]
fn remember_matches_official_latin1_base64_and_rejects_unsupported_characters_before_login() {
    let encoded = remember_value("001234", "päss&+%\"").unwrap();
    let decoded = general_purpose::STANDARD.decode(encoded).unwrap();
    let text: String = decoded.into_iter().map(char::from).collect();
    assert_eq!(text, r#"{"username":"001234","password":"päss&+%\""}"#);
    assert!(remember_value("001234", "密🙂").is_err());
}
