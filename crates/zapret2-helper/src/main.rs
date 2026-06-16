//! zapret2-helper — minimal privileged helper for zapret2-tui.
//!
//! Invoked via `pkexec` so the unprivileged TUI can perform the few operations
//! that require root: applying/removing firewall rules and starting/stopping
//! the nfqws2 daemon. The helper is intentionally small and auditable:
//!
//! - strict argument parsing (clap derive); unknown subcommands are rejected;
//! - it never invokes a shell — all work goes through `zapret2-core`'s audited
//!   [`actions`](zapret2_core::actions) functions;
//! - exactly one privileged operation runs per invocation, then it exits.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use zapret2_core::config::ZapretConfig;
use zapret2_core::{actions, DEFAULT_CONFIG_PATH};

#[derive(Parser)]
#[command(name = "zapret2-helper")]
#[command(version)]
#[command(about = "Privileged helper for zapret2-tui (run via pkexec)", long_about = None)]
struct Cli {
    /// Path to the zapret2 config file.
    #[arg(long, value_name = "FILE", default_value = DEFAULT_CONFIG_PATH, global = true)]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify the local environment (nfqws2 binary present).
    Check,
    /// Manage firewall rules.
    Firewall {
        #[command(subcommand)]
        action: FirewallAction,
    },
    /// Manage the nfqws2 daemon.
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Manage profiles on disk.
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
}

#[derive(Subcommand)]
enum FirewallAction {
    /// Apply firewall redirection rules.
    Apply,
    /// Remove firewall redirection rules.
    Remove,
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the nfqws2 daemon (detached).
    Start {
        /// Profile name whose nfqws2 options to apply before starting.
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,
    },
    /// Stop the nfqws2 daemon.
    Stop,
    /// Report whether the nfqws2 daemon is running (exit 0 = running).
    Status,
}

#[derive(Subcommand)]
enum ProfileAction {
    /// Validate and write a profile to disk.
    Save {
        #[arg(long, value_name = "NAME")]
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "")]
        strategy: String,
        /// nfqws2 options (validated against the whitelist).
        #[arg(
            long,
            value_name = "OPTS",
            default_value = "",
            allow_hyphen_values = true
        )]
        nfqws_opts: String,
        /// Comma- or space-separated hostlist file names.
        #[arg(long, value_name = "LIST", default_value = "")]
        hostlists: String,
    },
    /// Remove a profile from disk.
    Remove {
        #[arg(long, value_name = "NAME")]
        name: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("zapret2-helper: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let mut config = ZapretConfig::load(Some(cli.config))?;

    match cli.command {
        Command::Check => {
            actions::check(&config)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Firewall { action } => {
            match action {
                FirewallAction::Apply => actions::firewall_apply(&config).await?,
                FirewallAction::Remove => actions::firewall_remove(&config).await?,
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Daemon { action } => match action {
            DaemonAction::Start { profile } => {
                if let Some(name) = profile {
                    apply_profile(&mut config, &name)?;
                }
                actions::daemon_start(&config).await?;
                Ok(ExitCode::SUCCESS)
            }
            DaemonAction::Stop => {
                actions::daemon_stop(&config).await?;
                Ok(ExitCode::SUCCESS)
            }
            DaemonAction::Status => {
                if actions::daemon_status(&config) {
                    Ok(ExitCode::SUCCESS)
                } else {
                    // Distinct non-error "not running" signal.
                    Ok(ExitCode::from(3))
                }
            }
        },
        Command::Profile { action } => match action {
            ProfileAction::Save {
                name,
                description,
                strategy,
                nfqws_opts,
                hostlists,
            } => {
                let profile = zapret2_core::profile::Profile {
                    name,
                    description,
                    strategy,
                    hostlists: parse_hostlists(&hostlists),
                    nfqws_opts,
                };
                actions::profile_save(&config, &profile)?;
                Ok(ExitCode::SUCCESS)
            }
            ProfileAction::Remove { name } => {
                actions::profile_remove(&config, &name)?;
                Ok(ExitCode::SUCCESS)
            }
        },
    }
}

/// Split a comma/whitespace-separated hostlists string into a clean list.
fn parse_hostlists(raw: &str) -> Vec<String> {
    raw.split([',', ' ', '\t', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Load the named profile and patch the config's nfqws2 options with it.
fn apply_profile(config: &mut ZapretConfig, name: &str) -> anyhow::Result<()> {
    use zapret2_core::profile::ProfileManager;

    ProfileManager::validate_name(name)?;
    let mut manager = ProfileManager::new(config.zapret_base.join("profiles"));
    manager.load()?;
    let profile = manager
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("profile not found: {name}"))?;
    config.nfqws2_opt = profile.nfqws_opts.clone();
    config.current_profile = Some(name.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args)
    }

    #[test]
    fn parses_known_subcommands() {
        assert!(parse(&["zapret2-helper", "check"]).is_ok());
        assert!(parse(&["zapret2-helper", "firewall", "apply"]).is_ok());
        assert!(parse(&["zapret2-helper", "firewall", "remove"]).is_ok());
        assert!(parse(&["zapret2-helper", "daemon", "start"]).is_ok());
        assert!(parse(&["zapret2-helper", "daemon", "stop"]).is_ok());
        assert!(parse(&["zapret2-helper", "daemon", "status"]).is_ok());
    }

    #[test]
    fn rejects_unknown_subcommands() {
        assert!(parse(&["zapret2-helper"]).is_err()); // missing subcommand
        assert!(parse(&["zapret2-helper", "bogus"]).is_err());
        assert!(parse(&["zapret2-helper", "firewall", "nuke"]).is_err());
        assert!(parse(&["zapret2-helper", "daemon", "explode"]).is_err());
        assert!(parse(&["zapret2-helper", "profile", "wipe"]).is_err());
    }

    #[test]
    fn profile_save_requires_name_and_accepts_fields() {
        // Missing --name is an error.
        assert!(parse(&["zapret2-helper", "profile", "save"]).is_err());

        let cli = parse(&[
            "zapret2-helper",
            "profile",
            "save",
            "--name",
            "yt",
            "--nfqws-opts",
            "--qnum=200",
            "--hostlists",
            "a.txt, b.txt",
        ])
        .unwrap();
        match cli.command {
            Command::Profile {
                action:
                    ProfileAction::Save {
                        name,
                        nfqws_opts,
                        hostlists,
                        ..
                    },
            } => {
                assert_eq!(name, "yt");
                assert_eq!(nfqws_opts, "--qnum=200");
                assert_eq!(hostlists, "a.txt, b.txt");
            }
            _ => panic!("expected profile save"),
        }
    }

    #[test]
    fn profile_remove_requires_name() {
        assert!(parse(&["zapret2-helper", "profile", "remove"]).is_err());
        assert!(parse(&["zapret2-helper", "profile", "remove", "--name", "yt"]).is_ok());
    }

    #[test]
    fn parse_hostlists_splits_and_trims() {
        assert_eq!(
            parse_hostlists("  a.txt , b.txt   c.txt,,"),
            vec!["a.txt", "b.txt", "c.txt"]
        );
        assert!(parse_hostlists("").is_empty());
    }

    #[test]
    fn daemon_start_accepts_profile_and_defaults_to_none() {
        let cli = parse(&["zapret2-helper", "daemon", "start"]).unwrap();
        match cli.command {
            Command::Daemon {
                action: DaemonAction::Start { profile },
            } => assert_eq!(profile, None),
            _ => panic!("expected daemon start"),
        }

        let cli = parse(&["zapret2-helper", "daemon", "start", "--profile", "yt"]).unwrap();
        match cli.command {
            Command::Daemon {
                action: DaemonAction::Start { profile },
            } => assert_eq!(profile.as_deref(), Some("yt")),
            _ => panic!("expected daemon start"),
        }
    }

    #[test]
    fn config_defaults_and_overrides() {
        let cli = parse(&["zapret2-helper", "check"]).unwrap();
        assert_eq!(cli.config, PathBuf::from(DEFAULT_CONFIG_PATH));

        let cli = parse(&["zapret2-helper", "--config", "/tmp/c", "check"]).unwrap();
        assert_eq!(cli.config, PathBuf::from("/tmp/c"));
    }
}
