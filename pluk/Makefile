.PHONY: rel test test-int128 clean

rel:
	cargo build --release

test: rel
	cargo test --release -q
	$(MAKE) -C test

# The suite with Int = i128 (plan/NUMERIC.design.txt section 6). The release
# binary is rebuilt with the default features by the next 'make test'.
test-int128:
	cargo build --release --features int128
	cargo test --release -q --features int128
	$(MAKE) -C test

clean:
	$(MAKE) -C test clean
