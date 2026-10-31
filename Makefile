.PHONY: test lint format clean rc release

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

# Both run from a clean master equal to origin/master. They publish a tag and
# a GitHub Release, and never push to master.
# make rc VERSION=0.10.0       publishes v0.10.0-rc.N as a prerelease
# make release VERSION=0.10.0  publishes v0.10.0 as latest, after an RC of this master
rc:
	VERSION="$(VERSION)" scripts/release.sh rc

release:
	VERSION="$(VERSION)" scripts/release.sh stable
