
target/release/codeindex: src/main.rs Cargo.toml Cargo.lock
	cargo build --release

install:V: target/release/codeindex
	ln -sf $PWD/target/release/codeindex $PWD/bin/codeindex

chmod:V:
	chmod +x $PWD/bin/sqlite3 $PWD/bin/sed $PWD/bin/grep $PWD/bin/head $PWD/bin/tail $PWD/bin/cat

all:V: target/release/codeindex install chmod
	echo 'Shims ready in '$PWD/bin
	echo 'Add to PATH: export PATH='$PWD/bin':$PATH'

test:V: install chmod
	# Full index
	$PWD/bin/codeindex .
	# Freshen with no changes
	$PWD/bin/codeindex --freshen . 2>&1 | grep -q 'up to date'
	echo 'PASS: freshen no-op'
	# Touch and freshen (save/restore mtime to avoid triggering rebuild)
	stat -c %Y src/main.rs > /tmp/_codeindex_mtime
	touch src/main.rs
	$PWD/bin/codeindex --freshen . 2>&1 | grep -q 'reindexed'
	echo 'PASS: freshen detects stale'
	touch -d @$(cat /tmp/_codeindex_mtime) src/main.rs
	rm -f /tmp/_codeindex_mtime
	# Shim: head
	PATH=$PWD/bin:$PATH $PWD/bin/head -20 src/main.rs | grep -q 'sqlite3'
	echo 'PASS: head shim'
	# Shim: sed
	PATH=$PWD/bin:$PATH $PWD/bin/sed -n '1,10p' src/main.rs | grep -q 'sqlite3'
	echo 'PASS: sed shim'
	# Shim: grep
	PATH=$PWD/bin:$PATH $PWD/bin/grep create_schema src/main.rs | grep -q 'sqlite3'
	echo 'PASS: grep shim'
	# Shim: cat
	PATH=$PWD/bin:$PATH $PWD/bin/cat src/main.rs | grep -q 'sqlite3'
	echo 'PASS: cat shim'
	# Shim: tail
	PATH=$PWD/bin:$PATH $PWD/bin/tail -5 src/main.rs | grep -q 'sqlite3'
	echo 'PASS: tail shim'
	# Shim: sqlite3 freshen
	stat -c %Y src/main.rs > /tmp/_codeindex_mtime
	touch src/main.rs
	PATH=$PWD/bin:$PATH $PWD/bin/sqlite3 .codeindex.db 'SELECT count(*) FROM files;' | grep -q '1'
	echo 'PASS: sqlite3 shim freshens'
	touch -d @$(cat /tmp/_codeindex_mtime) src/main.rs
	rm -f /tmp/_codeindex_mtime
	echo 'ALL PASSED'

clean:V:
	rm -f $PWD/bin/codeindex
	cargo clean
