OMATALK_HOME ?= $(HOME)/.local/share/omatalk
REPO := $(CURDIR)
PLUGIN := plugin
PLUGIN_DIR ?= $(HOME)/.config/omarchy/plugins/zerobearing.omatalk

.PHONY: test lint format clean bump build verify release \
	plugin-bump plugin-build plugin-verify plugin-release \
	plugin-test plugin-validate plugin-dev-reload \
	dev-install dev-restart dev-uninstall

test:
	uv run --group dev pytest tests/

lint:
	uv run --group dev ruff check .

format:
	uv run --group dev ruff format .

clean:
	rm -f omatalk-src.tar.gz omatalk-src.tar.gz.sha256
	rm -rf build dist .pytest_cache .ruff_cache
	rm -rf *.egg-info
	find . -path ./.venv -prune -o -type d -name __pycache__ -print0 | xargs -0 -r rm -rf
	find . -path ./.venv -prune -o -type f \( -name '*.pyc' -o -name '*.pyo' \) -print0 | xargs -0 -r rm -f

# Version file only, no commit. Then make release / make plugin-release.
bump:
	scripts/bump.sh

build:
	scripts/build.sh

verify:
	scripts/verify.sh

# build + verify + commit + push + gh release create
release:
	scripts/release.sh

plugin-bump:
	scripts/plugin-bump.sh

plugin-build:
	scripts/plugin-build.sh

plugin-verify:
	scripts/plugin-verify.sh

plugin-release:
	scripts/plugin-release.sh

plugin-test: plugin-verify

plugin-validate:
	omarchy plugin validate "$(abspath $(PLUGIN))"

plugin-dev-reload:
	omarchy plugin disable zerobearing.omatalk >/dev/null 2>&1 || true
	mkdir -p "$(PLUGIN_DIR)"
	rsync -a --delete --exclude .git --exclude tests --exclude .github \
		"$(abspath $(PLUGIN))/" "$(PLUGIN_DIR)/"
	omarchy restart shell
	omarchy plugin enable zerobearing.omatalk >/dev/null 2>&1 || true

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
