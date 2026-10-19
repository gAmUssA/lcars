# LCARS: Ops daemon + Stream Deck plugin. `make help` lists targets.
SHELL := /bin/bash
OPS_DIR := packages/ops
PLUGIN_DIR := packages/plugin
PLUGIN_UUID := io.gamov.lcars
LABEL := io.gamov.lcars.ops
PLIST := $(HOME)/Library/LaunchAgents/$(LABEL).plist

.PHONY: help build test pack install install-skill start stop status restart uninstall

help: ## Show targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F ':.*## ' '{printf "  %-14s %s\n", $$1, $$2}'

build: ## Release build of Ops and a bundled plugin
	cd $(OPS_DIR) && cargo build --release
	cd $(PLUGIN_DIR) && npm ci && npm run build

test: ## Rust, TypeScript and shell test suites, plus the version check
	scripts/check-versions.sh
	cd $(OPS_DIR) && cargo test
	cd $(PLUGIN_DIR) && npm test
	scripts/run-tests.sh

pack: ## Create $(PLUGIN_UUID).streamDeckPlugin under packages/plugin/dist
	cd $(PLUGIN_DIR) && npm run build && mkdir -p dist && streamdeck pack $(PLUGIN_UUID).sdPlugin -f -o dist

install: ## Install Ops binary, config (if absent) and the launchd plist; does not start it
	scripts/install.sh

install-skill: ## Symlink the lcars skill into ~/.claude/skills
	mkdir -p $(HOME)/.claude/skills
	ln -sfn $(CURDIR)/packages/skill/lcars $(HOME)/.claude/skills/lcars
	@echo "linked ~/.claude/skills/lcars"

start: ## Load the launchd agent (starts Ops now and at login)
	launchctl bootstrap gui/$$(id -u) $(PLIST)
	@$(MAKE) --no-print-directory status

stop: ## Unload the launchd agent
	launchctl bootout gui/$$(id -u)/$(LABEL)

restart: stop start ## Reload the launchd agent

status: ## Show agent state and the last log lines
	@launchctl print gui/$$(id -u)/$(LABEL) 2>/dev/null | grep -E 'state|pid' | head -3 || echo "$(LABEL) is not loaded"
	@tail -n 5 $(HOME)/Library/Logs/lcars/ops.log 2>/dev/null || true

uninstall: ## Unload and remove the agent, binary and skill link (keeps ops.toml)
	-launchctl bootout gui/$$(id -u)/$(LABEL) 2>/dev/null
	rm -f $(PLIST) $(HOME)/.local/bin/ops $(HOME)/.claude/skills/lcars
