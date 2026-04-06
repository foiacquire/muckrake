GO_SRC := $(shell find . -name '*.go' -not -path './bin/*')

bin/mkrk: $(GO_SRC) go.mod go.sum
	go build -o $@ .

test:
	go test ./...

clean:
	rm -rf bin/

.PHONY: test clean
