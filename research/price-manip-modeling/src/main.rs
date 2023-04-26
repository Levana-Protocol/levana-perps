use std::path::Path;

use anyhow::Result;
use attack::Attack;
use clap::Parser;
use config::{Config, SlippageRules};
use types::Usd;

mod amm;
mod attack;
mod cli;
mod config;
mod perps;
mod system;
mod types;

fn main() -> Result<()> {
    let cmd = cli::Cmd::parse();
    cmd.init_logger();

    match cmd.subcommand {
        cli::Subcommand::Simulate { results } => simulate(&results)?,
        cli::Subcommand::Single {} => single()?,
    }

    Ok(())
}

fn simulate(results: &Path) -> Result<()> {
    let mut config = Config::default();
    let mut csv = csv::Writer::from_path(results)?;

    for long_size in (1..=10).map(|x| x * 50_000) {
        for buy_size in (1..=10).map(|x| x * 50_000) {
            for leverage in (1..=5).map(|x| x * 6) {
                let attack = Attack::StandardLong {
                    long_size: Usd(long_size.try_into()?),
                    leverage: leverage.try_into()?,
                    buy_size: Usd(buy_size.try_into()?),
                };

                for twap in 0..10 {
                    let twap = (twap * 5) + 1;
                    for house_wins in [false, true] {
                        for arbitrage_rate in [0.0, 100.0, 10000.0] {
                            for slippage in SlippageRules::all() {
                                config.twap_blocks = twap;
                                config.house_wins = house_wins;
                                config.arbitrage_rate = Usd(arbitrage_rate);
                                config.slippage = slippage;

                                log::debug!("Simulating: {config:?} {attack:?}");

                                match attack.simulate(&config) {
                                    Ok(simulate) => csv.serialize(&simulate)?,
                                    Err(e) => log::debug!("Simulation failed: {e:?}"),
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn single() -> Result<()> {
    let mut config = Config::default();
    config.arbitrage_rate = Usd(10_000.0);
    config.twap_blocks = 1;
    config.house_wins = false;

    let attack = Attack::StandardLong {
        long_size: Usd(5000.0),
        leverage: 6.0,
        buy_size: Usd(50000.0),
    };

    let simulation = attack.simulate(&config)?;
    println!("{simulation:?}");

    println!();
    println!("Profit: {}", simulation.attacker_profits.0);
    println!("ROI: {}", simulation.attacker_roi);

    Ok(())
}
