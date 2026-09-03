OMATALK_HOME ?= $(HOME)/.local/share/omatalk
REPO := $(CURDIR)

.PHONY: test clean dev-install dev-restart dev-uninstall bump release

test:
	uv run --group dev pytest tests/

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

# Point the installed daemon at this checkout instead of the last released
# tarball. Keeps the existing venv/models — swaps in an editable package
# install, so `dev-restart` is all that's needed after that for ordinary
# Python edits. The bar plugin is the other repo
# (zerobearing2/omarchy-omatalk-plugin); this target does not copy QML.
dev-install:
	systemctl --user stop omatalk.service
	uv pip install --quiet --python "$(OMATALK_HOME)/venv/bin/python" -e "$(REPO)"
	systemctl --user start omatalk.service
	@echo "Dev install active: daemon runs from $(REPO)"

# After editing omatalk/*.py: restart the daemon to pick up the change.
# No reinstall needed — dev-install's editable install already points here.
dev-restart:
	systemctl --user restart omatalk.service

# Undo dev-install: restore the official released build.
dev-uninstall:
	systemctl --user stop omatalk.service
	./install.sh
