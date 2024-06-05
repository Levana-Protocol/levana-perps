#![deny(clippy::as_conversions)]

mod cli;

use crate::cli::Opt;
use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let opt = Opt::parse();
    opt.init_logger();
    println!("{:?}", opt);
    Ok(())
}
