OMATALK_HOME ?= $(HOME)/.local/share/omatalk
REPO := $(CURDIR)

PLUGIN ?= plugin
PLUGIN_DIR ?= $(HOME)/.config/omarchy/plugins/zerobearing.omatalk
PLUGIN_GH ?= zerobearing2/omarchy-omatalk-plugin
# omatalk tag (vX.Y.Z) or 40-char commit for plugin-pin. Empty = latest GitHub release.
REF ?=

.PHONY: test lint format clean dev-install dev-restart dev-uninstall bump release \
	plugin-test plugin-validate plugin-dev-reload plugin-pin plugin-pin-release \
	plugin-bump plugin-bump-manifest plugin-release

test:
	uv run --group dev pytest tests/

# Check-only: fails on any lint violation, changes nothing.
lint:
	uv run --group dev ruff check .

# Rewrites files in place. Run before opening a PR — see AGENTS.md.
format:
	uv run --group dev ruff format .

clean:
	rm -rf build dist .pytest_cache
	rm -rf *.egg-info

# Bump pyproject.toml's version and commit it (not pushed — push yourself
# when ready). `make bump` increments the patch; `make bump VERSION=0.3.0`
# sets that exact version instead. This commit is what `release` (below) and
# the Release workflow read the version from, so bump and push *before*
# releasing, not as part of releasing.
bump:
	@current=$$(sed -n 's/^version = "\(.*\)"$$/\1/p' pyproject.toml); \
	if [ -n "$(VERSION)" ]; then \
		new="$(VERSION)"; \
	else \
		major=$$(echo "$$current" | cut -d. -f1); \
		minor=$$(echo "$$current" | cut -d. -f2); \
		patch=$$(echo "$$current" | cut -d. -f3); \
		new="$$major.$$minor.$$((patch + 1))"; \
	fi; \
	sed -i "s/^version = \".*\"/version = \"$$new\"/" pyproject.toml; \
	git add pyproject.toml; \
	git commit -m "Bump version to $$new"; \
	echo "Bumped $$current -> $$new (commit made — push when ready)"

# Trigger the Release workflow (manual-only, see .github/workflows/release.yml).
# It releases whatever version is already committed in pyproject.toml on the
# remote's default branch, so `make bump` (and push) first.
release:
	gh workflow run release.yml
	@echo "Triggered. Watch with: gh run watch \$$(gh run list --workflow=release.yml -L1 --json databaseId -q '.[0].databaseId')"
	@echo "After it lands: check whether the bar plugin needs re-pinning (see AGENTS.md 'Release')"

# Point the installed Daemon at this checkout instead of the last released
# tarball. Keeps the existing venv/models — swaps in an editable package
# install, so `dev-restart` is all that's needed after that for ordinary
# Python edits. QML is the plugin/ submodule; use plugin-dev-reload.
dev-install:
	systemctl --user stop omatalk.service
	uv pip install --quiet --python "$(OMATALK_HOME)/venv/bin/python" -e "$(REPO)"
	systemctl --user start omatalk.service
	@echo "Dev install active: Daemon runs from $(REPO)"

# After editing daemon/*.py: restart the Daemon to pick up the change.
# No reinstall needed — dev-install's editable install already points here.
dev-restart:
	systemctl --user restart omatalk.service

# Undo dev-install: restore the official released build.
dev-uninstall:
	systemctl --user stop omatalk.service
	./install.sh

# --- bar plugin (plugin/ submodule) ---

plugin-test:
	$(PLUGIN)/tests/run.sh

plugin-validate:
	omarchy plugin validate "$(abspath $(PLUGIN))"

plugin-dev-reload:
	omarchy plugin disable zerobearing.omatalk >/dev/null 2>&1 || true
	mkdir -p "$(PLUGIN_DIR)"
	rsync -a --delete \
		--exclude .git --exclude tests --exclude .github \
		"$(abspath $(PLUGIN))/" "$(PLUGIN_DIR)/"
	omarchy restart shell
	omarchy plugin enable zerobearing.omatalk >/dev/null 2>&1 || true

plugin-pin:
	./scripts/pin-installer.sh "$(REF)"

# Pin install.sh, bump plugin version, one commit in plugin/. Working tree
# must be clean. Does not push — git -C plugin push, then plugin-release.
plugin-pin-release:
	@if [ -n "$$(git -C $(PLUGIN) status --porcelain)" ]; then echo "plugin/ working tree must be clean" >&2; exit 1; fi
	./scripts/pin-installer.sh "$(REF)"
	@if git -C $(PLUGIN) diff --quiet -- Panel.qml; then echo "installer pin already current" >&2; exit 1; fi
	$(MAKE) plugin-bump-manifest
	$(MAKE) plugin-test
	@new=$$(sed -n 's/^  "version": "\(.*\)",$$/\1/p' $(PLUGIN)/manifest.json); \
	commit=$$(sed -n 's/^  readonly property string installerUrl: "https:\/\/raw.githubusercontent.com\/zerobearing2\/omatalk\/\([0-9a-f]\{40\}\)\/install.sh"$$/\1/p' $(PLUGIN)/Panel.qml); \
	git -C $(PLUGIN) add Panel.qml manifest.json; \
	git -C $(PLUGIN) commit -m "Pin omatalk installer $$commit and bump to $$new"; \
	echo "Committed $$new (pin $$commit). git -C plugin push, then make plugin-release."

plugin-bump-manifest:
	@current=$$(sed -n 's/^  "version": "\(.*\)",$$/\1/p' $(PLUGIN)/manifest.json); \
	if [ -z "$$current" ]; then echo "could not read version from plugin/manifest.json" >&2; exit 1; fi; \
	if [ -n "$(VERSION)" ]; then \
		new="$(VERSION)"; \
	else \
		major=$$(echo "$$current" | cut -d. -f1); \
		minor=$$(echo "$$current" | cut -d. -f2); \
		patch=$$(echo "$$current" | cut -d. -f3); \
		new="$$major.$$minor.$$((patch + 1))"; \
	fi; \
	sed -i "s/^  \"version\": \".*\",$$/  \"version\": \"$$new\",/" $(PLUGIN)/manifest.json; \
	echo "$$current -> $$new"

plugin-bump: plugin-bump-manifest
	@new=$$(sed -n 's/^  "version": "\(.*\)",$$/\1/p' $(PLUGIN)/manifest.json); \
	git -C $(PLUGIN) add manifest.json; \
	git -C $(PLUGIN) commit -m "Bump version to $$new"; \
	echo "Bumped to $$new (commit in plugin/ — git -C plugin push when ready)"

plugin-release:
	gh --repo $(PLUGIN_GH) workflow run release.yml
	@echo "Triggered. Watch with: gh --repo $(PLUGIN_GH) run watch \$$(gh --repo $(PLUGIN_GH) run list --workflow=release.yml -L1 --json databaseId -q '.[0].databaseId')"
