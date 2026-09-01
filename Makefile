.PHONY: test clean

test:
	uv run --group dev pytest tests/

clean:
	rm -rf build dist .pytest_cache
	rm -rf *.egg-info
	rm -f server.pid
