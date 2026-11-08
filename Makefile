# Homebrew's rustup is keg-only, so cargo may not be on the PATH.
export PATH := /opt/homebrew/opt/rustup/bin:$(HOME)/.cargo/bin:$(PATH)
# macOS's make 3.81 ignores that export for commands it runs without a shell.
CARGO := env PATH="$(PATH)" cargo
RUSTUP := env PATH="$(PATH)" rustup

IMAGE ?= tracker-online
# Extra client arguments, e.g. make client ARGS="compact --room test"
ARGS ?=

.DEFAULT_GOAL := help
.PHONY: help server client web build-web test lint docker docker-run clean

help:
	@echo "make server      Run the server on http://127.0.0.1:3000"
	@echo "make client      Run the desktop client (ARGS=\"compact --room test\")"
	@echo "make web         Build the web client and serve it on http://127.0.0.1:3000"
	@echo "make test        Run the tests"
	@echo "make lint        Check formatting and run clippy"
	@echo "make docker      Build the server image"
	@echo "make docker-run  Run the server image on port 3000"
	@echo "make clean       Remove build outputs"

server:
	$(CARGO) run --release -p tracker-server

client:
	$(CARGO) run --release -p tracker-client -- $(ARGS)

web: build-web server

build-web:
	$(RUSTUP) target add wasm32-unknown-unknown
	command -v trunk >/dev/null || $(CARGO) install --locked trunk
	cd crates/client && trunk build --release

test:
	$(CARGO) test --workspace

lint:
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(CARGO) clippy -p tracker-client --target wasm32-unknown-unknown -- -D warnings

docker:
	docker build -t $(IMAGE) .

docker-run:
	docker run --rm -p 3000:3000 $(IMAGE)

clean:
	$(CARGO) clean
	rm -rf dist
