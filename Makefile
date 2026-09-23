OMATALK_HOME ?= $(HOME)/.local/share/omatalk
REPO := $(CURDIR)

.PHONY: test lint format clean bump release \
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

# Version file only, no commit. Then make release.
bump:
	scripts/bump.sh

release:
	scripts/release.sh

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
