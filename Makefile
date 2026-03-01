MUSL_TARGET := x86_64-unknown-linux-musl
CONTAINER_RT := $(shell command -v docker 2>/dev/null || command -v podman 2>/dev/null)

release:
	$(CONTAINER_RT) run --rm -v "$(CURDIR):/src:Z" -w /src rust:latest sh -c '\
		rustup target add $(MUSL_TARGET) && \
		apt-get update -qq && apt-get install -y -qq musl-tools >/dev/null 2>&1 && \
		cargo build --release --target $(MUSL_TARGET)'
	@file target/$(MUSL_TARGET)/release/mkrk
