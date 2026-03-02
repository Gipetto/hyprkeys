.PHONY: build

build:
	cargo build --release

run:
	./target/release/hyprkeys -t light

