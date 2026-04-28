use anyhow::{Context, Result};
use chapaty::prelude::*;
use rayon::iter::ParallelBridge;
use std::path::Path;

use crate::agents::demo::{DemoAgent, DemoAgentGrid};

mod agents;

#[tokio::main]
async fn main() -> Result<()> {
    println!(">> Loading environment from Hugging Face...");
    let mut env = environment().await?;

    let ohlcv = ohlcv_id();
    let file_cfg = FileConfig::default().with_dir(Path::new("chapaty/reports"));

    // ==========================================
    // 1. Single Agent Baseline (For Tearsheet)
    // ==========================================
    println!(">> Running DemoAgent baseline backtest...");
    let mut baseline_agent = DemoAgent::new(ohlcv, 20, 50);
    let journal = env.evaluate_agent(&mut baseline_agent)?;

    journal.to_file_sync(&file_cfg)?;
    journal.cumulative_returns()?.to_file_sync(&file_cfg)?;
    journal.portfolio_performance()?.to_file_sync(&file_cfg)?;
    journal.trade_stats()?.to_file_sync(&file_cfg)?;

    // Export the downsampled EOD equity curve for the tearsheet
    env.equity_curve_report()?
        .into_eod()?
        .to_file_sync(&file_cfg)?;

    println!(">> DemoAgent baseline backtest complete.");

    // ==========================================
    // 2. Parallel Grid Search (Parameter Sweep)
    // ==========================================
    println!(">> Building DemoAgent Grid...");
    let (count, agents) = DemoAgentGrid::baseline(ohlcv)?.build();

    println!(">> Evaluating {count} agents in parallel...");
    // Calling .into_par_iter() directly on the agents Vec an cause rayon to stall for large Vecs. 
    // Prefere using agents.into_iter().par_bridge(). It is safe and efficient for Vecs.
    let leaderboard = env.evaluate_agents(agents.into_iter().par_bridge(), 100, count as u64)?;

    leaderboard.to_file_sync(&file_cfg)?;
    println!(">> Grid complete. Leaderboard saved.");

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
