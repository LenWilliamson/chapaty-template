use anyhow::{Context, Result};
use chapaty::prelude::*;
use serde::Serialize;
use std::path::Path;
use strum::{AsRefStr, EnumString};

use crate::agents::{
    demo::{DemoAgent, DemoAgentGrid},
    demo2::{Demo2Agent, Demo2AgentGrid},
};

mod agents;

/// Which agent to run. Change this one line to switch.
const ACTIVE_AGENT: ActiveAgent = ActiveAgent::Demo;

/// Max number of top performers to retain in the leaderboard.
const LEADERBOARD_TOP_K: usize = 100;

/// Root directory for all generated reports.
const REPORTS_ROOT: &str = "chapaty/reports";

/// Available agents. Add a variant + a match arm in `main` to register a new one.
#[derive(Debug, Clone, Copy, AsRefStr, EnumString)]
#[strum(serialize_all = "lowercase")]
enum ActiveAgent {
    Demo,
    Demo2,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!(">> Loading environment from Hugging Face...");
    let mut env = environment().await?;
    let ohlcv = ohlcv_id();

    let reports_dir = Path::new(REPORTS_ROOT).join(ACTIVE_AGENT.as_ref());
    let file_cfg = FileConfig::default().with_dir(&reports_dir);

    match ACTIVE_AGENT {
        ActiveAgent::Demo => run_workflow(
            &mut env,
            &file_cfg,
            DemoAgent::new(ohlcv, 20, 50),
            DemoAgentGrid::baseline(ohlcv)?.build(),
        ),
        ActiveAgent::Demo2 => run_workflow(
            &mut env,
            &file_cfg,
            Demo2Agent::new(ohlcv),
            Demo2AgentGrid::baseline(ohlcv)?.build(),
        ),
    }
}

async fn environment() -> Result<Environment> {
    let preset = EnvPreset::BinanceBtcUsdt1d;
    let file_stem = preset.to_string();

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

/// Runs a baseline backtest followed by a parallel grid search.
///
/// # Workflow
///
/// 1. **Baseline backtest** — evaluates `baseline` and writes:
///    - the trade journal,
///    - cumulative returns,
///    - portfolio performance,
///    - trade statistics,
///    - end-of-day equity curve.
/// 2. **Grid search** — evaluates every `(uid, agent)` pair in `grid` in
///    parallel via `rayon`, retaining the top [`LEADERBOARD_TOP_K`] performers,
///    and writes the resulting leaderboard.
///
/// All output files are written to `file_cfg`'s directory.
///
/// # Arguments
///
/// * `env` — the loaded trading [`Environment`].
/// * `file_cfg` — destination configuration for every report this function emits.
/// * `baseline` — the single agent to backtest for the tearsheet.
/// * `grid` — `(uid, agent)` pairs to backtest in parallel. UIDs are caller-assigned and
///   surface in the leaderboard for traceability.
///
/// # Performance
///
/// Before launching a large grid, benchmark a single agent with
/// [`Environment::evaluate_agent`] and estimate total time as: `(single_agent_time * grid.len()) / cpu_cores`.
fn run_workflow<T>(
    env: &mut Environment,
    file_cfg: &FileConfig,
    mut baseline: T,
    grid: Vec<(usize, T)>,
) -> Result<()>
where
    T: Agent + Send + Serialize,
{
    let label = baseline.identifier();

    println!(">> Running {label} baseline backtest...");
    let journal = env.evaluate_agent(&mut baseline)?;

    journal.to_file_sync(file_cfg)?;
    journal.cumulative_returns()?.to_file_sync(file_cfg)?;
    journal.portfolio_performance()?.to_file_sync(file_cfg)?;
    journal.trade_stats()?.to_file_sync(file_cfg)?;
    env.equity_curve_report()?
        .into_eod()?
        .to_file_sync(file_cfg)?;
    println!(">> {label} baseline backtest complete.");

    println!(">> Evaluating agents in parallel...");
    let leaderboard = env.evaluate_agents(grid, LEADERBOARD_TOP_K)?;
    leaderboard.to_file_sync(file_cfg)?;
    println!(">> {label} grid evaluation complete. Leaderboard saved.");

    Ok(())
}
