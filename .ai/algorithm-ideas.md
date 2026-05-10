# Seed Strategies: Ideas to Suggest

Use these when the user is unsure what to build. Each maps to a shipped `EnvPreset` so the LLM can immediately scaffold a `spec.md`.

_(Note: Remember the Data-Agnostic rule. If the user asks for "SMA Crossover on AAPL", you can still pitch Idea #1 using the BTC preset as a temporary placeholder while they request AAPL data on Discord)._

## 1. SMA Crossover (classic, good first build)

**Preset:** `EnvPreset::BinanceBtcUsdt1d` or `EnvPreset::BinanceBtcUsdt1dSma20Sma50`
**Idea:** Go long when SMA(20) crosses above SMA(50); close when it crosses back down. Calculate indicators on the fly using `StreamingSma`.
**References:** `chapaty::gym::trading::agent::crossover::{StreamingCrossover, PrecomputedCrossover}`.

## 2. News Breakout

**Preset:** `EnvPreset::NinjaTraderCme6eh61mUsEmpHighEventsOnly`
**Idea:** After a US high-impact employment release, wait N minutes, then enter in the direction of the news candle if price breaks its high/low.
**Reference:** `chapaty::gym::trading::agent::news::breakout::NewsBreakout` (also `NewsBreakoutGrid` for parameter sweeps).

## 3. News Fade

**Preset:** `EnvPreset::NinjaTraderCme6eh61mUsEmpHighEventsOnly`
**Idea:** Opposite of breakout. Assume markets overreact to news, enter counter-trend after a cool-down.
**Reference:** `chapaty::gym::trading::agent::news::fade::NewsFade` / `NewsFadeGrid`.

## 4. News Hybrid

**Preset:** `EnvPreset::NinjaTraderCme6eh61m5mUsEmpHighEventsOnly`
**Idea:** A composite agent that runs both the Breakout and Fade strategies simultaneously using a strict priority policy. Breakout signals represent stronger informational value and always dominate; if a Breakout triggers, it executes and retroactively closes any open Fade trades ("pivot" logic).
**Reference:** `chapaty::gym::trading::agent::news::hybrid::NewsHybrid` / `NewsHybridGrid`.

## 5. Bidirectional SMA Crossover (shipped demo)

**Preset:** `EnvPreset::BinanceBtcUsdt1d`
**Idea:** A continuous "Stop-and-Reverse" trend strategy. When the Fast SMA crosses above the Slow SMA, close any open Short positions and enter Long. When it crosses below, close any Long positions and enter Short. This ensures the agent is always exposed to the prevailing trend. See `src/agents/demo/agent.rs` for the shipped baseline implementation utilizing the `StreamingSma` indicator.

## 6. RSI Mean Reversion (classic, good first build)

**Preset:** `EnvPreset::BinanceBtcUsdt1d`
**Idea:** Calculate a 14-period RSI on the fly using `StreamingRsi`. Go long when RSI < 30; go short when RSI > 70; close open positions on neutral band re-entry (e.g., RSI crosses 50).

## 7. Volume Profile POC Reversion

**Preset:** `EnvPreset::BinanceBtcUsdt1h1mVolumeProfile1d100Usdt`
**Idea:** When price trades two standard deviations from the prior-day Point of Control, fade back toward it.

## 8. TPO Open-Drive

**Preset:** `EnvPreset::BinanceBtcUsdt1h1mTpo1d1Usdt` or `NinjaTraderCme6eh61mTpo1d`
**Idea:** When the opening range extends outside prior-day value area within the first 30 minutes, trade in the direction of the breakout.

## Template for Telling the User About a New Idea

> "Here are a few seed strategies that fit the data we already have preloaded. Pick one and I'll scaffold a spec.md, or drop your own idea into `src/agents/<name>/spec.md` and I'll work from that."

## Boilerplate & Reference Examples (For LLMs)

> **LLM FALLBACK DIRECTIVE:** The `chapaty-api.md` file contains everything you need for 95% of strategies. However, if the user explicitly asks for an exact clone of an official core strategy, OR if you are failing to compile a complex multi-agent state machine, you may use your web-fetching tools to read the raw source code of our official implementations as a reference.

**For single-run testing in `src/main.rs`:**

- `crossover.rs`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/examples/crossover.rs)
- `news_breakout.rs`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/examples/news_breakout.rs)
- `news_fade.rs`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/examples/news_fade.rs)
- `news_hybrid.rs`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/examples/news_hybrid.rs)

**For highly parallel Grid Search setups (`src/main.rs`):**

- `news_breakout_grid.rs`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/examples/news_breakout_grid.rs)
- `news_fade_grid.rs`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/examples/news_fade_grid.rs)

**For Agent Implementations (`src/agents/<name>/agent.rs`):**

- `crossover`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/src/gym/trading/agent/crossover.rs)
- `news_breakout`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/src/gym/trading/agent/news/breakout.rs)
- `news_fade`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/src/gym/trading/agent/news/fade.rs)
- `news_hybrid`: [View Raw](https://raw.githubusercontent.com/LenWilliamson/chapaty/refs/heads/main/src/gym/trading/agent/news/hybrid.rs)

If the user asks for a News Breakout strategy, fetch `breakout.rs` and `news_breakout.rs` to guarantee your generated code matches the framework's optimal design patterns.
