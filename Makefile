OMATALK_HOME ?= $(HOME)/.local/share/omatalk
REPO := $(CURDIR)
PLUGIN := plugin
PLUGIN_DIR ?= $(HOME)/.config/omarchy/plugins/zerobearing.omatalk
PLUGIN_GH := zerobearing2/omarchy-omatalk-plugin

.PHONY: test lint format clean bump release \
	dev-install dev-restart dev-uninstall \
	on-pushed-master pack pin verify-pin pin-release \
	plugin-ready plugin-on-master \
	plugin-test plugin-validate plugin-dev-reload \
	plugin-vendor plugin-set-version plugin-bump plugin-release

test:
	uv run --group dev pytest tests/

lint:
	uv run --group dev ruff check .

format:
	uv run --group dev ruff format .

clean:
	rm -rf build dist .pytest_cache
	rm -rf *.egg-info

# Bump pyproject.toml and commit. Then `make pin`, commit install.sh, push,
# `make release`.
bump:
	@current=$$(sed -n 's/^version = "\(.*\)"$$/\1/p' pyproject.toml); \
	if [ -n "$(VERSION)" ]; then new="$(VERSION)"; else \
		major=$$(echo "$$current" | cut -d. -f1); \
		minor=$$(echo "$$current" | cut -d. -f2); \
		patch=$$(echo "$$current" | cut -d. -f3); \
		new="$$major.$$minor.$$((patch + 1))"; \
	fi; \
	sed -i "s/^version = \".*\"/version = \"$$new\"/" pyproject.toml; \
	git add pyproject.toml; \
	git commit -m "Bump version to $$new"; \
	echo "Bumped $$current -> $$new (push when ready)"

on-pushed-master:
	@test "$$(git rev-parse --abbrev-ref HEAD)" = master || { echo "need master" >&2; exit 1; }
	@test "$$(git rev-parse HEAD)" = "$$(git rev-parse origin/master)" || { echo "push master first" >&2; exit 1; }
	@git diff --quiet HEAD -- install.sh || { echo "commit install.sh first" >&2; exit 1; }

pack:
	scripts/pack-src.sh omatalk-src.tar.gz
	sha256sum omatalk-src.tar.gz > omatalk-src.tar.gz.sha256

pin:
	scripts/pin-install.sh

verify-pin:
	scripts/verify-pin.sh

# Copy the pinned installer into the plugin and bump it. Daemon releases
# do not do this; run it when Install should ship a newer first-time Daemon.
pin-release: pin plugin-vendor
	@echo "Pinned and vendored. Commit install.sh here, git -C plugin push, then make plugin-release."

plugin-ready:
	@test -e $(PLUGIN)/.git -a -f $(PLUGIN)/Panel.qml || { echo "git submodule update --init" >&2; exit 1; }
	@test "$$(git -C $(PLUGIN) rev-parse --show-toplevel)" = "$(abspath $(PLUGIN))" || { echo "plugin/ is not the submodule" >&2; exit 1; }

plugin-on-master: plugin-ready
	@test -z "$$(git -C $(PLUGIN) status --porcelain)" || { echo "plugin/ working tree must be clean" >&2; exit 1; }
	@git -C $(PLUGIN) switch --quiet master

# Daemon only. Pin must already match this tree (make pin after make bump).
# The workflow packs the same way, checks the committed digest, and creates
# the GitHub release; it does not clobber an existing tag.
release: on-pushed-master verify-pin
	gh workflow run release.yml --ref master
	@echo "Triggered. Watch with: gh run watch \$$(gh run list --workflow=release.yml -L1 --json databaseId -q '.[0].databaseId')"

dev-install:
	systemctl --user stop omatalk.service
	uv pip install --quiet --python "$(OMATALK_HOME)/venv/bin/python" -e "$(REPO)"
	systemctl --user start omatalk.service
	@echo "Dev install active: Daemon runs from $(REPO)"

dev-restart:
	systemctl --user restart omatalk.service

dev-uninstall:
	systemctl --user stop omatalk.service
	./install.sh

plugin-test: plugin-ready
	$(PLUGIN)/tests/run.sh

plugin-validate: plugin-ready
	omarchy plugin validate "$(abspath $(PLUGIN))"

plugin-dev-reload: plugin-ready
	omarchy plugin disable zerobearing.omatalk >/dev/null 2>&1 || true
	mkdir -p "$(PLUGIN_DIR)"
	rsync -a --delete --exclude .git --exclude tests --exclude .github \
		"$(abspath $(PLUGIN))/" "$(PLUGIN_DIR)/"
	omarchy restart shell
	omarchy plugin enable zerobearing.omatalk >/dev/null 2>&1 || true

# Copy this repo's install.sh into plugin/ and bump the plugin. Root file
# is the original; do not edit plugin/install.sh by hand.
plugin-vendor: plugin-on-master
	@cp -f install.sh $(PLUGIN)/install.sh
	$(MAKE) plugin-set-version
	$(MAKE) plugin-test
	@new=$$(sed -n 's/^  "version": "\(.*\)",$$/\1/p' $(PLUGIN)/manifest.json); \
	git -C $(PLUGIN) add install.sh manifest.json; \
	git -C $(PLUGIN) commit -m "Vendor install.sh and bump to $$new"; \
	echo "Committed plugin $$new"

plugin-set-version:
	@current=$$(sed -n 's/^  "version": "\(.*\)",$$/\1/p' $(PLUGIN)/manifest.json); \
	if [ -z "$$current" ]; then echo "could not read plugin version" >&2; exit 1; fi; \
	if [ -n "$(VERSION)" ]; then new="$(VERSION)"; else \
		major=$$(echo "$$current" | cut -d. -f1); \
		minor=$$(echo "$$current" | cut -d. -f2); \
		patch=$$(echo "$$current" | cut -d. -f3); \
		new="$$major.$$minor.$$((patch + 1))"; \
	fi; \
	sed -i "s/^  \"version\": \".*\",$$/  \"version\": \"$$new\",/" $(PLUGIN)/manifest.json; \
	echo "$$current -> $$new"

plugin-bump: plugin-on-master
	$(MAKE) plugin-set-version
	@new=$$(sed -n 's/^  "version": "\(.*\)",$$/\1/p' $(PLUGIN)/manifest.json); \
	git -C $(PLUGIN) add manifest.json; \
	git -C $(PLUGIN) commit -m "Bump version to $$new"; \
	echo "Bumped to $$new (git -C plugin push when ready)"

plugin-release:
	gh --repo $(PLUGIN_GH) workflow run release.yml
	@echo "Triggered. Watch with: gh --repo $(PLUGIN_GH) run watch \$$(gh --repo $(PLUGIN_GH) run list --workflow=release.yml -L1 --json databaseId -q '.[0].databaseId')"
