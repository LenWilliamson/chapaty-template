# Chapaty Template

[![Discord](https://img.shields.io/discord/1495690333911257108.svg?label=Discord&logo=discord&color=7289da&logoColor=white)][discord]
[![CI (Main)](https://github.com/LenWilliamson/chapaty-template/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/LenWilliamson/chapaty-template/actions/workflows/ci.yaml)
[![CI (Develop)](https://github.com/LenWilliamson/chapaty-template/actions/workflows/ci.yaml/badge.svg?branch=develop)](https://github.com/LenWilliamson/chapaty-template/actions/workflows/ci.yaml)
[![Chapaty](https://img.shields.io/crates/v/chapaty.svg?label=chapaty)][chapaty-crate]

The fastest way to build quantitative trading agents in Rust.

[▶️ **Try Web Demo**][chapaty] | [🔬 **Read Deep Dive**][blog-deep-dive] | [👾 **Join Discord**][discord]

---

**Chapaty** brings a familiar [Gym-style API][gymnasium] (`reset`, `step`, `act`) to algorithmic trading research. Built in Rust for high-performance parallel backtesting, this template provides an LLM-friendly workflow and automated [QuantStats](https://github.com/ranaroussi/quantstats) HTML tearsheets out of the box.

![End-to-end Chapaty workflow: running `make run` executes the demo strategy and produces a QuantStats tearsheet.][workflow-gif]

> _End-to-end workflow: `make run` executes the shipped demo agent and generates a QuantStats tearsheet._

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
| **Rust** (`rustup`, `cargo`) | [rust-lang.org/tools/install](https://www.rust-lang.org/tools/install) (Requires 1.97.0+, Edition 2024) |
| **Python** (`3.13.1+`)       | [pyenv](https://github.com/pyenv/pyenv#installation) is recommended.                                    |
| **LLM Environment**          | Claude Code, DeepSeek, Gemini CLI, Aider, Cursor, etc.                                                  |

## Windows Users

The included `Makefile` is designed for Unix-like systems. To run this project on Windows, you have a few options:

1. **WSL (Windows Subsystem for Linux)**: Recommended. Runs the Makefile and paths natively.
2. **Git Bash**: Ships with a `make`-compatible shell and covers most commands.

## Market Data (Free via Hugging Face)

Chapaty uses pre-compiled `.postcard` environments hosted for free on [Hugging Face Datasets][hf-datasets]. Your first `make run` automatically downloads and caches the required data locally.

Need a different dataset or timeframe? Drop a request in the `#data-requests` channel on [Discord][discord].

## Technical Analysis and Indicators

Chapaty includes pre-calculated technical analysis out of the box, so your agents can focus on decision-making:

- **Trend & Momentum:** SMA, EMA, RSI (Calculated on-the-fly via `StreamingSma`, `StreamingRsi`, etc.)
- **Volume & Orderflow:** Volume Profile, TPO/Market Profile
- **Contextual:** Economic Calendar events

Need a specific indicator we don't have? Please open a **Feature Request** on the [Chapaty core repository][chapaty-repo-issues] (including the mathematical formula or reference implementation), or simply drop a request in the `#data-requests` channel on [Discord][discord].

## Staying Updated

Chapaty is evolving. To pull the latest AI prompts and updated visualization scripts without breaking your custom strategies:

```bash
make update
```

This synchronizes `AI.md`, the entire `.ai/` directory, and the `visualization/` directory with the upstream `main` branch, runs a global `cargo update` to fetch the latest patch versions of all Rust dependencies, and finally refreshes the `Makefile` itself.

If the `Makefile` changed, re-run `make update` once to apply the new logic.

> **Warning:** Any manual changes to `AI.md`, the `.ai/` directory, the `visualization/` directory, or the `Makefile` will be overwritten. Your `src/` directory and `Cargo.toml` are left untouched — only `cargo update` will modify `Cargo.lock`.

## Version Compatibility

By default, the `main` branch of this template is always locked to the latest stable release of the `chapaty` core engine.

If you need to pin your repository to a historical version, you can check out a specific Git tag. We use SemVer build metadata (`+x`) to track template-specific improvements (like LLM prompt updates or Makefile fixes) independently from the core engine.

| Template Tag | Core `chapaty` Version | Notes         |
| ------------ | ---------------------- | ------------- |
| `v1.3.4+x`   | `1.3.4`                | Active Stable |
| `v1.3.1+x`   | `1.3.1`                | Legacy        |
| `v1.3.0+x`   | `1.3.0`                | Legacy        |
| `v1.2.1+x`   | `1.2.1`                | Legacy        |
| `v1.2.0+x`   | `1.2.0`                | Legacy        |
| `v1.1.4+x`   | `1.1.4`                | Legacy        |
| `v1.1.3+x`   | `1.1.3`                | Legacy        |
| `v1.1.2+x`   | `1.1.2`                | Legacy        |
| `v1.1.0+x`   | `1.1.0`                | Legacy        |

_(Example: Checking out tag `v1.1.2+5` guarantees you are using the 5th iteration of the template designed specifically for `chapaty v1.1.2`.)_

## Repository Layout

```text
chapaty-template/
├── AI.md                        # AI bootstrap (defers to .ai/)
├── .ai/                         # AI-agnostic prompts
│   ├── agent-plan.md            # Strict spec-first protocol
│   ├── algorithm-ideas.md       # Seed strategies
│   ├── chapaty-api.md           # Exact chapaty API surface (don't hallucinate)
│   ├── rust-vibe-rules.md       # Rust rules for user code
│   └── update-prompts.md        # Triage prompt for post-release .ai/ updates
├── .github/
│   └── workflows/
│       └── ci.yaml              # CI/CD pipeline (you may delete this)
├── bin/
│   └── pre-push.sh              # fmt + clippy + audit + test + doc + build (you may delete this)
├── chapaty/
│   └── reports/                 # Output reports and CSVs (generated after `make run`)
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
│   │   ├── demo.rs
│   │   ├── template/             # Starter skeleton, ready to fill in
│   │   │   ├── agent.rs
│   │   │   └── spec.md
│   │   └── template.rs
│   ├── agents.rs
│   ├── crash.rs                 # Panic hook + fatal-error reporting (stderr + optional GCS upload)
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

- [Discord][discord]: The fastest way to get help, request data, or post your strategy's PnL in the `#tearsheets` channel.

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

[chapaty]: https://chapaty.com
[discord]: https://discord.gg/MmMAB6NCuK
[chapaty-crate]: https://crates.io/crates/chapaty
[hf-datasets]: https://huggingface.co/datasets/chapaty/environments
[chapaty-repo-issues]: https://github.com/LenWilliamson/chapaty/issues
[template-repo-issues]: https://github.com/LenWilliamson/chapaty-template/issues
[gymnasium]: https://github.com/Farama-Foundation/Gymnasium
[blog-deep-dive]: https://dev.to/len_chapaty/an-open-source-gym-style-backtesting-framework-for-algorithmic-trading-in-rust-53fg
[workflow-gif]: https://media2.dev.to/dynamic/image/width=800%2Cheight=%2Cfit=scale-down%2Cgravity=auto%2Cformat=auto/https%3A%2F%2Fdev-to-uploads.s3.amazonaws.com%2Fuploads%2Farticles%2F09ihqa1cehty46nxipg1.gif
