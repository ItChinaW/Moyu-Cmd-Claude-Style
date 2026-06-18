use anyhow::Result;

#[path = "../platform/yahoo_ws.rs"]
mod yahoo_ws;

#[tokio::main]
async fn main() -> Result<()> {
    let symbols = std::env::args().skip(1).collect::<Vec<_>>();
    if symbols.is_empty() {
        anyhow::bail!("usage: cargo run --bin yahoo_probe -- SPCX NVDA QQQ");
    }
    let target_len = symbols.len();
    let mut seen = 0usize;
    yahoo_ws::subscribe_forever(&symbols, move |quote| {
        if seen < target_len {
            println!("{}", serde_json::to_string_pretty(&quote).unwrap());
            seen += 1;
        }
    }).await?;
    Ok(())
}
