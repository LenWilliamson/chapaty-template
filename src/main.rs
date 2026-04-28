use anyhow::{Context, Result};
use chapaty::prelude::*;
use std::path::Path;

use crate::agents::demo::DemoAgent;

mod agents;

#[tokio::main]
async fn main() -> Result<()> {
    println!(">> Loading environment from Hugging Face (first run downloads + caches)...");
    let mut env = environment().await?;

    let ohlcv = ohlcv_id();
    let mut agent = DemoAgent::new(ohlcv, 20, 50);

    println!(">> Running DemoAgent backtest...");
    let journal = env.evaluate_agent(&mut agent)?;

    println!(">> Exporting results...");
    let file_cfg = FileConfig::default().with_dir(Path::new("chapaty/reports"));

    // Export the raw journal
    journal.to_file_sync(&file_cfg)?;

    // Export the statistics
    journal.cumulative_returns()?.to_file_sync(&file_cfg)?;
    journal.portfolio_performance()?.to_file_sync(&file_cfg)?;
    journal.trade_stats()?.to_file_sync(&file_cfg)?;

    // Export the downsampled EOD equity curve for the tearsheet
    env.equity_curve_report()?
        .into_eod()?
        .to_file_sync(&file_cfg)?;

    println!(">> Wrote chapaty/reports/journal.csv (and companion reports).");
    println!(
        ">> Open chapaty/reports/tearsheet.html in a browserto visualize the backtest results."
    );

    Ok(())
}

async fn environment() -> Result<Environment> {
    let preset = EnvPreset::BinanceBtcUsdt1dSma20Sma50;
    let file_stem = preset.to_string();

    // None = automatically pin to the current crate version
    let loc = StorageLocation::HuggingFace { version: None };
    let cfg = IoConfig::new(loc).with_file_stem(&file_stem);

    chapaty::load(preset, &cfg)
        .await
        .context("Failed to load trading environment")
}

fn ohlcv_id() -> OhlcvId {
    OhlcvId {
        broker: DataBroker::Binance,
        exchange: Exchange::Binance,
        symbol: Symbol::Spot(SpotPair::BtcUsdt),
        period: Period::Day(1),
    }
}
