use amazon_price_scraper_lib::web;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = web::load_config()?;
    web::serve(config).await
}
