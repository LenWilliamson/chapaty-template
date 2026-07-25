use std::{path::Path, str::FromStr, sync::LazyLock};

use anyhow::Result;
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
static GRID_SEARCH_LIMIT: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("GRID_SEARCH_LIMIT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(100)
});

/// Local root directory for all generated reports.
static RESULTS_LOCAL_DIR: &str = "chapaty/reports";

/// Cloud uri for all generated reports.
static RESULTS_CLOUD_URI: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("RESULTS_CLOUD_URI").ok());

/// Which agent to run.
static ACTIVE_AGENT: LazyLock<ActiveAgent> = LazyLock::new(|| {
    std::env::var("ACTIVE_AGENT")
        .ok()
        .and_then(|s| ActiveAgent::from_str(s.trim()).ok())
        .unwrap_or(ActiveAgent::Demo)
});

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

async fn run() -> Result<()> {
    dotenvy::dotenv().ok();
    println!(">> Loading environment...");

    match *ACTIVE_AGENT {
        ActiveAgent::Demo => {
            backtest(
                &mut DemoAgent::env().await?,
                DemoAgent::new(),
                DemoAgentGrid::baseline().build(),
            )
            .await
        }
        ActiveAgent::Template => {
            backtest(
                &mut TemplateAgent::env().await?,
                TemplateAgent::new(),
                TemplateAgentGrid::baseline()?.build(),
            )
            .await
        }
    }
}

/// Runs a baseline backtest followed by a parallel grid search.
///
/// # Backtest steps
///
/// 1. **Baseline backtest:** evaluates `baseline` and writes:
///    - the trade journal,
///    - cumulative returns,
///    - portfolio performance,
///    - trade statistics,
///    - end-of-day equity curve.
/// 2. **Grid search:** evaluates every `(uid, agent)` pair in `grid` in
///    parallel via `rayon`, retaining the top [`LEADERBOARD_TOP_K`] performers,
///    and writes the resulting leaderboard.
///
/// All output files are written to the default results directory.
///
/// # Arguments
///
/// * `env` — the loaded trading [`Environment`].
/// * `baseline` — the single agent to backtest for the tearsheet.
/// * `grid` — `(uid, agent)` pairs to backtest in parallel. UIDs are
///   caller-assigned and surface in the leaderboard for traceability.
///
/// # Performance
///
/// The estimated total runtime of a gridsearch is:
/// `(single_agent_time * grid.len()) / cpu_cores`.
async fn backtest<T>(env: &mut Environment, mut baseline: T, grid: Vec<(usize, T)>) -> Result<()>
where
    T: Agent + Send + Serialize,
{
    let label = baseline.identifier();

    println!(">> Running {label} baseline backtest...");
    let journal = env.evaluate_agent(&mut baseline)?;

    save_report(&journal).await?;
    save_report(&journal.cumulative_returns()?).await?;
    save_report(&journal.portfolio_performance()?).await?;
    save_report(&journal.trade_stats()?).await?;
    save_report(&env.equity_curve_report()?.into_eod()?).await?;

    println!(">> {label} baseline backtest complete.");

    println!(">> Evaluating agents in parallel...");
    let top_k = (*GRID_SEARCH_LIMIT / 10).clamp(10, 100);
    let leaderboard = env.evaluate_agents(subset(grid), top_k)?;
    save_report(&leaderboard).await?;
    println!(">> {label} grid evaluation complete. Leaderboard saved.");

    Ok(())
}

fn subset<T>(mut agents: Vec<(usize, T)>) -> Vec<(usize, T)> {
    agents.shuffle(&mut rand::rng());
    agents.truncate(*GRID_SEARCH_LIMIT);
    agents
}

/// Writes `report` to the cloud bucket if `RESULTS_CLOUD_URI` is set,
/// otherwise to local disk.
async fn save_report<R>(report: &R) -> Result<()>
where
    R: Report + ReportName + ToSchema + Sync + Send,
{
    let agent = ACTIVE_AGENT.as_ref();
    if let Some(prefix) = RESULTS_CLOUD_URI.as_deref() {
        let dest = join(prefix, &format!("{agent}/{}.csv", report.base_name()));
        report.to_cloud(&CloudConfig::new(dest)).await?;
    } else {
        let reports_dir = Path::new(RESULTS_LOCAL_DIR).join(agent);
        report.to_file_sync(&FileConfig::default().with_dir(reports_dir))?;
    }
    Ok(())
}

fn join(prefix: &str, file_name: &str) -> String {
    let p = prefix.trim_end_matches('/');
    format!("{p}/{file_name}")
}
