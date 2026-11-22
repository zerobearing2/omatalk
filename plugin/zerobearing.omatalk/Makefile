.PHONY: test bump release

test:
	tests/run.sh

# Version in manifest.json only, no commit. Then make release.
bump:
	scripts/bump.sh

release:
	scripts/release.sh
