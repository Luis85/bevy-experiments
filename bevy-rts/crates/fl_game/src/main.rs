//! The `fl_game` testbed binary.

use std::path::PathBuf;

use bevy::app::AppExit;
use clap::Parser;
use fl_game::GameBuilder;

/// The default port of the remote debug endpoint (BRP).
const DEFAULT_REMOTE_PORT: u16 = 15702;

/// The environment variable that replaces [`DEFAULT_REMOTE_PORT`] when `--remote-port` is absent.
const PORT_ENV: &str = "BRP_EXTRAS_PORT";

/// The remote debug port: the `--remote-port` flag, else `env` (the value of [`PORT_ENV`]), else
/// [`DEFAULT_REMOTE_PORT`]. An unparsable `env` is reported and ignored.
fn remote_port(flag: Option<u16>, env: Option<&str>) -> u16 {
    if let Some(port) = flag {
        return port;
    }
    match env.map(str::trim).filter(|v| !v.is_empty()) {
        None => DEFAULT_REMOTE_PORT,
        Some(text) => text.parse().unwrap_or_else(|_| {
            eprintln!("fl_game: ignoring {PORT_ENV}={text:?}: not a port number");
            DEFAULT_REMOTE_PORT
        }),
    }
}

#[derive(Parser, Debug)]
#[command(name = "fl_game", about = "Frontline testbed")]
struct Cli {
    /// The scenario to start.
    #[arg(long, default_value = "base.testbed", conflicts_with = "stress")]
    scenario: String,
    /// Start the stress scenario (the scale entity mix, scaled to this many wandering units)
    /// instead.
    #[arg(long, value_name = "N")]
    stress: Option<u32>,
    /// Exit after this many seconds and print the FPS line.
    #[arg(long, value_name = "SECS")]
    exit_after: Option<f32>,
    /// Mod directories, in load order (default: the base mod).
    #[arg(long, value_name = "DIR", num_args = 1..)]
    mods: Vec<PathBuf>,
    /// Serve the remote debug endpoint.
    #[arg(long)]
    remote: bool,
    /// The remote debug endpoint's port (default: $BRP_EXTRAS_PORT, else 15702).
    #[arg(long, value_name = "P", requires = "remote")]
    remote_port: Option<u16>,
    /// Keep updating while unfocused (benches and agents).
    #[arg(long)]
    agent: bool,
}

impl Cli {
    fn builder(self) -> GameBuilder {
        let mut builder =
            GameBuilder::new()
                .scenario(&self.scenario)
                .exit_after(self.exit_after)
                .debug_remote(self.remote.then(|| {
                    remote_port(self.remote_port, std::env::var(PORT_ENV).ok().as_deref())
                }))
                .agent_mode(self.agent);
        if let Some(units) = self.stress {
            builder = builder.stress(units);
        }
        for dir in self.mods {
            builder = builder.content_mod(dir);
        }
        builder
    }
}

fn main() -> AppExit {
    Cli::parse().builder().run()
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn brp_extras_port_is_the_default_and_the_flag_wins() {
        assert_eq!(remote_port(None, None), 15702);
        assert_eq!(remote_port(None, Some("15731")), 15731);
        assert_eq!(
            remote_port(Some(15800), Some("15731")),
            15800,
            "the flag wins"
        );
        assert_eq!(remote_port(None, Some("")), 15702);
        assert_eq!(remote_port(None, Some("not a port")), 15702);
        assert_eq!(remote_port(None, Some("70000")), 15702, "out of range");
    }

    #[test]
    fn cli_parses_every_flag() {
        let cli = Cli::try_parse_from([
            "fl_game",
            "--stress",
            "3000",
            "--exit-after",
            "25",
            "--mods",
            "a",
            "b",
            "--remote",
            "--remote-port",
            "15800",
            "--agent",
        ])
        .expect("valid flags");
        assert_eq!(cli.stress, Some(3000));
        assert_eq!(cli.exit_after, Some(25.0));
        assert_eq!(cli.mods, [PathBuf::from("a"), PathBuf::from("b")]);
        assert!(cli.remote && cli.agent);
        assert_eq!(cli.remote_port, Some(15800));

        let cli = Cli::try_parse_from(["fl_game"]).expect("no flags");
        assert_eq!(cli.scenario, "base.testbed");
        assert!(cli.stress.is_none() && cli.mods.is_empty() && !cli.remote);
        assert!(Cli::try_parse_from(["fl_game", "--remote-port", "1"]).is_err());
        assert!(Cli::try_parse_from(["fl_game", "--scenario", "x", "--stress", "5"]).is_err());
    }
}
