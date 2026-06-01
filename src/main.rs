mod platform;
mod config;
mod net;

fn main() -> anyhow::Result<()> {
    let signer = platform::zhihu::sign::ZhihuSigner::new()?;
    let demo = "101_3_3.0+/api/v3/feed/topstory/hot-lists/total?limit=50&desktop=true+DEMO_DC0";
    println!("x-zse-96 = {}", signer.sign(demo)?);
    Ok(())
}
