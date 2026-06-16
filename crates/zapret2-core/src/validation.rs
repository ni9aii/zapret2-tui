//! nfqws2 option validation — shared between the daemon manager and the TUI
//! form layer without either importing the other.

use crate::{Result, ZapretError};

/// Flags that nfqws2 accepts. Values paired with `--flag=value` or as
/// separate positional tokens are always allowed; only flags are checked.
const ALLOWED_NFQWS_OPTS: &[&str] = &[
    "--qnum",
    "--desync",
    "--hostlist",
    "--split",
    "--wss",
    "--dpi-desync",
    "--dpi-desync-fw-external",
    "--dpi-desync-ttl",
    "--encrypt",
    "--md5",
    "--server",
    "--port",
    "--proxy",
    "--proxy-host",
    "--proxy-port",
];

fn validate_arg(arg: &str) -> bool {
    if arg.starts_with('-') {
        let flag = arg.split('=').next().unwrap_or(arg);
        ALLOWED_NFQWS_OPTS.contains(&flag)
    } else {
        true
    }
}

/// Parse an nfqws2 option string and validate every flag against the
/// allowlist. Returns the shell-split arguments on success.
pub fn validate_opts(opts: &str) -> Result<Vec<String>> {
    let args = shell_words::split(opts)
        .map_err(|e| ZapretError::ConfigError(format!("failed to parse NFQWS2_OPT: {e}")))?;
    for arg in &args {
        if !validate_arg(arg) {
            return Err(ZapretError::ConfigError(format!(
                "forbidden argument in NFQWS2_OPT: {arg}"
            )));
        }
    }
    Ok(args)
}
