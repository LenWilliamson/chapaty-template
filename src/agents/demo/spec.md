# Formal Specification: Bidirectional SMA Crossover (Demo)

## 1. Summary
A continuous, stop-and-reverse (SAR) trend-following strategy. It uses a Fast SMA and a Slow SMA to determine the market trend. When the Fast SMA crosses above the Slow SMA, it enters a Long position. When it crosses below, it closes the Long and immediately enters a Short position, ensuring the agent is always exposed to the prevailing trend.

## 2. Environment
**Preset:** `EnvPreset::BinanceBtcUsdt1dSma20Sma50`
**Why:** Provides daily Bitcoin spot data. While the preset includes precomputed SMAs, this demo utilizes the `StreamingSma` technical indicator to demonstrate how users can compute their own stateful indicators on the fly.

## 3. Observation Inputs
- **Market Data:** `obs.market_view.ohlcv().last_event(&self.ohlcv_id)` to feed the streaming SMAs.
- **Portfolio State:** `obs.states.find_active_trade_for_agent(&self.identifier())` to check current position and `TradeType`.

## 4. Entry Logic
- If `Fast SMA > Slow SMA` (Bullish) AND the current position is Flat or Short:
  - Open a Long position (Market Order).
- If `Fast SMA < Slow SMA` (Bearish) AND the current position is Flat or Long:
  - Open a Short position (Market Order).

## 5. Exit Logic
Exits are purely signal-driven. 
- If a Long is open and a Bearish signal occurs, close the Long at market.
- If a Short is open and a Bullish signal occurs, close the Short at market.
*(Because Chapaty processes `MarketClose` before `Open` within the same step, we can yield both actions simultaneously).*

## 6. Parameters
- `fast_period`: Lookback window for the Fast SMA. Default: `20`. Grid range: `[10, 30]`.
- `slow_period`: Lookback window for the Slow SMA. Default: `50`. Grid range: `[40, 60]`.

## 7. Assumptions / Out of Scope
- Trades a fixed quantity of `1.0` BTC. Risk-based position sizing is out of scope for this simple demo.
- Assumes sufficient margin to execute stop-and-reverse sweeps.
