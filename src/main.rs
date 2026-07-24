use std::{path::Path, str::FromStr, sync::LazyLock};

use anyhow::{Context, Result};
use chapaty::prelude::*;
use rand::seq::SliceRandom;
use serde::Serialize;
use strum::{AsRefStr, Display, EnumString};

use crate::agents::{
    demo::{DemoAgent, DemoAgentGrid},
    template::{TemplateAgent, TemplateAgentGrid},
};

mod agents;
mod crash;

/// Number of agents randomly selected from the agent grid.
static GRID_SEARCH_LIMIT: LazyLock<u32> = LazyLock::new(|| {
    std::env::var("GRID_SEARCH_LIMIT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(100)
});

/// Root directory for all generated reports.
static RESULTS_DIR: LazyLock<String> = LazyLock::new(|| {
    std::env::var("RESULTS_DIR").unwrap_or_else(|_| "chapaty/reports".to_string())
});

/// Which agent to run.
static ACTIVE_AGENT: LazyLock<ActiveAgent> = LazyLock::new(|| {
    std::env::var("ACTIVE_AGENT")
        .ok()
        .and_then(|s| ActiveAgent::from_str(s.trim()).ok())
        .unwrap_or(ActiveAgent::Demo)
});

/// Available agents. Add a variant + a match arm in `main` to register a new
/// one.
#[derive(Debug, Clone, Copy, AsRefStr, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
enum ActiveAgent {
    Demo,
    Template,
}

#[tokio::main]
async fn main() {
    crash::install_panic_hook();
    if let Err(err) = run().await {
        crash::handle_fatal_error(err).await;
    }
}

/// The actual application entry point. All fallible startup/workflow logic
/// lives here so `main` stays free to funnel every `Err` through the crash
/// reporting path below.
async fn run() -> Result<()> {
    println!(">> Loading environment from Hugging Face...");
    let mut env = environment().await?;
    let ohlcv = ohlcv_id();

    let reports_dir = Path::new(&*RESULTS_DIR).join(ACTIVE_AGENT.as_ref());
    let file_cfg = FileConfig::default().with_dir(&reports_dir);

    match *ACTIVE_AGENT {
        ActiveAgent::Demo => run_workflow(
            &mut env,
            &file_cfg,
            DemoAgent::new(ohlcv, 20, 50),
            DemoAgentGrid::baseline(ohlcv)?.build(),
        ),
        ActiveAgent::Template => run_workflow(
            &mut env,
            &file_cfg,
            TemplateAgent::new(ohlcv),
            TemplateAgentGrid::baseline(ohlcv)?.build(),
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
/// * `file_cfg` — destination configuration for every report this function
///   emits.
/// * `baseline` — the single agent to backtest for the tearsheet.
/// * `grid` — `(uid, agent)` pairs to backtest in parallel. UIDs are
///   caller-assigned and surface in the leaderboard for traceability.
///
/// # Performance
///
/// Before launching a large grid, benchmark a single agent with
/// [`Environment::evaluate_agent`] and estimate total time as:
/// `(single_agent_time * grid.len()) / cpu_cores`.
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
    let top_k = (*GRID_SEARCH_LIMIT / 10).max(100);
    let grid_subset = select_grid_subset(grid);
    let leaderboard = env.evaluate_agents(grid_subset, top_k as usize)?;
    leaderboard.to_file_sync(file_cfg)?;
    println!(">> {label} grid evaluation complete. Leaderboard saved.");

    Ok(())
}

pub fn select_grid_subset<T>(mut agents: Vec<(usize, T)>) -> Vec<(usize, T)> {
    agents.shuffle(&mut rand::rng());
    agents.truncate(*GRID_SEARCH_LIMIT as usize);
    agents
}
