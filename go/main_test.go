package main

import (
	"database/sql"
	"go/ast"
	"os"
	"path/filepath"
	"testing"
	"time"

	_ "modernc.org/sqlite"
)

const sample = `package example

import (
	"fmt"
	"sync"
)

type Point struct {
	X float64
	Y float64
}

func (p *Point) Distance(other *Point) float64 {
	dx := p.X - other.X
	dy := p.Y - other.Y
	return dx*dx + dy*dy
}

func Add(a, b int) int {
	return a + b
}

func main() {
	p := &Point{X: 1.0, Y: 2.0}
	q := &Point{X: 4.0, Y: 6.0}
	d := p.Distance(q)
	fmt.Println("distance:", d)
	sum := Add(3, 4)
	fmt.Println("sum:", sum)
}
`

const sampleWithChannels = `package example

import "fmt"

func producer(ch chan<- int, done chan struct{}) {
	for i := 0; i < 10; i++ {
		ch <- i
	}
	close(ch)
	close(done)
}

func consumer(ch <-chan int) {
	for v := range ch {
		fmt.Println(v)
	}
}

func dispatcher(in <-chan int, out chan<- int) {
	select {
	case v := <-in:
		out <- v
	}
}

func main() {
	ch := make(chan int)
	done := make(chan struct{})
	go producer(ch, done)
	go consumer(ch)
	<-done
}
`

// sampleNoChannels has range over slice/map — should produce zero channel rows
const sampleNoChannels = `package example

func loops() {
	nums := []int{1, 2, 3}
	for _, n := range nums {
		_ = n
	}
	m := map[string]int{"a": 1}
	for k, v := range m {
		_ = k
		_ = v
	}
}
`

func setupDB(t *testing.T) *sql.DB {
	t.Helper()
	db, err := sql.Open("sqlite", ":memory:")
	if err != nil {
		t.Fatal(err)
	}
	if err := createSchema(db); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	return db
}

func indexSample(t *testing.T, db *sql.DB, src, filename string) {
	t.Helper()
	dir := t.TempDir()
	// loadPackages needs a Go module
	if err := os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module testmod\n\ngo 1.21\n"), 0644); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(dir, filename)
	if err := os.WriteFile(path, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	fset, pkgFiles, info, err := loadPackages(dir)
	if err != nil {
		t.Fatalf("loadPackages: %v", err)
	}
	f, ok := pkgFiles[path]
	if !ok {
		// Try matching by filename suffix — abs paths may differ
		for k, v := range pkgFiles {
			if filepath.Base(k) == filename {
				f = v
				ok = true
				break
			}
		}
		if !ok {
			t.Fatalf("file %s not found in loaded packages (have %v)", filename, pkgFileKeys(pkgFiles))
		}
	}
	if err := indexFile(db, fset, filename, path, f, info); err != nil {
		t.Fatal(err)
	}
}

func queryStrings(t *testing.T, db *sql.DB, query string) []string {
	t.Helper()
	rows, err := db.Query(query)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	var result []string
	for rows.Next() {
		var s string
		if err := rows.Scan(&s); err != nil {
			t.Fatal(err)
		}
		result = append(result, s)
	}
	return result
}

func queryInt(t *testing.T, db *sql.DB, query string) int64 {
	t.Helper()
	var n int64
	if err := db.QueryRow(query).Scan(&n); err != nil {
		t.Fatal(err)
	}
	return n
}

func contains(ss []string, target string) bool {
	for _, s := range ss {
		if s == target {
			return true
		}
	}
	return false
}

func pkgFileKeys(m map[string]*ast.File) []string {
	var keys []string
	for k := range m {
		keys = append(keys, k)
	}
	return keys
}

func TestSymbolsExtracted(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sample, "example.go")

	names := queryStrings(t, db, "SELECT name FROM symbols ORDER BY start_line")
	if !contains(names, "Point") {
		t.Error("missing struct Point")
	}
	if !contains(names, "Distance") {
		t.Error("missing method Distance")
	}
	if !contains(names, "Add") {
		t.Error("missing function Add")
	}
	if !contains(names, "main") {
		t.Error("missing function main")
	}

	// Check kinds
	kinds := queryStrings(t, db, "SELECT kind FROM symbols WHERE name = 'Point'")
	if !contains(kinds, "struct") {
		t.Errorf("Point should be struct, got %v", kinds)
	}
	kinds = queryStrings(t, db, "SELECT kind FROM symbols WHERE name = 'Distance'")
	if !contains(kinds, "method") {
		t.Errorf("Distance should be method, got %v", kinds)
	}
	kinds = queryStrings(t, db, "SELECT kind FROM symbols WHERE name = 'Add'")
	if !contains(kinds, "function") {
		t.Errorf("Add should be function, got %v", kinds)
	}
}

func TestQualifiedNames(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sample, "example.go")

	qnames := queryStrings(t, db, "SELECT qualified_name FROM symbols WHERE name = 'Distance' AND qualified_name IS NOT NULL")
	if len(qnames) == 0 {
		t.Fatal("Distance should have a qualified name")
	}
	if qnames[0] != "Point.Distance" {
		t.Errorf("expected Point.Distance, got %s", qnames[0])
	}
}

func TestImportsExtracted(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sample, "example.go")

	modules := queryStrings(t, db, "SELECT module FROM imports WHERE file = 'example.go' ORDER BY module")
	if !contains(modules, "fmt") {
		t.Error("missing import fmt")
	}
	if !contains(modules, "sync") {
		t.Error("missing import sync")
	}
}

func TestCallsExtracted(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sample, "example.go")

	callees := queryStrings(t, db, "SELECT DISTINCT callee FROM calls ORDER BY callee")
	if !contains(callees, "Add") {
		t.Error("missing call to Add")
	}
	if !contains(callees, "p.Distance") || !contains(callees, "fmt.Println") {
		// method calls show as receiver.method
		t.Errorf("expected method calls, got %v", callees)
	}
}

func TestDefsExtracted(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sample, "example.go")

	vars := queryStrings(t, db, "SELECT var FROM defs WHERE file = 'example.go' ORDER BY var")
	if !contains(vars, "p") {
		t.Error("missing def p")
	}
	if !contains(vars, "q") {
		t.Error("missing def q")
	}
	if !contains(vars, "d") {
		t.Error("missing def d")
	}
	if !contains(vars, "dx") {
		t.Error("missing def dx")
	}
	if !contains(vars, "sum") {
		t.Error("missing def sum")
	}
}

func TestLinesHaveSymbolAttribution(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sample, "example.go")

	// A line inside main should be attributed to main
	syms := queryStrings(t, db, "SELECT symbol FROM lines WHERE file = 'example.go' AND content LIKE '%Point{X: 1.0%'")
	if len(syms) == 0 || syms[0] != "main" {
		t.Errorf("expected main attribution, got %v", syms)
	}

	// A line inside Distance should be attributed to Point.Distance
	syms = queryStrings(t, db, "SELECT symbol FROM lines WHERE file = 'example.go' AND content LIKE '%dx*dx%'")
	if len(syms) == 0 || syms[0] != "Point.Distance" {
		t.Errorf("expected Point.Distance attribution, got %v", syms)
	}
}

func TestFTSSearchWorks(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sample, "example.go")
	if err := finalize(db); err != nil {
		t.Fatal(err)
	}

	count := queryInt(t, db, `SELECT count(*) FROM lines_fts WHERE content MATCH '"Distance"'`)
	if count < 2 {
		t.Errorf("expected at least 2 FTS hits for Distance, got %d", count)
	}
}

func TestDirectoriesPopulated(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sample, "example.go")
	populateDirectories(db)

	count := queryInt(t, db, "SELECT file_count FROM directories WHERE path = '.'")
	if count != 1 {
		t.Errorf("expected 1 file in root directory, got %d", count)
	}
}

func TestChannelsMake(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sampleWithChannels, "chans.go")

	ops := queryStrings(t, db, "SELECT operation FROM channels WHERE file = 'chans.go' ORDER BY line_num")
	if !contains(ops, "make") {
		t.Error("missing make operation")
	}
}

func TestChannelsSendRecvClose(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sampleWithChannels, "chans.go")

	ops := queryStrings(t, db, "SELECT DISTINCT operation FROM channels ORDER BY operation")
	for _, expected := range []string{"send", "recv", "close", "make", "select"} {
		if !contains(ops, expected) {
			t.Errorf("missing channel operation: %s (got %v)", expected, ops)
		}
	}
}

func TestChannelsSendHasVar(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sampleWithChannels, "chans.go")

	vars := queryStrings(t, db, "SELECT var FROM channels WHERE operation = 'send'")
	if len(vars) == 0 {
		t.Fatal("expected send operations with var names")
	}
	if !contains(vars, "ch") {
		t.Errorf("expected send to channel 'ch', got %v", vars)
	}
}

func TestChannelsRangeOnlyForChannelTypes(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sampleNoChannels, "nochans.go")

	count := queryInt(t, db, "SELECT count(*) FROM channels WHERE file = 'nochans.go'")
	if count != 0 {
		t.Errorf("expected 0 channel operations for slice/map range, got %d", count)
	}
}

func TestChannelsRangeOverChannel(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sampleWithChannels, "chans.go")

	ranges := queryStrings(t, db, "SELECT var FROM channels WHERE operation = 'range'")
	if !contains(ranges, "ch") {
		t.Errorf("expected range over channel 'ch', got %v", ranges)
	}
}

func TestCollectGoFilesSkipsVendor(t *testing.T) {
	dir := t.TempDir()
	os.MkdirAll(filepath.Join(dir, "pkg"), 0755)
	os.MkdirAll(filepath.Join(dir, "vendor", "lib"), 0755)
	os.WriteFile(filepath.Join(dir, "pkg", "main.go"), []byte("package main\n"), 0644)
	os.WriteFile(filepath.Join(dir, "vendor", "lib", "dep.go"), []byte("package lib\n"), 0644)

	files := collectGoFiles(dir)
	for _, f := range files {
		if filepath.Base(f) == "dep.go" {
			t.Error("should skip vendor/")
		}
	}
	found := false
	for _, f := range files {
		if filepath.Base(f) == "main.go" {
			found = true
		}
	}
	if !found {
		t.Error("should find pkg/main.go")
	}
}

func TestFreshenDetectsStaleFile(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, ".indexnudge.db")
	os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module testmod\n\ngo 1.21\n"), 0644)
	os.WriteFile(filepath.Join(dir, "lib.go"), []byte("package main\n\nfunc Hello() {}\n"), 0644)

	if err := fullIndex(dir, dbPath); err != nil {
		t.Fatal(err)
	}

	db, _ := sql.Open("sqlite", dbPath)
	count := queryInt(t, db, "SELECT count(*) FROM symbols WHERE name = 'Hello'")
	if count != 1 {
		t.Fatalf("expected 1 Hello symbol, got %d", count)
	}
	db.Close()

	time.Sleep(50 * time.Millisecond)
	os.WriteFile(filepath.Join(dir, "lib.go"), []byte("package main\n\nfunc Hello() {}\nfunc Goodbye() {}\n"), 0644)

	if err := doFreshen(dir, dbPath); err != nil {
		t.Fatal(err)
	}

	db, _ = sql.Open("sqlite", dbPath)
	defer db.Close()
	count = queryInt(t, db, "SELECT count(*) FROM symbols WHERE name = 'Goodbye'")
	if count != 1 {
		t.Errorf("freshen should have picked up Goodbye, got %d", count)
	}
}

func TestFreshenRemovesDeletedFile(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, ".indexnudge.db")
	os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module testmod\n\ngo 1.21\n"), 0644)
	os.WriteFile(filepath.Join(dir, "gone.go"), []byte("package main\n\nfunc Ephemeral() {}\n"), 0644)

	if err := fullIndex(dir, dbPath); err != nil {
		t.Fatal(err)
	}

	db, _ := sql.Open("sqlite", dbPath)
	count := queryInt(t, db, "SELECT count(*) FROM files")
	if count != 1 {
		t.Fatalf("expected 1 file, got %d", count)
	}
	db.Close()

	os.Remove(filepath.Join(dir, "gone.go"))

	if err := doFreshen(dir, dbPath); err != nil {
		t.Fatal(err)
	}

	db, _ = sql.Open("sqlite", dbPath)
	defer db.Close()
	count = queryInt(t, db, "SELECT count(*) FROM files")
	if count != 0 {
		t.Errorf("freshen should have removed deleted file, got %d", count)
	}
	count = queryInt(t, db, "SELECT count(*) FROM symbols")
	if count != 0 {
		t.Errorf("symbols from deleted file should be gone, got %d", count)
	}
}

func TestFuncSignature(t *testing.T) {
	db := setupDB(t)
	indexSample(t, db, sample, "example.go")

	sigs := queryStrings(t, db, "SELECT signature FROM symbols WHERE name = 'Distance' AND signature IS NOT NULL")
	if len(sigs) == 0 {
		t.Fatal("Distance should have a signature")
	}
	sig := sigs[0]
	if sig == "" {
		t.Error("signature should not be empty")
	}
	// Should contain receiver, name, params, return type
	for _, want := range []string{"Point", "Distance", "Point", "float64"} {
		found := false
		for i := range sig {
			if i+len(want) <= len(sig) && sig[i:i+len(want)] == want {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("signature %q should contain %q", sig, want)
		}
	}
}
