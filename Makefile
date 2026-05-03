# ==============================================================================
# Chapaty Template: Quick Actions
# ==============================================================================
# Usage: make <target>
#
# Targets:
#   setup   : One-time install (compiles Rust release build + Python deps)
#   run     : Runs the backtest and generates the QuantStats HTML tearsheet
#   update  : Refreshes LLM prompts + viz script, bumps chapaty crate
#   check   : Runs fmt, clippy, and tests (./bin/pre-push.sh hook)
#   doctor  : Validates required dependencies (Rust + Python)
#   clean   : Removes build artifacts, reports, and the Python venv
# ==============================================================================

.PHONY: setup run update check doctor clean

# Public repo used by `make update` to pull the latest prompts.
TEMPLATE_REPO ?= https://raw.githubusercontent.com/LenWilliamson/chapaty-template/refs/heads/main
PYTHON_VENV   := .venv
VENV_PIP      := $(PYTHON_VENV)/bin/pip
VENV_PYTHON   := $(PYTHON_VENV)/bin/python

setup: doctor
	@echo ">> Building chapaty in release mode (first run downloads ~2 min of deps)..."
	cargo build --release
	@echo ">> Creating Python virtual environment in $(PYTHON_VENV)..."
	python3 -m venv $(PYTHON_VENV)
	@echo ">> Upgrading pip..."
	$(VENV_PYTHON) -m pip install --upgrade pip --quiet
	@echo ">> Installing Python visualization dependencies..."
	$(VENV_PIP) install -r visualization/requirements.txt --quiet
	@echo ">> Setup complete. Run 'make run' to backtest."

run:
	@if [ ! -d "$(PYTHON_VENV)" ]; then \
		echo "ERROR: Python virtual environment not found. Please run 'make setup' first."; \
		exit 1; \
	fi
	@echo ">> Running Chapaty backtest..."
	cargo run --release
	@echo ">> Generating QuantStats tearsheet..."
	$(VENV_PYTHON) visualization/generate_tearsheet.py
	@echo ">> Run completed."

update:
	@echo ">> [1/3] Syncing LLM prompts from $(TEMPLATE_REPO)..."
	@mkdir -p .ai
	curl -fsSL $(TEMPLATE_REPO)/AI.md                      -o AI.md
	curl -fsSL $(TEMPLATE_REPO)/.ai/agent-plan.md          -o .ai/agent-plan.md
	curl -fsSL $(TEMPLATE_REPO)/.ai/chapaty-api.md         -o .ai/chapaty-api.md
	curl -fsSL $(TEMPLATE_REPO)/.ai/rust-vibe-rules.md     -o .ai/rust-vibe-rules.md
	curl -fsSL $(TEMPLATE_REPO)/.ai/algorithm-ideas.md     -o .ai/algorithm-ideas.md
	@echo ">> [2/3] Syncing visualization script..."
	curl -fsSL $(TEMPLATE_REPO)/visualization/generate_tearsheet.py -o visualization/generate_tearsheet.py
	curl -fsSL $(TEMPLATE_REPO)/visualization/requirements.txt      -o visualization/requirements.txt
	@echo ">> [3/3] Updating Rust dependencies..."
	cargo update
	@echo ">> Update complete. If your agent fails to compile, paste the error into your LLM or reach out on Discord."

check:
	@echo ">> Running pre-push checks..."
	./bin/pre-push.sh

doctor:
	@echo ">> Checking required dependencies..."
	@echo ""
	@if command -v rustc > /dev/null 2>&1; then \
		echo "  Rust:   $$(rustc --version)"; \
	else \
		echo "  Rust:   NOT FOUND"; \
		echo "            Install via: https://www.rust-lang.org/tools/install"; \
		MISSING=1; \
	fi
	@if command -v python3 > /dev/null 2>&1; then \
		echo "  Python: $$(python3 --version)"; \
	else \
		echo "  Python: NOT FOUND"; \
		echo "            Install via: https://github.com/pyenv/pyenv#installation"; \
		MISSING=1; \
	fi
	@echo ""
	@if ! command -v rustc > /dev/null 2>&1 || ! command -v python3 > /dev/null 2>&1; then \
		echo "ERROR: One or more required dependencies are missing. See above."; \
		exit 1; \
	fi
	@echo ">> All required dependencies found."

clean:
	@echo ">> Cleaning Rust artifacts and reports..."
	cargo clean
	rm -rf reports chapaty/reports
	@echo ">> Removing Python virtual environment..."
	rm -rf $(PYTHON_VENV)
