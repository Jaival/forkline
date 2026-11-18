//! Token lookup (env, OS credential store, GitHub CLI) and the OAuth device
//! flow.

use fl_types::{AuthPoll, DeviceCode};

use crate::github::Github;
use crate::{Error, Result};

/// Build-time OAuth app client ID. `None` until the app is registered.
pub const CLIENT_ID: Option<&str> = option_env!("FORKLINE_GITHUB_CLIENT_ID");
/// Dev override, read at run time.
pub const ENV_TOKEN: &str = "FORKLINE_GITHUB_TOKEN";
/// Scopes: private repos, org membership (team review requests), inbox.
pub const SCOPES: &str = "repo read:org notifications";

const SERVICE: &str = "dev.forkline.app";
const USER: &str = "github";

/// The signed-in user's GitHub token, if any: `FORKLINE_GITHUB_TOKEN` (dev),
/// then a token the user saved (Windows Credential Manager), then the GitHub
/// CLI's token. Never log the result.
pub fn token() -> Option<String> {
    env_token().or_else(stored_token).or_else(gh_token)
}

pub fn env_token() -> Option<String> {
    std::env::var(ENV_TOKEN).ok().filter(|t| !t.is_empty())
}

/// Token from Windows Credential Manager. Store errors read as "no token".
pub fn stored_token() -> Option<String> {
    entry().ok()?.get_password().ok().filter(|t| !t.is_empty())
}

/// Token of the GitHub CLI (`gh auth token`), read once per process.
pub fn gh_token() -> Option<String> {
    static GH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    GH.get_or_init(|| {
        let mut cmd = std::process::Command::new("gh");
        cmd.args(["auth", "token", "--hostname", "github.com"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let out = cmd.output().ok().filter(|o| o.status.success())?;
        let token = String::from_utf8(out.stdout).ok()?.trim().to_owned();
        (!token.is_empty()).then_some(token)
    })
    .clone()
}

pub fn store_token(token: &str) -> Result<()> {
    entry()?.set_password(token).map_err(store_err)
}

pub fn delete_token() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(e) => Err(store_err(e)),
    }
}

fn entry() -> Result<keyring_core::Entry> {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        #[cfg(windows)]
        if let Ok(store) = windows_native_keyring_store::Store::new() {
            keyring_core::set_default_store(store);
        }
    });
    keyring_core::Entry::new(SERVICE, USER).map_err(store_err)
}

fn store_err(e: keyring_core::Error) -> Error {
    Error::Invalid(format!("credential store: {e}"))
}

/// A started device flow. `device_code` never leaves Rust.
#[derive(Debug, Clone)]
pub struct Device {
    pub device_code: String,
    pub interval: u32,
    pub expires_at: i64,
}

/// Step 1: ask GitHub for a user code.
pub async fn start(gh: &Github, client_id: &str) -> Result<(DeviceCode, Device)> {
    let v = gh
        .oauth("/login/device/code", &[("client_id", client_id), ("scope", SCOPES)])
        .await?;
    let s = |k: &str| v[k].as_str().map(str::to_owned);
    let n = |k: &str| v[k].as_u64().map(|n| n as u32);
    let (Some(device_code), Some(user_code), Some(verification_uri), Some(expires_in)) =
        (s("device_code"), s("user_code"), s("verification_uri"), n("expires_in"))
    else {
        let msg = v["error_description"].as_str().unwrap_or("unexpected device code response");
        return Err(Error::Api { status: 400, message: msg.to_owned() });
    };
    let interval = n("interval").unwrap_or(5);
    let code = DeviceCode { user_code, verification_uri, expires_in, interval };
    let device = Device { device_code, interval, expires_at: crate::now() + i64::from(expires_in) };
    Ok((code, device))
}

/// Step 2: one poll. On `Done` the token is returned with the poll result.
pub async fn poll(gh: &Github, client_id: &str, device: &mut Device) -> Result<(AuthPoll, Option<String>)> {
    if crate::now() >= device.expires_at {
        return Ok((AuthPoll::Expired, None));
    }
    let v = gh
        .oauth(
            "/login/oauth/access_token",
            &[
                ("client_id", client_id),
                ("device_code", &device.device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )
        .await?;
    if let Some(token) = v["access_token"].as_str().filter(|t| !t.is_empty()) {
        // The login is filled in by the caller after it reads `/user`.
        return Ok((AuthPoll::Done { login: String::new() }, Some(token.to_owned())));
    }
    let poll = match v["error"].as_str().unwrap_or("") {
        "authorization_pending" => AuthPoll::Pending { interval: device.interval },
        "slow_down" => {
            device.interval = v["interval"].as_u64().map_or(device.interval + 5, |n| n as u32);
            AuthPoll::Pending { interval: device.interval }
        }
        "expired_token" => AuthPoll::Expired,
        "access_denied" => AuthPoll::Denied,
        other => {
            let msg = v["error_description"].as_str().unwrap_or(other);
            return Err(Error::Api { status: 400, message: msg.to_owned() });
        }
    };
    Ok((poll, None))
}
