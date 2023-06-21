mod app;
mod cli;
mod db;
mod endpoints;
mod types;

use anyhow::Result;
use app::App;
use clap::Parser;
use cli::Opt;

#[tokio::main]
async fn main() -> Result<()> {
    main_inner().await
}

async fn main_inner() -> Result<()> {
    dotenv::dotenv().ok();
    let opt = Opt::parse();
    opt.init_logger();
    let app = App::new(opt).await?;
    app.migrate_db().await?;
    endpoints::launch(app).await
}
