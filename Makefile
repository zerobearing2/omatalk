OMATALK_HOME ?= $(HOME)/.local/share/omatalk
PLUGIN_DIR ?= $(HOME)/.config/omarchy/plugins/zerobearing.omatalk
REPO := $(CURDIR)

.PHONY: test clean dev-install dev-restart dev-plugin-reload dev-uninstall

test:
	uv run --group dev pytest tests/

clean:
	rm -rf build dist .pytest_cache
	rm -rf *.egg-info

# One-time (or after a dependency change): point the installed daemon and
# bar plugin at this checkout instead of the last released tarball. Keeps
# the existing venv/models — swaps in an editable package install, so
# `dev-restart` is all that's needed after that for ordinary Python edits.
#
# The plugin dir is copied, not symlinked: Quickshell's QML loader compares
# a requested path against its canonicalized (symlink-resolved) form to
# catch case mismatches, and a symlinked plugin dir resolves to a totally
# different absolute path, which trips that check into a false positive
# ("File name case mismatch") even though the real file names match exactly.
dev-install:
	omarchy plugin disable zerobearing.omatalk >/dev/null 2>&1 || true
	systemctl --user stop omatalk.service
	uv pip install --quiet --python "$(OMATALK_HOME)/venv/bin/python" -e "$(REPO)"
	systemctl --user start omatalk.service
	$(MAKE) dev-plugin-reload
	@echo "Dev install active: daemon + plugin now run from $(REPO)"

# After editing omatalk/*.py: restart the daemon to pick up the change.
# No reinstall needed — dev-install's editable install already points here.
dev-restart:
	systemctl --user restart omatalk.service

# After editing anything under plugin/: re-copy it into place and restart
# the shell. `omarchy plugin disable`/`enable`/`rescanPlugins` report success
# but are not reliable here — confirmed by watching the shell's own log
# (`quickshell log --pid <pid> -r "*=true"`): the very first reload after a
# shell restart takes effect, but repeated disable/enable cycles on an
# already-loaded plugin silently stop re-triggering "Local plugin changed,
# reloading" at all, leaving the mounted widget on stale QML with no error.
# A full shell restart has been 100% reliable every time; nothing lighter
# has been, so this trades speed for actually working.
dev-plugin-reload:
	omarchy plugin disable zerobearing.omatalk >/dev/null 2>&1 || true
	rm -rf "$(PLUGIN_DIR)"
	cp -r "$(REPO)/plugin" "$(PLUGIN_DIR)"
	omarchy restart shell
	omarchy plugin enable zerobearing.omatalk >/dev/null 2>&1 || true

# Undo dev-install: restore the official released build.
dev-uninstall:
	omarchy plugin disable zerobearing.omatalk >/dev/null 2>&1 || true
	systemctl --user stop omatalk.service
	rm -rf "$(PLUGIN_DIR)"
	./install.sh
