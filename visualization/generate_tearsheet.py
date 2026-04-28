"""
Generate a QuantStats HTML tearsheet from a Chapaty Equity Curve.

Reads `chapaty/reports/equity_curve.parquet` if present, else falls back to
`chapaty/reports/equity_curve.csv`.

Converts the pre-downsampled Mark-to-Market (M2M) PnL snapshots from the
chapaty lib into a continuous, daily percentage return series required by
QuantStats to calculate true institutional metrics (Sharpe, Drawdown, Volatility).

Note: This script should be executed via the project's isolated virtual
environment, which is handled automatically by running `make run`.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pandas as pd
import quantstats as qs

INITIAL_CAPITAL: float = 10_000.0
BENCHMARK_TICKER: str = "SPY"  # SPDR S&P 500 ETF

# Path resolution to lock exactly to the reports folder
PROJECT_ROOT = Path(__file__).resolve().parent.parent
REPORTS_DIR = PROJECT_ROOT / "chapaty" / "reports"
OUTPUT_PATH = REPORTS_DIR / "tearsheet.html"

# Columns matching the Rust EquityCurveCol enum
TS_COL = "timestamp"
PNL_COL = "portfolio_value"


def load_equity_curve() -> pd.DataFrame:
    parquet_path = REPORTS_DIR / "equity_curve.parquet"
    csv_path = REPORTS_DIR / "equity_curve.csv"

    if parquet_path.exists():
        print(f"[tearsheet] Reading {parquet_path}")
        return pd.read_parquet(parquet_path)
    if csv_path.exists():
        print(f"[tearsheet] Reading {csv_path}")
        return pd.read_csv(csv_path)

    print(
        f"[tearsheet] ERROR: no equity curve found in {REPORTS_DIR}/. "
        f"Did you run `make run` first?",
        file=sys.stderr,
    )
    sys.exit(1)


def build_return_series(df: pd.DataFrame) -> pd.Series:
    missing = {TS_COL, PNL_COL} - set(df.columns)
    if missing:
        print(
            f"[tearsheet] ERROR: equity curve missing required columns: {missing}. "
            f"Got columns: {list(df.columns)}",
            file=sys.stderr,
        )
        sys.exit(1)

    df = df.copy()

    # 1. Coerce timestamps and drop invalid rows
    df[TS_COL] = pd.to_datetime(df[TS_COL], utc=True, errors="coerce")
    df = df.dropna(subset=[TS_COL, PNL_COL])

    if df.empty:
        print("[tearsheet] ERROR: equity curve data is empty.", file=sys.stderr)
        sys.exit(1)

    # 2. Calculate Total Absolute Equity at each tick
    # portfolio_value is the Net M2M PnL (+150, -20, etc).
    df["total_equity"] = INITIAL_CAPITAL + df[PNL_COL]

    # 3. Set Timezone-Naive DatetimeIndex
    # Chapaty natively downsamples to EOD, outputting strict 00:00:00 UTC timestamps.
    # We just need to strip the timezone for QuantStats.
    df["date"] = df[TS_COL].dt.tz_localize(None)
    daily_equity = df.set_index("date")["total_equity"].sort_index()

    # 4. Forward-Fill Calendar Days
    # If the market is closed or the strategy holds without new ticks over a weekend,
    # the equity stays exactly the same. Forward-filling ensures those days register as 0% return.
    full_date_range = pd.date_range(
        start=daily_equity.index.min(), end=daily_equity.index.max(), freq="D"
    )
    daily_equity = daily_equity.reindex(full_date_range).ffill()

    # 5. Convert absolute equity curve into daily percentage returns
    daily_returns = daily_equity.pct_change().dropna()

    return daily_returns


def main() -> int:
    df = load_equity_curve()
    returns = build_return_series(df)

    REPORTS_DIR.mkdir(parents=True, exist_ok=True)

    # Note: QuantStats will require an internet connection here to download
    # the benchmark historical data via Yahoo Finance.
    print("[tearsheet] Generating QuantStats report (this may take a moment)...")
    qs.reports.html(
        returns,
        benchmark=BENCHMARK_TICKER,
        output=str(OUTPUT_PATH),
        title="Chapaty Portfolio Tearsheet",
        download_filename=OUTPUT_PATH.name,
    )

    print(f"[tearsheet] Wrote {OUTPUT_PATH}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
