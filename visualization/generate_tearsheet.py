"""
Generate a QuantStats HTML tearsheet from a Chapaty Equity Curve.

Mirrors `src/main.rs`'s `save_report`: if `RESULTS_CLOUD_BUCKET` is set (a
"gs://bucket/prefix" URI), the equity curve is read from
`{RESULTS_CLOUD_BUCKET}/{agent}/equity_curve.csv` and the finished tearsheet
is uploaded back to `{RESULTS_CLOUD_BUCKET}/{agent}/tearsheet.html`. Otherwise
both read and write stay local: `chapaty/reports/<agent>/equity_curve.parquet`
(falling back to the `.csv`) and `chapaty/reports/<agent>/tearsheet.html`.
The agent subdirectory is passed as the first CLI argument (e.g.
`python generate_tearsheet.py demo`) and matches the `ActiveAgent` variant
name (lowercased) declared in `src/main.rs`.

Converts the pre-downsampled Mark-to-Market (M2M) PnL snapshots from the
chapaty lib into a continuous, daily percentage return series required by
QuantStats to calculate true institutional metrics (Sharpe, Drawdown, Volatility).

Note: This script should be executed via the project's isolated virtual
environment, which is handled automatically by running `make run`.
"""

from __future__ import annotations

import argparse
import io
import os
import sys
from pathlib import Path

import numpy as np
import pandas as pd
import quantstats as qs
from dotenv import load_dotenv
from google.cloud.storage import Blob, Client

INITIAL_CAPITAL: float = 10_000.0
BENCHMARK_TICKER: str = "SPY"  # SPDR S&P 500 ETF

# Path resolution to lock exactly to the reports folder
PROJECT_ROOT = Path(__file__).resolve().parent.parent
REPORTS_ROOT = PROJECT_ROOT / "chapaty" / "reports"

# Columns matching the Rust EquityCurveCol enum
TS_COL = "timestamp"
PNL_COL = "portfolio_value"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a QuantStats tearsheet for a Chapaty agent's backtest."
    )
    parser.add_argument(
        "agent",
        help="Agent subdirectory under chapaty/reports/ (e.g. 'demo', 'template').",
    )
    return parser.parse_args()


def load_equity_curve_local(reports_dir: Path) -> pd.DataFrame:
    parquet_path = reports_dir / "equity_curve.parquet"
    csv_path = reports_dir / "equity_curve.csv"

    if parquet_path.exists():
        print(f"[tearsheet] Reading {parquet_path}")
        return pd.read_parquet(parquet_path)
    if csv_path.exists():
        print(f"[tearsheet] Reading {csv_path}")
        return pd.read_csv(csv_path)

    print(
        f"[tearsheet] ERROR: no equity curve found in {reports_dir}/. "
        f"Did you run `make run` first?",
        file=sys.stderr,
    )
    sys.exit(1)


def load_equity_curve_cloud(
    client: Client, bucket_uri: str, agent: str
) -> pd.DataFrame:
    blob = Blob.from_string(
        f"{bucket_uri.rstrip('/')}/{agent}/equity_curve.csv", client=client
    )

    print(f"[tearsheet] Reading gs://{blob.bucket.name}/{blob.name}")
    if not blob.exists():
        print(
            f"[tearsheet] ERROR: no equity curve found at gs://{blob.bucket.name}/{blob.name}. "
            f"Did the backtest run with RESULTS_CLOUD_BUCKET set?",
            file=sys.stderr,
        )
        sys.exit(1)

    return pd.read_csv(io.BytesIO(blob.download_as_bytes()))


def upload_tearsheet_cloud(
    client: Client, output_path: Path, bucket_uri: str, agent: str
) -> None:
    blob = Blob.from_string(
        f"{bucket_uri.rstrip('/')}/{agent}/{output_path.name}", client=client
    )

    print(
        f"[tearsheet] Uploading {output_path} to gs://{blob.bucket.name}/{blob.name}..."
    )
    try:
        blob.upload_from_filename(str(output_path))
        print(f"[tearsheet] Uploaded to gs://{blob.bucket.name}/{blob.name}")
    except Exception as e:  # noqa: BLE001 - best-effort upload, never fail the run over it
        print(
            f"[tearsheet] WARNING: failed to upload to gs://{blob.bucket.name}/{blob.name}: {e}",
            file=sys.stderr,
        )


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
    load_dotenv(PROJECT_ROOT / ".env")

    args = parse_args()
    reports_dir = REPORTS_ROOT / args.agent
    output_path = reports_dir / "tearsheet.html"

    bucket_uri = os.environ.get("RESULTS_CLOUD_BUCKET")
    client = Client() if bucket_uri else None

    if bucket_uri:
        df = load_equity_curve_cloud(client, bucket_uri, args.agent)
    else:
        if not reports_dir.is_dir():
            print(
                f"[tearsheet] ERROR: agent directory not found: {reports_dir}",
                file=sys.stderr,
            )
            return 1
        df = load_equity_curve_local(reports_dir)

    returns = build_return_series(df)

    if returns.empty or np.isclose(returns, 0.0, atol=1e-8).all():
        print(
            "[tearsheet] WARNING: Strategy generated no returns (flat equity curve).",
            file=sys.stderr,
        )
        print(
            "[tearsheet] Skipping QuantStats HTML generation to avoid math errors.",
            file=sys.stderr,
        )
        return 0

    reports_dir.mkdir(parents=True, exist_ok=True)

    # Note: QuantStats will require an internet connection here to download
    # the benchmark historical data via Yahoo Finance.
    print("[tearsheet] Generating QuantStats report (this may take a moment)...")
    qs.reports.html(
        returns,
        benchmark=BENCHMARK_TICKER,
        output=str(output_path),
        title=f"Chapaty Portfolio Tearsheet — {args.agent}",
        download_filename=output_path.name,
    )

    print(f"[tearsheet] Wrote {output_path}")

    if bucket_uri:
        upload_tearsheet_cloud(output_path, bucket_uri, args.agent, client)

    return 0


if __name__ == "__main__":
    sys.exit(main())
