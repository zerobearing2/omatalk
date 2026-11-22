.PHONY: test test-plugin lint format clean rc release

test:
	cargo test --locked

test-plugin:
	plugin/zerobearing.omatalk/tests/run.sh

lint:
	cargo fmt --check
	cargo clippy --locked --all-targets -- -D warnings

format:
	cargo fmt

clean:
	cargo clean
	rm -f omatalk-x86_64.tar.gz omatalk-x86_64.tar.gz.sha256

# Both run from a clean master or release-X.Y.x equal to its origin branch.
# They publish a tag and a GitHub Release, and never push to a branch.
# make rc VERSION=0.10.0       publishes v0.10.0-rc.N as a prerelease
# make release VERSION=0.10.0  publishes v0.10.0; an RC first is optional
rc:
	VERSION="$(VERSION)" scripts/release.sh rc

release:
	VERSION="$(VERSION)" scripts/release.sh stable
