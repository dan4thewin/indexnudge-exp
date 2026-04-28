GOFLAGS=-gcflags=all=-l

rust:V:
	cd rust
	cargo build --release
	cp target/release/indexnudge ../bin-rust

rust-test:V:
	cd rust && cargo test

go:V:
	cd go
	go build -o indexnudge .
	cp indexnudge ../bin-go

go-test:V:
	cd go && go test -count=1 ./...

all:V: rust go
	echo 'Build complete'

test:V: rust-test go-test shim-test
	echo 'ALL PASSED'

shim-test:V: rust go
	(cd rust && target/release/indexnudge)
	echo 'PASS: rust full index'
	(cd rust && target/release/indexnudge --freshen 2>&1) | grep -q 'up to date'
	echo 'PASS: rust freshen no-op'
	(cd go && ./indexnudge)
	echo 'PASS: go full index'
	(cd go && ./indexnudge --freshen 2>&1) | grep -q 'up to date'
	pat='sqlite3 [^ ]*\.indexnudge\.db '
	bin/head -20 rust/src/main.rs | grep -q "$pat"
	echo 'PASS: head shim (rust)'
	bin/sed -n '1,10p' rust/src/main.rs | grep -q "$pat"
	echo 'PASS: sed shim (rust)'
	bin/grep create_schema rust/src/main.rs | grep -q "$pat"
	echo 'PASS: grep shim (rust)'
	bin/cat rust/src/main.rs | grep -q "$pat"
	echo 'PASS: cat shim (rust)'
	bin/tail -5 rust/src/main.rs | grep -q "$pat"
	echo 'PASS: tail shim (rust)'
	bin/head -20 go/main.go | grep -q "$pat"
	echo 'PASS: head shim (go)'
	bin/sed -n '1,10p' go/main.go | grep -q "$pat"
	echo 'PASS: sed shim (go)'
	bin/grep extractSymbols go/main.go | grep -q "$pat"
	echo 'PASS: grep shim (go)'
	bin/sqlite3 rust/.indexnudge.db 'SELECT count(*) FROM files;' | grep -qE '^[1-9]'
	echo 'PASS: sqlite3 shim (rust)'
	bin/sqlite3 go/.indexnudge.db 'SELECT count(*) FROM files;' | grep -qE '^[1-9]'
	echo 'PASS: sqlite3 shim (go)'
	echo 'SHIM TESTS PASSED'

#PATH=~+/bin-rust:$PATH bash -c 'cd rust; head src/main.rs'
patch:V:
	cd goose
	git checkout -- crates/goose/src/agents/platform_extensions/developer/tree.rs
	git apply ../patches/goose-tree-indexnudge.patch

goose:V: patch
	cd goose
	cargo build --release -p goose-cli
	cp target/release/goose ../bin

clean:V:
	(cd rust && cargo clean)
	(cd go && go clean)
	rm -f rust/.indexnudge.db go/.indexnudge.db
