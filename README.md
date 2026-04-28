# Chapaty Template

[![Discord](https://img.shields.io/discord/1495690333911257108.svg?label=Discord&logo=discord&color=7289da&logoColor=white)][discord]
[![CI (Main)](https://github.com/LenWilliamson/chapaty-template/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/LenWilliamson/chapaty-template/actions/workflows/ci.yml)
[![CI (Develop)](https://github.com/LenWilliamson/chapaty-template/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/LenWilliamson/chapaty-template/actions/workflows/ci.yml)
[![Chapaty](https://img.shields.io/crates/v/chapaty.svg?label=chapaty)][chapaty-crate]

> **Welcome to Chapaty!** Trying out a new framework can be frustrating if things break on day one. If you run into setup issues, framework bugs, or missing data, please reach out on [Discord][discord]. We want to ensure a smooth developer experience and will fix framework bugs promptly.

This repository is the fastest way to start building quantitative trading agents in Rust. Designed with a familiar, [**Gym-style API**][gymnasium] (`reset`, `step`, `act`), [`chapaty`][chapaty-crate] evaluates parallel backtests efficiently. This template wires the core `chapaty` library up with an LLM-friendly workflow and an automated QuantStats HTML tearsheet.

**You don't need to be a Rust expert.** Describe your strategy in plain English, and instruct your LLM of choice to generate the Rust code using the provided `AI.md` instructions (see: [Vibe-Coding Workflow](#vibe-coding-workflow)).

## Quick Start (60 seconds)

Windows users: Please read the [Windows Users](#windows-users) section before proceeding.

```bash
# 1. Clone the template (we use 'ct' as a shorthand directory name)
git clone --depth 1 https://github.com/LenWilliamson/chapaty-template.git ct
cd ct

# 2. Check dependencies (Rust + Python)
make doctor

# 3. Compile the project and install visualization dependencies
make setup

# 4. Run the shipped demo agent and generate a tearsheet
make run

# 5. Open the resulting HTML report
open chapaty/reports/tearsheet.html        # macOS
# xdg-open chapaty/reports/tearsheet.html  # Linux
```

## Prerequisites

| Tool                         | Installation                                                                                            |
| ---------------------------- | ------------------------------------------------------------------------------------------------------- |
| **Rust** (`rustup`, `cargo`) | [rust-lang.org/tools/install](https://www.rust-lang.org/tools/install) (Requires 1.95.0+, Edition 2024) |
| **Python** (`3.13.1+`)       | [pyenv](https://github.com/pyenv/pyenv#installation) is recommended.                                    |
| **LLM Environment**          | Claude Code, DeepSeek, Gemini CLI, Aider, Cursor, etc.                                                  |

## Windows Users

The included `Makefile` is designed for Unix-like systems. To run this project on Windows, you have a few options:

1. **WSL (Windows Subsystem for Linux)**: Recommended. Runs the Makefile and paths natively.
2. **Git Bash**: Ships with a `make`-compatible shell and covers most commands.

## Market Data (Free via Hugging Face)

Chapaty uses pre-compiled `.postcard` environments hosted for free on [Hugging Face Datasets][hf-datasets]. Your first `make run` automatically downloads and caches the required data locally.

**Currently available datasets:**

- Binance BTC/USDT (Daily spot, 1h / 1m spot, with SMA / TPO / Volume Profile variants)
- NinjaTrader CME EUR/USD futures (1m / 5m, with US high-impact employment news)

Need a different dataset or timeframe? Drop a request in the `#data-requests` channel on [Discord][discord].

## Technical Analysis and Indicators

Chapaty includes pre-calculated technical analysis out of the box, so your agents can focus on decision-making:

- **Trend & Momentum:** SMA, EMA, RSI
- **Volume & Orderflow:** Volume Profile, TPO/Market Profile
- **Contextual:** Economic Calendar events

Need a specific indicator we don't have? Please open a **Feature Request** on the [Chapaty core repository][chapaty-repo-issues] (including the mathematical formula or reference implementation), or simply drop a request in the `#data-requests` channel on [Discord][discord].

## Vibe-Coding Workflow

1. **Describe your idea:**

   ```bash
   mkdir -p src/agents/my_strategy
   touch src/agents/my_strategy/spec.md
   ```

2. **Dump your thoughts into `spec.md`:**
   Paste a PDF excerpt, a Pine Script / Python snippet, a blog URL, or a rough paragraph. No formal structure is required.

3. **Prompt your LLM:**
   Copy and paste this prompt into your AI coding tool:

   ```text
   Read `AI.md` and `src/agents/my_strategy/spec.md`. Ask me clarifying questions about entries, exits, stop-loss, take-profit, timeframes, and assets. Once approved, rewrite `spec.md` into a formal specification, build the strategy in `src/agents/my_strategy/agent.rs`, and wire it into `src/main.rs`.
   ```

4. **Run and Review:**
   ```bash
   make run
   open chapaty/reports/tearsheet.html
   ```

## Staying Updated

Chapaty is evolving. To pull the latest API docs for your LLM and updated visualization scripts without breaking your custom strategies:

```bash
make update
```

This updates `AI.md`, `visualization/generate_tearsheet.py`, and runs `cargo update -p chapaty`. Your `src/agents/` directory is left untouched.

## Version Compatibility

This template's git tags mirror the exact `chapaty` crate version it was validated against. Pin your clone to match your desired dependency:

| Template Tag | `chapaty` Version | Notes  |
| ------------ | ----------------- | ------ |
| `v1.1.0`     | `1.1.0`           | Stable |

## Repository Layout

```text
chapaty-template/
├── AI.md                        # AI bootstrap (defers to .ai/)
├── .ai/                         # AI-agnostic prompts
│   ├── agent-plan.md            # Strict spec-first protocol
│   ├── algorithm-ideas.md       # Seed strategies
│   ├── chapaty-api.md           # Exact chapaty API surface (don't hallucinate)
│   └── rust-vibe-rules.md       # Rust rules for user code
├── .github/
│   └── workflows/               # CI/CD pipelines (you may delete this)
├── bin/
│   └── pre-push.sh              # fmt + clippy + test + build
├── chapaty/
│   └── reports/                 # Output reports and CSVs
│       ├── cumulative_returns.csv
│       ├── equity_curve.csv
│       ├── journal.csv
│       ├── portfolio_performance.csv
│       ├── tearsheet.html       # QuantStats report
│       └── trade_statistics.csv
├── src/
│   ├── agents/                  # Your strategies live here
│   │   ├── demo/                # Shipped demo (safe to delete/override)
│   │   │   ├── agent.rs
│   │   │   └── spec.md
│   │   └── demo.rs
│   ├── agents.rs
│   └── main.rs                  # Runner (async tokio main)
├── visualization/
│   ├── generate_tearsheet.py    # pandas + quantstats HTML tearsheet
│   └── requirements.txt
├── Cargo.lock
├── Cargo.toml
├── LICENSE
├── Makefile
└── README.md
```

## Getting Help & Contributing

**Community & Support:**

- [Discord][discord]: The fastest way to get help, request data, or post your strategy's PnL in the `#showcase` channel.

**Issue Tracking:**

- [Template Issues][template-repo-issues]: Open issues here if the `Makefile`, Python script, or CI pipelines are broken.
- [Chapaty Core Issues][chapaty-repo-issues]: Open issues here for bugs in the chapaty lib, memory leaks, or new Technical Indicator requests, etc.

Before submitting a Pull Request to this template, please verify your changes pass:

```bash
./bin/pre-push.sh
```

## Disclaimer

**Trading and investing involve substantial risk. You may lose some or all of your capital.**

Chapaty is an **open-source software project** provided for **research and educational purposes only**. It **does not constitute financial, investment, legal, or trading advice**.

This software is provided **“AS IS”**, without warranties or conditions of any kind, express or implied, as stated in the **Apache License, Version 2.0**. The software may contain bugs, errors, or inaccuracies.

**In no event shall the authors or contributors be liable for any direct or indirect losses, damages, or consequences**, including but not limited to financial losses, arising from the use of this software.

By using Chapaty, you acknowledge that **you are solely responsible for any trading decisions, strategies, or outcomes**.

[discord]: https://discord.gg/MmMAB6NCuK
[chapaty-crate]: https://crates.io/crates/chapaty
[hf-datasets]: https://huggingface.co/datasets/chapaty/environments
[chapaty-repo-issues]: https://github.com/LenWilliamson/chapaty/issues
[template-repo-issues]: https://github.com/LenWilliamson/chapaty-template/issues
[gymnasium]: https://github.com/Farama-Foundation/Gymnasium
