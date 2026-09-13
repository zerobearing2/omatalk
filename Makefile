OMATALK_HOME ?= $(HOME)/.local/share/omatalk
REPO := $(CURDIR)
PLUGIN := plugin
PLUGIN_DIR ?= $(HOME)/.config/omarchy/plugins/zerobearing.omatalk
PLUGIN_GH := zerobearing2/omarchy-omatalk-plugin

.PHONY: test lint format clean bump release \
	dev-install dev-restart dev-uninstall \
	on-pushed-master plugin-ready plugin-on-master \
	plugin-test plugin-validate plugin-dev-reload \
	plugin-pin plugin-pin-release plugin-set-version plugin-bump plugin-release

test:
	uv run --group dev pytest tests/

lint:
	uv run --group dev ruff check .

format:
	uv run --group dev ruff format .

clean:
	rm -rf build dist .pytest_cache
	rm -rf *.egg-info

# Bump pyproject.toml and commit. Push, then `make release`.
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

plugin-ready:
	@test -e $(PLUGIN)/.git -a -f $(PLUGIN)/Panel.qml || { echo "git submodule update --init" >&2; exit 1; }
	@test "$$(git -C $(PLUGIN) rev-parse --show-toplevel)" = "$(abspath $(PLUGIN))" || { echo "plugin/ is not the submodule" >&2; exit 1; }

plugin-on-master: plugin-ready
	@test -z "$$(git -C $(PLUGIN) status --porcelain)" || { echo "plugin/ working tree must be clean" >&2; exit 1; }
	@git -C $(PLUGIN) switch --quiet master

# Re-pin plugin if install.sh changed, then cut the Daemon release from origin/master.
release: plugin-ready on-pushed-master
	@hash=$$(git show HEAD:install.sh | sha256sum | awk '{print $$1}'); \
	pinned=$$(sed -n 's/^  readonly property string installerSha256: "\(.*\)"$$/\1/p' $(PLUGIN)/Panel.qml); \
	if [ "$$hash" != "$$pinned" ]; then $(MAKE) plugin-pin-release; fi
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

plugin-pin: plugin-ready on-pushed-master
	@hash=$$(git show HEAD:install.sh | sha256sum | awk '{print $$1}'); \
	pinned=$$(sed -n 's/^  readonly property string installerSha256: "\(.*\)"$$/\1/p' $(PLUGIN)/Panel.qml); \
	if [ "$$hash" = "$$pinned" ]; then echo "plugin pin current"; exit 0; fi; \
	commit=$$(git rev-parse HEAD); \
	url="https://raw.githubusercontent.com/zerobearing2/omatalk/$$commit/install.sh"; \
	sed -i \
		-e "s|^  readonly property string installerUrl: \".*\"$$|  readonly property string installerUrl: \"$$url\"|" \
		-e "s|^  readonly property string installerSha256: \".*\"$$|  readonly property string installerSha256: \"$$hash\"|" \
		$(PLUGIN)/Panel.qml; \
	echo "Pinned $$commit $$hash"

plugin-pin-release: plugin-on-master
	$(MAKE) plugin-pin
	@git -C $(PLUGIN) diff --quiet -- Panel.qml && { echo "installer pin already current"; exit 1; }
	$(MAKE) plugin-set-version
	$(MAKE) plugin-test
	@new=$$(sed -n 's/^  "version": "\(.*\)",$$/\1/p' $(PLUGIN)/manifest.json); \
	commit=$$(sed -n 's/^  readonly property string installerUrl: "https:\/\/raw.githubusercontent.com\/zerobearing2\/omatalk\/\([0-9a-f]\{40\}\)\/install.sh"$$/\1/p' $(PLUGIN)/Panel.qml); \
	git -C $(PLUGIN) add Panel.qml manifest.json; \
	git -C $(PLUGIN) commit -m "Pin omatalk installer $$commit and bump to $$new"; \
	echo "Committed $$new (pin $$commit). git -C plugin push, then make plugin-release."

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
