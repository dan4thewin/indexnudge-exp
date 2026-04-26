GOFLAGS=-gcflags=all=-l

rust:V:
	cd rust && cargo build --release

rust-test:V:
	cd rust && cargo test

go:V:
	cd go && go build -o indexnudge .

go-test:V:
	cd go && go test -count=1 ./...

all:V: rust go
	echo 'Build complete'

test:V: rust-test go-test shim-test
	echo 'ALL PASSED'

shim-test:V: rust go
	rust/target/release/indexnudge rust
	rust/target/release/indexnudge --freshen rust 2>&1 | grep -q 'up to date'
	echo 'PASS: rust freshen no-op'
	go/indexnudge -dir go -output go/.indexnudge.db
	echo 'PASS: go full index'
	/usr/bin/bash bin/head -20 rust/src/main.rs | grep -q 'sqlite3'
	echo 'PASS: head shim (rust)'
	/usr/bin/bash bin/sed -n '1,10p' rust/src/main.rs | grep -q 'sqlite3'
	echo 'PASS: sed shim (rust)'
	/usr/bin/bash bin/grep create_schema rust/src/main.rs | grep -q 'sqlite3'
	echo 'PASS: grep shim (rust)'
	/usr/bin/bash bin/cat rust/src/main.rs | grep -q 'sqlite3'
	echo 'PASS: cat shim (rust)'
	/usr/bin/bash bin/tail -5 rust/src/main.rs | grep -q 'sqlite3'
	echo 'PASS: tail shim (rust)'
	/usr/bin/bash bin/head -20 go/main.go | grep -q 'sqlite3'
	echo 'PASS: head shim (go)'
	/usr/bin/bash bin/sed -n '1,10p' go/main.go | grep -q 'sqlite3'
	echo 'PASS: sed shim (go)'
	/usr/bin/bash bin/grep extractSymbols go/main.go | grep -q 'sqlite3'
	echo 'PASS: grep shim (go)'
	/usr/bin/bash bin/sqlite3 rust/.indexnudge.db 'SELECT count(*) FROM files;' | grep -qE '^[1-9]'
	echo 'PASS: sqlite3 shim (rust)'
	/usr/bin/bash bin/sqlite3 go/.indexnudge.db 'SELECT count(*) FROM files;' | grep -qE '^[1-9]'
	echo 'PASS: sqlite3 shim (go)'
	rm -f rust/.indexnudge.db go/.indexnudge.db
	echo 'SHIM TESTS PASSED'

patch:V:
	cd goose && git checkout -- crates/goose/src/agents/platform_extensions/developer/tree.rs
	cd goose && git apply ../patches/goose-tree-indexnudge.patch

goose:V: patch
	cd goose && cargo build --release -p goose-cli

clean:V:
	cd rust && cargo clean
	rm -f go/indexnudge
	rm -f rust/.indexnudge.db go/.indexnudge.db
