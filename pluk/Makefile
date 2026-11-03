.PHONY: rel test clean

rel:
	cargo build --release

test: rel
	cargo test --release -q
	$(MAKE) -C test

clean:
	$(MAKE) -C test clean
