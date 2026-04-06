build:
	go build -o bin/mkrk .

test:
	go test ./...

clean:
	rm -rf bin/

.PHONY: build test clean
