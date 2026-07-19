//! Integration tests that drive a *real* zapret2 install.
//!
//! These are gated behind the `integration` feature and require:
//!   * root (the tests run `nft` and spawn `nfqws2`),
//!   * a built `nfqws2` at `$ZAPRET_BASE/nfq2/nfqws2`
//!     (e.g. `cd $ZAPRET_BASE/nfq2 && make`),
//!   * the `nft` binary on `PATH`.
//!
//! Run from CI with `--features integration` on a privileged runner. Locally:
//!
//! ```bash
//! sudo env ZAPRET_BASE=/opt/zapret2 cargo test --features integration \
//!     --package zapret2-core --test integration_zapret
//! ```
//!
//! The test writes its own config file (pointing at the real `$ZAPRET_BASE`)
//! so it does not depend on any pre-existing `/opt/zapret2/config`.

#![cfg(feature = "integration")]

use std::path::PathBuf;
use zapret2_core::privilege::PrivilegeMode;
use zapret2_core::ZapretController;

/// Default zapret2 install prefix used by the upstream build.
const DEFAULT_ZAPRET_BASE: &str = "/opt/zapret2";

/// Drops firewall state and the pid file on scope exit, so a panicking test
/// never leaves `inet zapret2` rules or a stale pid file behind.
struct FirewallCleanup;

impl Drop for FirewallCleanup {
    fn drop(&mut self) {
        let _ = std::process::Command::new("nft")
            .args(["delete", "table", "inet", "zapret2"])
            .status();
        let _ = std::fs::remove_file("/var/run/nfqws2.pid");
    }
}

fn nfqws2_bin(base: &str) -> PathBuf {
    PathBuf::from(base).join("nfq2").join("nfqws2")
}

/// Bail out (pass) when the environment cannot run a real zapret2.
fn ensure_real_zapret2(base: &str) {
    let bin = nfqws2_bin(base);
    assert!(
        bin.exists(),
        "integration test needs a built nfqws2 at {}; build it with \
         `cd {} && make` (see CI `integration` job)",
        bin.display(),
        PathBuf::from(base).join("nfq2").display(),
    );
    // nft is required to apply firewall rules.
    assert!(
        which::which("nft").is_ok(),
        "integration test needs the `nft` binary on PATH",
    );
    // These tests mutate firewall state and must run as root.
    assert_eq!(
        unsafe { libc::getuid() },
        0,
        "integration test must run as root to apply nftables rules",
    );
}

fn write_test_config(tmp: &std::path::Path, base: &str) -> PathBuf {
    // Options deliberately avoid `--hostlist=...` so the test does not depend
    // on any hostlist files shipped with zapret2.
    let cfg = format!(
        "ZAPRET_BASE={base}\n\
         NFQWS2_ENABLE=1\n\
         NFQWS2_OPT=\"--qnum=200 --dpi-desync=disorder --dpi-desync-ttl=1\"\n\
         QNUM=200\n\
         FWTYPE=nftables\n\
         DESYNC_MARK=0x40000000\n"
    );
    let path = tmp.join("zapret2-config");
    std::fs::write(&path, cfg).expect("write test config");
    path
}

async fn run_cycle(controller: &mut ZapretController) -> zapret2_core::Result<()> {
    // Start: applies nftables rules + spawns nfqws2.
    controller.start().await?;

    let status = controller.status().await;
    assert!(
        status.daemon_running,
        "nfqws2 should be running after start"
    );
    assert!(
        status.firewall_active,
        "nftables 'zapret2' table should exist after start",
    );

    // Stop: removes rules + kills nfqws2.
    controller.stop().await?;

    let status = controller.status().await;
    assert!(
        !status.daemon_running,
        "nfqws2 should not be running after stop",
    );
    assert!(
        !status.firewall_active,
        "nftables 'zapret2' table should be gone after stop",
    );

    Ok(())
}

#[tokio::test]
async fn real_zapret2_start_stop_cycle() {
    let base = std::env::var("ZAPRET_BASE").unwrap_or_else(|_| DEFAULT_ZAPRET_BASE.to_string());
    ensure_real_zapret2(&base);

    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg_path = write_test_config(tmp.path(), &base);

    let mut controller = ZapretController::new(Some(cfg_path)).expect("controller");
    controller.set_privilege_mode(PrivilegeMode::Direct);

    // Cleanup always runs, even on assertion/panic inside run_cycle.
    let _cleanup = FirewallCleanup;

    run_cycle(&mut controller).await.expect("start/stop cycle");
}
