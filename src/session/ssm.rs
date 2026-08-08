//! The internal `smew ssm-session` / `smew ssh-proxy` runners: call
//! ssm:StartSession through the SDK, then exec session-manager-plugin with
//! the same six positional arguments the aws CLI would hand it. This is what
//! lets smew drive sessions without the aws CLI installed — the plugin is
//! the only external binary left.
//!
//! Every function here returns only on failure (the happy path replaces the
//! process with the plugin); the returned String is the error message.

use aws_config::SdkConfig;

use crate::aws;

const PLUGIN: &str = "session-manager-plugin";

/// What to tell the user when the plugin binary is missing.
pub const PLUGIN_INSTALL_HINT: &str = "session-manager-plugin not found — install it:\n  \
    macOS:  brew install --cask session-manager-plugin\n  \
    other:  https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html";

fn aws_err(e: &(impl std::error::Error + 'static)) -> String {
    format!("{}", aws_sdk_ssm::error::DisplayErrorContext(e))
}

/// Splits a `key=value` --param argument.
pub fn parse_param(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .ok_or_else(|| format!("--param wants key=value, got {s:?}"))
}

/// The JSON the plugin receives as its 5th argument: the StartSession
/// request that produced the session (it reads Target and the parameters
/// from it).
fn request_json(target: &str, document: Option<&str>, params: &[(String, String)]) -> String {
    let mut req = serde_json::Map::new();
    req.insert("Target".into(), serde_json::json!(target));
    if let Some(d) = document {
        req.insert("DocumentName".into(), serde_json::json!(d));
    }
    if !params.is_empty() {
        let mut p = serde_json::Map::new();
        for (k, v) in params {
            p.insert(k.clone(), serde_json::json!([v]));
        }
        req.insert("Parameters".into(), serde_json::Value::Object(p));
    }
    serde_json::Value::Object(req).to_string()
}

/// Calls ssm:StartSession and execs the plugin. `profile`/`region` may be
/// empty (SDK default resolution). Returns the error message on failure.
pub async fn exec_session(
    profile: &str,
    region: &str,
    target: &str,
    document: Option<&str>,
    params: &[(String, String)],
) -> String {
    let cfg = match aws::load(profile, region).await {
        Ok(c) => c,
        Err(e) => return e,
    };
    start_and_exec(&cfg, profile, target, document, params).await
}

async fn start_and_exec(
    cfg: &SdkConfig,
    profile: &str,
    target: &str,
    document: Option<&str>,
    params: &[(String, String)],
) -> String {
    let region = aws::region_of(cfg);
    if region.is_empty() {
        return "no AWS region resolved — pass --region or set one on the profile".to_string();
    }
    let ssm = aws_sdk_ssm::Client::new(cfg);
    let mut req = ssm.start_session().target(target);
    if let Some(d) = document {
        req = req.document_name(d);
    }
    for (k, v) in params {
        req = req.parameters(k, vec![v.clone()]);
    }
    let out = match req.send().await {
        Ok(o) => o,
        Err(e) => {
            let msg = aws_err(&e);
            return if aws::is_sso_token_error(&msg) {
                format!("{msg}\n{}", aws::sso_login_hint(profile))
            } else {
                format!("ssm:StartSession failed: {msg}")
            };
        }
    };
    let (Some(sid), Some(token), Some(stream)) =
        (out.session_id(), out.token_value(), out.stream_url())
    else {
        return "ssm:StartSession returned an incomplete response".to_string();
    };
    let session_json = serde_json::json!({
        "SessionId": sid,
        "TokenValue": token,
        "StreamUrl": stream,
    })
    .to_string();

    // The six positional arguments the aws CLI passes the plugin:
    // response JSON · region · operation · profile · request JSON · endpoint.
    exec_plugin(vec![
        session_json,
        region.clone(),
        "StartSession".to_string(),
        profile.to_string(),
        request_json(target, document, params),
        format!("https://ssm.{region}.amazonaws.com"),
    ])
}

/// The ssh ProxyCommand runner: optionally pushes a 60-second public key via
/// EC2 Instance Connect (ephemeral mode), then streams SSH over SSM on
/// stdio. Only stderr may be written to — stdout carries the SSH stream.
pub async fn exec_ssh_proxy(
    profile: &str,
    region: &str,
    target: &str,
    port: u16,
    user: &str,
    public_key: Option<&str>,
) -> String {
    let cfg = match aws::load(profile, region).await {
        Ok(c) => c,
        Err(e) => return e,
    };
    if let Some(path) = public_key {
        let key = match std::fs::read_to_string(path) {
            Ok(k) => k,
            Err(e) => return format!("cannot read public key {path}: {e}"),
        };
        let eic = aws_sdk_ec2instanceconnect::Client::new(&cfg);
        if let Err(e) = eic
            .send_ssh_public_key()
            .instance_id(target)
            .instance_os_user(user)
            .ssh_public_key(key.trim())
            .send()
            .await
        {
            return format!(
                "ec2-instance-connect:SendSSHPublicKey failed: {}",
                aws_sdk_ssm::error::DisplayErrorContext(&e)
            );
        }
    }
    start_and_exec(
        &cfg,
        profile,
        target,
        Some("AWS-StartSSHSession"),
        &[("portNumber".to_string(), port.to_string())],
    )
    .await
}

/// Replaces this process with the plugin (unix); returns the error message
/// when the exec itself fails (missing binary, permissions).
fn exec_plugin(args: Vec<String>) -> String {
    let mut cmd = std::process::Command::new(PLUGIN);
    cmd.args(&args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec(); // returns only on failure
        if err.kind() == std::io::ErrorKind::NotFound {
            PLUGIN_INSTALL_HINT.to_string()
        } else {
            format!("failed to run {PLUGIN}: {err}")
        }
    }
    #[cfg(not(unix))]
    {
        match cmd.status() {
            Ok(st) => std::process::exit(st.code().unwrap_or(1)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PLUGIN_INSTALL_HINT.to_string(),
            Err(e) => format!("failed to run {PLUGIN}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_json_shapes() {
        assert_eq!(request_json("i-0abc", None, &[]), r#"{"Target":"i-0abc"}"#);
        let j = request_json(
            "i-0abc",
            Some("AWS-StartPortForwardingSession"),
            &[
                ("portNumber".into(), "80".into()),
                ("localPortNumber".into(), "8080".into()),
            ],
        );
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["Target"], "i-0abc");
        assert_eq!(v["DocumentName"], "AWS-StartPortForwardingSession");
        assert_eq!(v["Parameters"]["portNumber"][0], "80");
        assert_eq!(v["Parameters"]["localPortNumber"][0], "8080");
    }

    #[test]
    fn params_parse() {
        assert_eq!(
            parse_param("portNumber=5432").unwrap(),
            ("portNumber".to_string(), "5432".to_string())
        );
        assert!(parse_param("nope").is_err());
        // values may contain '=' (split at the first one only)
        assert_eq!(
            parse_param("k=a=b").unwrap(),
            ("k".to_string(), "a=b".to_string())
        );
    }
}
