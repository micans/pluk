.PHONY: rel test clean

rel:
	cargo build --release

test: rel
	$(MAKE) -C test

clean:
	$(MAKE) -C test clean
