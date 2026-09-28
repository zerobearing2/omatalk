.PHONY: test lint format clean bump release

test:
	cargo test --locked

lint:
	cargo fmt --check
	cargo clippy --locked --all-targets -- -D warnings

format:
	cargo fmt

clean:
	cargo clean
	rm -f omatalk-x86_64.tar.gz omatalk-x86_64.tar.gz.sha256

# Version file only, no commit. Then make release.
bump:
	scripts/bump.sh

release:
	scripts/release.sh
