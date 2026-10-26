GOFLAGS=-gcflags=all=-l

rust:V:
	make -C core rust && cp core/bin-rust/indexnudge bin-rust/indexnudge

rust-test:V:
	make -C core rust-test

go:V:
	make -C core go && cp core/bin-go/indexnudge bin-go/indexnudge && cp core/bin-go/indexnudge core/go/indexnudge

go-test:V:
	make -C core go-test

all:V: rust go
	echo 'Build complete'

snippets:V:
	gpp             AGENTS.md.gpp > AGENTS.md.snippet
	gpp -DLANG_RUST AGENTS.md.gpp > AGENTS.md.rust.snippet
	gpp -DLANG_GO   AGENTS.md.gpp > AGENTS.md.go.snippet
	echo 'Regenerated: AGENTS.md{,.rust,.go}.snippet'

test:V: rust-test go-test shim-test
	echo 'ALL PASSED'

shim-test:V: rust go
	clean=$(echo $PATH | tr : \\n | grep -Fv "$PWD" | tr \\n :)
	echo $clean
	PATH=$PWD/bin-rust:$clean
	pat='sqlite3 [^ ]*\.indexnudge-[a-z]*\.db '
	(cd core/rust && target/release/indexnudge)
	echo 'PASS: rust full index'
	(cd core/rust && target/release/indexnudge --freshen 2>&1) | (! grep -q 'Freshened:')
	echo 'PASS: rust freshen no-op'
	env head -20 rust/src/main.rs | grep -q "$pat"
	echo 'PASS: head shim (rust)'
	env sed -n '1,10p' rust/src/main.rs | grep -q "$pat"
	echo 'PASS: sed shim (rust)'
	env grep create_schema rust/src/main.rs | grep -q "$pat"
	echo 'PASS: grep shim (rust)'
	env cat rust/src/main.rs | grep -q "$pat"
	echo 'PASS: cat shim (rust)'
	env tail -5 rust/src/main.rs | grep -q "$pat"
	echo 'PASS: tail shim (rust)'
	env sqlite3 rust/.indexnudge-rust.db 'SELECT count(*) FROM files;' | grep -qE '^[1-9]'
	echo 'PASS: sqlite3 shim (rust)'
	bash sqlite3 rust/.indexnudge-rust.db 'SELECT count(*) FROM files;' | grep -qE '^[1-9]'
	echo 'PASS: bash sqlite3 shim (rust)'
	PATH=$PWD/bin-go:$clean
	(cd core/go && ./indexnudge)
	echo 'PASS: go full index'
	(cd core/go && ./indexnudge --freshen 2>&1) | (! grep -q 'Freshened:')
	echo 'PASS: go freshen no-op'
	env head -20 go/main.go | grep -q "$pat"
	echo 'PASS: head shim (go)'
	env sed -n '1,10p' go/main.go | grep -q "$pat"
	echo 'PASS: sed shim (go)'
	env grep extractSymbols go/main.go | grep -q "$pat"
	echo 'PASS: grep shim (go)'
	env sqlite3 go/.indexnudge-go.db 'SELECT count(*) FROM files;' | grep -qE '^[1-9]'
	echo 'PASS: sqlite3 shim (go)'
	bash sqlite3 go/.indexnudge-go.db 'SELECT count(*) FROM files;' | grep -qE '^[1-9]'
	echo 'PASS: bash sqlite3 shim (go)'
	echo 'SHIM TESTS PASSED'

patch:V:
	cd goose
	git checkout -- crates/goose/src/agents/platform_extensions/developer/tree.rs
	git apply ../patches/goose-tree-indexnudge.patch

goose:V: patch
	cd goose
	cargo build --release -p goose-cli
	cp target/release/goose ../bin

clean:V:
	make -C core clean
	rm -f rust/.indexnudge-rust.db go/.indexnudge-go.db
