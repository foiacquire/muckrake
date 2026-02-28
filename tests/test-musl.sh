#!/bin/sh
set -eu

MUSL_TARGET="x86_64-unknown-linux-musl"
BINARY="target/${MUSL_TARGET}/release/mkrk"
CONTAINER_RT="${CONTAINER_RT:-$(command -v podman 2>/dev/null || command -v docker 2>/dev/null || true)}"

if [ -z "$CONTAINER_RT" ]; then
    echo "SKIP: no container runtime (podman or docker) found" >&2
    exit 0
fi

if [ ! -f "$BINARY" ]; then
    echo "Building musl binary..." >&2
    "$CONTAINER_RT" run --rm -v "$(pwd):/src:Z" -w /src rust:latest sh -c "
        rustup target add $MUSL_TARGET && \
        apt-get update -qq && apt-get install -y -qq musl-tools >/dev/null 2>&1 && \
        cargo build --release --target $MUSL_TARGET"
fi

file_output=$(file "$BINARY")
case "$file_output" in
    *"statically linked"*|*"static-pie linked"*)
        echo "OK: binary is statically linked" >&2
        ;;
    *)
        echo "FAIL: binary is not statically linked" >&2
        echo "  $file_output" >&2
        exit 1
        ;;
esac

echo "Running smoke test in Alpine container..." >&2
"$CONTAINER_RT" run --rm \
    -v "$(pwd)/$BINARY:/usr/local/bin/mkrk:ro" \
    alpine:latest \
    sh -c '
        set -eu
        mkdir -p /tmp/testproject && cd /tmp/testproject
        mkrk --version
        mkrk init -n
        test -f .mkrk
        mkrk category add docs --pattern "*.txt"
        echo "smoke test content" > doc.txt
        mkrk ingest
        mkrk list
        mkrk verify
        mkrk tag doc.txt smoke
        mkrk tags doc.txt --no-hash-check
    '

echo "PASS: musl smoke test" >&2
