.PHONY: rel test test-int128 clean

rel:
	cargo build --release

test: rel
	cargo test --release -q
	$(MAKE) -C test

# The suite with Int = i128 (plan/NUMERIC.design.txt section 6). Built in its
# own target directory, so that target/release/pluk stays the default build; the
# binary is kept as target/release/$(PLUK128).
PLUK128 = pluk7

test-int128:
	cargo build --release --features int128 --target-dir target/int128
	mkdir -p target/release
	cp target/int128/release/pluk target/release/$(PLUK128)
	cargo test --release -q --features int128 --target-dir target/int128
	$(MAKE) -C test p=../target/release/$(PLUK128)

clean:
	$(MAKE) -C test clean
