mod app;
mod config;
mod net;
mod platform;
mod ui;

use anyhow::Result;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cfg = config::Config::load()?;
    app::runner::run_app(cfg.zhihu.cookie).await
}
