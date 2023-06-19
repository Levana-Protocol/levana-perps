mod app;
mod cli;
mod db;
mod endpoints;

use anyhow::Result;
use app::App;
use clap::Parser;
use cli::Opt;

#[tokio::main]
async fn main() -> Result<()> {
    main_inner().await
}

async fn main_inner() -> Result<()> {
    let opt = Opt::parse();
    opt.init_logger();
    let app = App::new(opt).await?;
    endpoints::launch(app).await
}
