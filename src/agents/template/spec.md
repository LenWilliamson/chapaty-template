# Formal Specification: Breakout Range Strategy (Demo2)

## 1. Summary

A momentum breakout strategy that trades in the direction of unusually large daily candles. It calculates the body range (absolute difference between open and close) of each daily candle and compares it against a Simple Moving Average (SMA) of recent body ranges. When the current body range exceeds the SMA multiplied by a factor, a breakout signal is triggered. The strategy includes pyramiding logic (opening additional trades if successive signals align with the current trend) and a specific cool-down mechanism for counter-trend signals to avoid immediate whipsaws.

## 2. Environment

**Preset:** `EnvPreset::BinanceBtcUsdt1d`
**Why:** The strategy is explicitly designed around daily candles and calculating a 20-day moving average of their ranges. This daily dataset perfectly fits the required timeframe, and BTC-USDT is a solid default placeholder asset.

## 3. Observation Inputs

- `obs.market_view.ohlcv().last_event()`: To access the latest closed daily candle (Open, Close) and calculate the `Range`.
- `obs.states.iter_live()`: To manage current open positions, check their direction (Long vs. Short), and handle pyramiding or closing logic.

## 4. Entry Logic

1. **Range Calculation**: For each completed daily candle, calculate the body `Range` = `abs(close - open)`.
2. **SMA Calculation**: Maintain a rolling 20-period SMA of the `Range` values (`volma`).
3. **Breakout Signal**: A signal occurs if `Range > volma * volmult`.
   - If `close > open` (Green): Signal is **Long**.
   - If `close < open` (Red): Signal is **Short**.
4. **Execution**:
   - **No Open Positions**: Enter a trade in the direction of the signal.
   - **Currently in a Position**:
     - _Same Direction (Pyramiding)_: If the signal direction matches the current open position(s) (e.g., Short position and a new Short signal), open an _additional_ trade in the same direction.
     - _Opposite Direction (Counter)_: If the signal opposes the current open position(s) (e.g., Short position but a Long signal), immediately close the current open trade(s). **Do not** open a new trade in the counter direction. Instead, enter a "cool-down" state to ignore this specific bar for entry and wait for the next valid signal on subsequent days.

## 5. Exit Logic

- **Dynamic Stop Loss (SL)**: Upon entry, calculate the SL distance based on the `Range` of the triggering breakout bar. SL distance = `Range * sl_pct` (default 75% or 0.75).
- **Dynamic Take Profit (TP)**: Set the TP using a Risk-Reward Ratio (`tp_crv`). TP distance = `SL distance * tp_crv` (default 2.0).
  - _Example_: Breakout Range is $2,000. SL is placed $1,500 away. TP is placed $3,000 away.
- **Manual Override**: As defined in the Entry Logic, if a breakout signal triggers in the opposite direction of the current trade, the position is closed immediately at market price, superseding the SL/TP.

## 6. Parameters

| Field       | Type    | Default | Description                                                                     | Grid Search Range  |
| :---------- | :------ | :------ | :------------------------------------------------------------------------------ | :----------------- |
| `volmalen`  | `usize` | `20`    | Length of the SMA used to average past body ranges.                             | `[10, 20, 30]`     |
| `volmult`   | `f64`   | `2.0`   | The multiplier applied to the SMA to define a breakout threshold.               | `[1.5, 2.0, 2.5]`  |
| `sl_pct`    | `f64`   | `0.75`  | Percentage of the breakout bar's range to use for Stop Loss distance.           | `[0.5, 0.75, 1.0]` |
| `tp_crv`    | `f64`   | `2.0`   | Risk-Reward multiplier applied to the SL distance to calculate the TP distance. | `[1.5, 2.0, 3.0]`  |
| `trade_qty` | `f64`   | `1.0`   | Fixed position size (Quantity) for each trade.                                  | N/A                |

## 7. Assumptions / Out of Scope

- **SMA Indicator**: Since the core engine's `StreamingSma` expects `f64` we will feed the `Range` (as an `f64`) into a standard `StreamingSma` instance inside the agent's state manually every tick.
- **Cool-down Scope**: The cool-down rule implies we just skip opening a new trade on the exact day the counter-trend signal happens. The very next day, if a fresh breakout occurs, it is evaluated normally.
- **Pyramiding Quantity**: We assume each additional trade opened during a pyramiding signal uses the same base `trade_qty`.
