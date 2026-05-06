// indexnudge — POC: index a Rust codebase into SQLite for agentic querying
// AI-generated code

use anyhow::{Context, Result};
use clap::Parser;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use syn::visit::Visit;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "indexnudge", about = "Index a Rust codebase into SQLite")]
struct Cli {
    /// Directory to index
    #[arg(default_value = ".")]
    dir: PathBuf,

    /// Output database path (default: <dir>/.indexnudge.db)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Incremental refresh: only re-index files newer than the DB
    #[arg(long)]
    freshen: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = cli.dir.canonicalize().context("cannot resolve directory")?;
    let db_path = cli.output.unwrap_or_else(|| dir.join(".indexnudge-rust.db"));

    if cli.freshen {
        freshen(&dir, &db_path)
    } else {
        full_index(&dir, &db_path)
    }
}

fn full_index(dir: &PathBuf, db_path: &PathBuf) -> Result<()> {
    let bak_path = db_path.with_extension("db.bak");
    let tmp_path = db_path.with_extension("db.tmp");

    // Move existing DB to .bak for safety
    if db_path.exists() {
        fs::rename(db_path, &bak_path)?;
    }
    // Remove any stale .tmp
    let _ = fs::remove_file(&tmp_path);

    let result = (|| -> Result<()> {
        let conn = Connection::open(&tmp_path)?;
        create_schema(&conn)?;

        let files = collect_rs_files(dir);
        eprintln!("Indexing {} files...", files.len());

        for file in &files {
            let rel = file.strip_prefix(dir).unwrap_or(file);
            let rel_str = rel.to_string_lossy();
            index_file(&conn, &rel_str, file)?;
        }

        finalize(&conn)?;
        Ok(())
    })();

    match result {
        Ok(()) => {
            fs::rename(&tmp_path, db_path)?;
            let _ = fs::remove_file(&bak_path);
            eprintln!("Done. Database: {}", db_path.display());
            Ok(())
        }
        Err(e) => {
            // .tmp is partial, .bak (if any) is last good, canonical absent
            eprintln!("Full index failed: {}. Old database preserved as .bak if it existed.", e);
            Err(e)
        }
    }
}

fn freshen(dir: &PathBuf, db_path: &PathBuf) -> Result<()> {
    let bak_path = db_path.with_extension("db.bak");

    if !db_path.exists() {
        if bak_path.exists() {
            // Another freshen is in progress — exit, let shim fall back
            eprintln!("Another freshen in progress, skipping.");
            return Ok(());
        }
        eprintln!("No existing database, doing full index.");
        return full_index(dir, db_path);
    }

    // Rename canonical → .bak
    fs::rename(db_path, &bak_path)?;

    let result = (|| -> Result<(u64, u64)> {
        let conn = Connection::open(&bak_path)?;
        conn.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=OFF; PRAGMA busy_timeout=5000;")?;
        conn.execute_batch("BEGIN IMMEDIATE;")?;

        // Collect current files on disk
        let disk_files = collect_rs_files(dir);
        let disk_set: std::collections::HashSet<String> = disk_files
            .iter()
            .map(|f| {
                f.strip_prefix(dir)
                    .unwrap_or(f)
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        // Get indexed files and their mtimes from DB
        let mut stmt = conn.prepare("SELECT file, indexed_mtime FROM files")?;
        let db_files: Vec<(String, f64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        let db_set: std::collections::HashSet<String> =
            db_files.iter().map(|(f, _)| f.clone()).collect();

        let mut reindexed = 0u64;
        let mut removed = 0u64;

        // Delete files that no longer exist on disk
        for (db_file, _) in &db_files {
            if !disk_set.contains(db_file.as_str()) {
                delete_file_rows(&conn, db_file)?;
                removed += 1;
            }
        }

        // Build a map of db file → indexed_mtime for quick lookup
        let db_mtimes: std::collections::HashMap<&str, f64> =
            db_files.iter().map(|(f, m)| (f.as_str(), *m)).collect();

        // Index new or stale files
        for file in &disk_files {
            let rel = file
                .strip_prefix(dir)
                .unwrap_or(file)
                .to_string_lossy()
                .to_string();
            let disk_mtime = file_mtime(file);

            let needs_index = if let Some(&db_mtime) = db_mtimes.get(rel.as_str()) {
                disk_mtime > db_mtime
            } else {
                true // new file
            };

            if needs_index {
                if db_set.contains(&rel) {
                    delete_file_rows(&conn, &rel)?;
                }
                index_file(&conn, &rel, file)?;
                reindexed += 1;
            }
        }

        if reindexed > 0 || removed > 0 {
            // Rebuild directories and FTS
            conn.execute("DELETE FROM directories", [])?;
            conn.execute("DELETE FROM directories", [])?;
            populate_directories(&conn)?;
        }

        conn.execute_batch("COMMIT;")?;
        Ok((reindexed, removed))
    })();

    match result {
        Ok((reindexed, removed)) => {
            fs::rename(&bak_path, db_path)?;
            if reindexed > 0 || removed > 0 {
                eprintln!("Freshened: {} reindexed, {} removed.", reindexed, removed);
            } else {
                eprintln!("All files up to date.");
            }
            Ok(())
        }
        Err(e) => {
            // ROLLBACK implicit on drop. .bak is untouched (pre-freshen state).
            // Canonical absent — shims fall back to filesystem.
            eprintln!("Freshen failed: {}. Database preserved as .bak.", e);
            Err(e)
        }
    }
}

/// Collect all .rs files under dir, skipping target/
fn collect_rs_files(dir: &PathBuf) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
        .filter(|e| !e.path_is_symlink())
        .filter(|e| !e.path().components().any(|c| c.as_os_str() == "target"))
        .map(|e| e.into_path())
        .collect()
}

/// Index a single file: parse, extract, insert into all tables
fn index_file(conn: &Connection, rel_str: &str, abs_path: &PathBuf) -> Result<()> {
    let source = match fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("skip {}: {}", rel_str, e);
            return Ok(());
        }
    };

    let syntax = match syn::parse_file(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("parse error in {}: {} (indexing lines only)", rel_str, e);
            return index_file_lines_only(conn, rel_str, &source, abs_path);
        }
    };

    index_lines(conn, rel_str, &source, &syntax)?;

    let mut visitor = Extractor::new(rel_str.to_string(), &source);
    visitor.visit_file(&syntax);

    insert_symbols(conn, &visitor.symbols)?;
    insert_calls(conn, &visitor.calls)?;
    insert_imports(conn, &visitor.imports)?;
    insert_defs(conn, &visitor.defs)?;

    let line_count = source.lines().count() as i64;
    let mtime = file_mtime(abs_path);
    conn.execute(
        "INSERT INTO files (file, lines, language, indexed_mtime) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![rel_str, line_count, "rust", mtime],
    )?;
    Ok(())
}

fn index_file_lines_only(conn: &Connection, rel_str: &str, source: &str, abs_path: &PathBuf) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO lines (file, line_num, content, symbol) VALUES (?1, ?2, ?3, '')",
    )?;
    for (i, line) in source.lines().enumerate() {

    let mut stmt_fts = conn.prepare(
        "INSERT INTO lines_fts (file, line_num, content, symbol) VALUES (?1, ?2, ?3, '')",
    )?;
        stmt.execute(rusqlite::params![rel_str, (i + 1) as i64, line])?;
        stmt_fts.execute(rusqlite::params![rel_str, (i + 1) as i64, line])?;
    }

    let line_count = source.lines().count() as i64;
    let mtime = file_mtime(abs_path);
    conn.execute(
        "INSERT INTO files (file, lines, language, indexed_mtime, parse_error) VALUES (?1, ?2, ?3, ?4, 1)",
        rusqlite::params![rel_str, line_count, "rust", mtime],
    )?;
    Ok(())
}

/// Delete all rows for a file from every table
fn delete_file_rows(conn: &Connection, file: &str) -> Result<()> {
    conn.execute("DELETE FROM lines WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM lines_fts WHERE file = ?1", rusqlite::params![file])?;
    conn.execute(
        "DELETE FROM symbols WHERE file = ?1",
        rusqlite::params![file],
    )?;
    conn.execute(
        "DELETE FROM calls WHERE caller_file = ?1",
        rusqlite::params![file],
    )?;
    conn.execute(
        "DELETE FROM imports WHERE file = ?1",
        rusqlite::params![file],
    )?;
    conn.execute("DELETE FROM defs WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM files WHERE file = ?1", rusqlite::params![file])?;
    Ok(())
}

/// Get file mtime as seconds since epoch (f64)
fn file_mtime(path: &PathBuf) -> f64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64()
        })
        .unwrap_or(0.0)
}

/// Rebuild directories table and populate FTS (used after full index)
fn finalize(conn: &Connection) -> Result<()> {
    populate_directories(conn)?;
    Ok(())
}

fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode=DELETE;
        PRAGMA synchronous=OFF;

        CREATE TABLE lines (
            file     TEXT,
            line_num INTEGER,
            content  TEXT,
            symbol   TEXT,
            PRIMARY KEY (file, line_num)
        );
        CREATE VIRTUAL TABLE lines_fts USING fts5(file, line_num, content, symbol);

        CREATE TABLE symbols (
            id             INTEGER PRIMARY KEY,
            name           TEXT,
            qualified_name TEXT,
            kind           TEXT,
            file           TEXT,
            start_line     INTEGER,
            end_line       INTEGER,
            parent         TEXT,
            signature      TEXT
        );

        CREATE TABLE calls (
            caller_file   TEXT,
            caller_symbol TEXT,
            callee        TEXT,
            callee_qname  TEXT,
            kind          TEXT,
            line_num      INTEGER
        );
        CREATE TABLE defs (
            symbol TEXT,
            file   TEXT,
            line_num INTEGER,
            var    TEXT,
            kind   TEXT
        );

        CREATE TABLE imports (
            file   TEXT,
            module TEXT,
            line_num INTEGER
        );

        CREATE INDEX idx_symbols_name ON symbols(name);
        CREATE INDEX idx_symbols_qualified ON symbols(qualified_name);
        CREATE INDEX idx_symbols_file ON symbols(file);
        CREATE INDEX idx_calls_callee ON calls(callee);
        CREATE INDEX idx_calls_caller ON calls(caller_symbol);
        CREATE INDEX idx_defs_var ON defs(var);
        CREATE INDEX idx_imports_module ON imports(module);

        CREATE TABLE files (
            file           TEXT PRIMARY KEY,
            lines          INTEGER,
            language       TEXT,
            indexed_mtime  REAL,
            parse_error    INTEGER DEFAULT 0
        );

        CREATE TABLE directories (
            path        TEXT PRIMARY KEY,
            file_count  INTEGER,
            total_lines INTEGER
        );
        ",
    )?;
    Ok(())
}

// --- Line indexing ---

fn index_lines(conn: &Connection, file: &str, source: &str, syntax: &syn::File) -> Result<()> {
    // Build a line→symbol map from the parsed file
    let mut line_symbols: Vec<Option<String>> = vec![None; source.lines().count() + 1];
    let mut mapper = LineSymbolMapper {
        lines: &mut line_symbols,
        scope: Vec::new(),
        source,
    };
    mapper.visit_file(syntax);

    let mut stmt = conn
        .prepare("INSERT INTO lines (file, line_num, content, symbol) VALUES (?1, ?2, ?3, ?4)")?;

    let mut stmt_fts = conn
        .prepare("INSERT INTO lines_fts (file, line_num, content, symbol) VALUES (?1, ?2, ?3, ?4)")?;
    for (i, line_content) in source.lines().enumerate() {
        let line_num = (i + 1) as i64;
        let sym = line_symbols.get(i + 1).and_then(|s| s.as_deref());
        stmt.execute(rusqlite::params![file, line_num, line_content, sym])?;
        stmt_fts.execute(rusqlite::params![file, line_num, line_content, sym])?;
    }
    Ok(())
}

struct LineSymbolMapper<'a> {
    lines: &'a mut Vec<Option<String>>,
    scope: Vec<String>,
    source: &'a str,
}

impl<'a> LineSymbolMapper<'a> {
    fn current_symbol(&self) -> Option<String> {
        self.scope.last().cloned()
    }
}

impl<'a, 'ast> Visit<'ast> for LineSymbolMapper<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        self.scope.push(name);
        let span = node.sig.ident.span();
        let start = span.start().line;
        let end = end_line_of_block(self.source, node.block.brace_token.span.close());
        for l in start..=end {
            if l < self.lines.len() {
                self.lines[l] = self.current_symbol();
            }
        }
        syn::visit::visit_item_fn(self, node);
        self.scope.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let qualified = if let Some(parent) = self.scope.last() {
            format!("{}::{}", parent, name)
        } else {
            name.clone()
        };
        self.scope.push(qualified);
        let start = node.sig.ident.span().start().line;
        let end = end_line_of_block(self.source, node.block.brace_token.span.close());
        for l in start..=end {
            if l < self.lines.len() {
                self.lines[l] = self.current_symbol();
            }
        }
        syn::visit::visit_impl_item_fn(self, node);
        self.scope.pop();
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let name = type_name(&node.self_ty);
        self.scope.push(name);
        syn::visit::visit_item_impl(self, node);
        self.scope.pop();
    }
}

// --- Extraction ---

struct SymbolRecord {
    name: String,
    qualified_name: Option<String>,
    kind: String,
    file: String,
    start_line: usize,
    end_line: usize,
    parent: Option<String>,
    signature: Option<String>,
}

struct CallRecord {
    caller_file: String,
    caller_symbol: Option<String>,
    callee: String,
    kind: String, // "function", "method", "macro"
    callee_qname: Option<String>,
    line_num: usize,
}

struct DefRecord {
    symbol: Option<String>,
    file: String,
    line_num: usize,
    var: String,
    kind: String, // "let", "assign", "parameter", "mut_ref"
}

struct ImportRecord {
    file: String,
    module: String,
    line_num: usize,
}

struct Extractor {
    file: String,
    source: String,
    symbols: Vec<SymbolRecord>,
    calls: Vec<CallRecord>,
    imports: Vec<ImportRecord>,
    defs: Vec<DefRecord>,
    scope: Vec<String>,     // stack of enclosing symbol names
    impl_type: Vec<String>, // stack of impl type names
}

impl Extractor {
    fn new(file: String, source: &str) -> Self {
        Self {
            file,
            source: source.to_string(),
            symbols: Vec::new(),
            calls: Vec::new(),
            imports: Vec::new(),
            defs: Vec::new(),
            scope: Vec::new(),
            impl_type: Vec::new(),
        }
    }

    fn current_scope(&self) -> Option<String> {
        self.scope.last().cloned()
    }

    fn current_impl(&self) -> Option<&String> {
        self.impl_type.last()
    }

    fn qualified_name(&self, name: &str) -> Option<String> {
        self.current_impl().map(|t| format!("{}::{}", t, name))
    }

    fn sig_text(&self, sig: &syn::Signature) -> String {
        let s = sig.ident.span().start();
        let e = sig.output.span().end();
        extract_span(&self.source, s.line, s.column, e.line, e.column)
    }

    fn add_params_as_defs(&mut self, sig: &syn::Signature) {
        for input in &sig.inputs {
            match input {
                syn::FnArg::Receiver(_) => {}
                syn::FnArg::Typed(pat_type) => {
                    if let Some(name) = pat_name(&pat_type.pat) {
                        self.defs.push(DefRecord {
                            symbol: self.current_scope(),
                            file: self.file.clone(),
                            line_num: pat_type.pat.span().start().line,
                            var: name,
                            kind: "parameter".to_string(),
                        });
                    }
                }
            }
        }
    }
}

use proc_macro2::Span;

fn span_start_line(span: Span) -> usize {
    span.start().line
}

fn span_end_line(span: Span) -> usize {
    span.end().line
}

impl<'ast> Visit<'ast> for Extractor {
    // --- Symbols ---

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        let start = span_start_line(node.sig.ident.span());
        let end = end_line_of_block(&self.source, node.block.brace_token.span.close());
        self.symbols.push(SymbolRecord {
            name: name.clone(),
            qualified_name: None,
            kind: "function".into(),
            file: self.file.clone(),
            start_line: start,
            end_line: end,
            parent: None,
            signature: Some(self.sig_text(&node.sig)),
        });
        self.scope.push(name);
        self.add_params_as_defs(&node.sig);
        syn::visit::visit_item_fn(self, node);
        self.scope.pop();
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let ty = type_name(&node.self_ty);
        self.impl_type.push(ty.clone());

        let trait_name = node
            .trait_
            .as_ref()
            .map(|(_, path, _)| path_to_string(path));
        let kind = if trait_name.is_some() {
            "trait_impl"
        } else {
            "impl"
        };

        self.symbols.push(SymbolRecord {
            name: ty.clone(),
            qualified_name: trait_name.as_ref().map(|t| format!("<{} as {}>", ty, t)),
            kind: kind.into(),
            file: self.file.clone(),
            start_line: span_start_line(node.impl_token.span),
            end_line: span_end_line(node.brace_token.span.close()),
            parent: None,
            signature: trait_name.map(|t| format!("impl {} for {}", t, ty)),
        });

        self.scope.push(ty);
        syn::visit::visit_item_impl(self, node);
        self.scope.pop();
        self.impl_type.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let qname = self.qualified_name(&name);
        let parent = self.current_impl().cloned();
        let start = span_start_line(node.sig.ident.span());
        let end = end_line_of_block(&self.source, node.block.brace_token.span.close());
        self.symbols.push(SymbolRecord {
            name: name.clone(),
            qualified_name: qname.clone(),
            kind: "method".into(),
            file: self.file.clone(),
            start_line: start,
            end_line: end,
            parent,
            signature: Some(self.sig_text(&node.sig)),
        });
        self.scope.push(qname.unwrap_or(name));
        self.add_params_as_defs(&node.sig);
        syn::visit::visit_impl_item_fn(self, node);
        self.scope.pop();
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.symbols.push(SymbolRecord {
            name: node.ident.to_string(),
            qualified_name: None,
            kind: "struct".into(),
            file: self.file.clone(),
            start_line: span_start_line(node.ident.span()),
            end_line: span_end_line(node.ident.span()), // approximate
            parent: None,
            signature: None,
        });
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.symbols.push(SymbolRecord {
            name: node.ident.to_string(),
            qualified_name: None,
            kind: "enum".into(),
            file: self.file.clone(),
            start_line: span_start_line(node.ident.span()),
            end_line: span_end_line(node.brace_token.span.close()),
            parent: None,
            signature: None,
        });
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        let name = node.ident.to_string();
        self.symbols.push(SymbolRecord {
            name: name.clone(),
            qualified_name: None,
            kind: "trait".into(),
            file: self.file.clone(),
            start_line: span_start_line(node.ident.span()),
            end_line: span_end_line(node.brace_token.span.close()),
            parent: None,
            signature: None,
        });
        self.scope.push(name);
        syn::visit::visit_item_trait(self, node);
        self.scope.pop();
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        let name = node.sig.ident.to_string();
        let qname = self.scope.last().map(|t| format!("{}::{}", t, name));
        self.symbols.push(SymbolRecord {
            name: name.clone(),
            qualified_name: qname,
            kind: "trait_method".into(),
            file: self.file.clone(),
            start_line: span_start_line(node.sig.ident.span()),
            end_line: node
                .default
                .as_ref()
                .map_or(span_end_line(node.sig.ident.span()), |block| {
                    end_line_of_block(&self.source, block.brace_token.span.close())
                }),
            parent: self.scope.last().cloned(),
            signature: Some(self.sig_text(&node.sig)),
        });
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.symbols.push(SymbolRecord {
            name: node.ident.to_string(),
            qualified_name: None,
            kind: "const".into(),
            file: self.file.clone(),
            start_line: span_start_line(node.ident.span()),
            end_line: span_end_line(node.ident.span()),
            parent: None,
            signature: None,
        });
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.symbols.push(SymbolRecord {
            name: node.ident.to_string(),
            qualified_name: None,
            kind: "static".into(),
            file: self.file.clone(),
            start_line: span_start_line(node.ident.span()),
            end_line: span_end_line(node.ident.span()),
            parent: None,
            signature: None,
        });
        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.symbols.push(SymbolRecord {
            name: node.ident.to_string(),
            qualified_name: None,
            kind: "type".into(),
            file: self.file.clone(),
            start_line: span_start_line(node.ident.span()),
            end_line: span_end_line(node.ident.span()),
            parent: None,
            signature: None,
        });
        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if let Some(ref ident) = node.ident {
            self.symbols.push(SymbolRecord {
                name: ident.to_string(),
                qualified_name: None,
                kind: "macro".into(),
                file: self.file.clone(),
                start_line: span_start_line(ident.span()),
                end_line: span_end_line(ident.span()),
                parent: None,
                signature: None,
            });
        }
        syn::visit::visit_item_macro(self, node);
    }

    // --- Calls ---

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        let callee = expr_callee_name(&node.func);
        if let Some(callee) = callee {
            self.calls.push(CallRecord {
                caller_file: self.file.clone(),
                caller_symbol: self.current_scope(),
                callee,
                callee_qname: None,
                kind: "function".into(),
                line_num: span_start_line(node.func.span()),
            });
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.calls.push(CallRecord {
            caller_file: self.file.clone(),
            caller_symbol: self.current_scope(),
            callee: node.method.to_string(),
            callee_qname: None,
            kind: "method".into(),
            line_num: span_start_line(node.method.span()),
        });
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let name = path_to_string(&node.path);
        self.calls.push(CallRecord {
            caller_file: self.file.clone(),
            caller_symbol: self.current_scope(),
            callee: name,
            callee_qname: None,
            kind: "macro".into(),
            line_num: span_start_line(node.path.span()),
        });
        // Don't recurse into macro tokens — syn can't parse them generically
    }

    // --- Imports ---

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let modules = flatten_use_tree(&node.tree, &[]);
        for (module, line_num) in modules {
            self.imports.push(ImportRecord {
                file: self.file.clone(),
                module,
                line_num,
            });
        }
    }

    // --- Defs (best effort) ---

    fn visit_local(&mut self, node: &'ast syn::Local) {
        if let Some(name) = pat_name(&node.pat) {
            let kind = if is_pat_mut(&node.pat) {
                "let_mut"
            } else {
                "let"
            };
            self.defs.push(DefRecord {
                symbol: self.current_scope(),
                file: self.file.clone(),
                line_num: span_start_line(node.pat.span()),
                var: name,
                kind: kind.to_string(),
            });
        }
        syn::visit::visit_local(self, node);
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        if let Some(name) = expr_lhs_name(&node.left) {
            self.defs.push(DefRecord {
                symbol: self.current_scope(),
                file: self.file.clone(),
                line_num: span_start_line(node.left.span()),
                var: name,
                kind: "assign".to_string(),
            });
        }
        syn::visit::visit_expr_assign(self, node);
    }
}

// --- Helpers ---

fn end_line_of_block(_source: &str, close_span: Span) -> usize {
    close_span.end().line
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(tp) => path_to_string(&tp.path),
        _ => quote::quote!(#ty).to_string(),
    }
}

fn path_to_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn expr_callee_name(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Path(ep) => Some(path_to_string(&ep.path)),
        syn::Expr::Field(ef) => {
            // e.g. self.field — unlikely as a call, but handle it
            Some(ef.member.span().source_text().unwrap_or_default())
        }
        _ => None,
    }
}

fn pat_name(pat: &syn::Pat) -> Option<String> {
    match pat {
        syn::Pat::Ident(pi) => Some(pi.ident.to_string()),
        syn::Pat::Type(pt) => pat_name(&pt.pat),
        _ => None,
    }
}

fn is_pat_mut(pat: &syn::Pat) -> bool {
    match pat {
        syn::Pat::Ident(pi) => pi.mutability.is_some(),
        syn::Pat::Type(pt) => is_pat_mut(&pt.pat),
        _ => false,
    }
}

fn expr_lhs_name(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Path(ep) => Some(path_to_string(&ep.path)),
        syn::Expr::Field(ef) => {
            let base = expr_lhs_name(&ef.base)?;
            let member = match &ef.member {
                syn::Member::Named(ident) => ident.to_string(),
                syn::Member::Unnamed(idx) => idx.index.to_string(),
            };
            Some(format!("{}.{}", base, member))
        }
        syn::Expr::Index(ei) => expr_lhs_name(&ei.expr),
        _ => None,
    }
}

fn flatten_use_tree(tree: &syn::UseTree, prefix: &[String]) -> Vec<(String, usize)> {
    match tree {
        syn::UseTree::Path(p) => {
            let mut new_prefix = prefix.to_vec();
            new_prefix.push(p.ident.to_string());
            flatten_use_tree(&p.tree, &new_prefix)
        }
        syn::UseTree::Name(n) => {
            let mut path = prefix.to_vec();
            path.push(n.ident.to_string());
            vec![(path.join("::"), span_start_line(n.ident.span()))]
        }
        syn::UseTree::Rename(r) => {
            let mut path = prefix.to_vec();
            path.push(r.ident.to_string());
            vec![(path.join("::"), span_start_line(r.ident.span()))]
        }
        syn::UseTree::Glob(g) => {
            let mut path = prefix.to_vec();
            path.push("*".to_string());
            vec![(path.join("::"), span_start_line(g.star_token.span))]
        }
        syn::UseTree::Group(g) => g
            .items
            .iter()
            .flat_map(|t| flatten_use_tree(t, prefix))
            .collect(),
    }
}

fn extract_span(
    source: &str,
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if start_line == 0 || end_line == 0 || start_line > lines.len() {
        return String::new();
    }
    if start_line == end_line {
        let line = lines[start_line - 1];
        let end = end_col.min(line.len());
        if start_col >= end {
            return line.to_string();
        }
        return line[start_col..end].to_string();
    }
    let mut result = String::new();
    for l in start_line..=end_line.min(lines.len()) {
        if l == start_line {
            result.push_str(&lines[l - 1][start_col..]);
        } else if l == end_line {
            result.push_str(&lines[l - 1][..end_col.min(lines[l - 1].len())]);
        } else {
            result.push_str(lines[l - 1]);
        }
        if l < end_line {
            result.push('\n');
        }
    }
    result
}

// --- DB insertion ---

fn insert_symbols(conn: &Connection, symbols: &[SymbolRecord]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO symbols (name, qualified_name, kind, file, start_line, end_line, parent, signature)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for s in symbols {
        stmt.execute(rusqlite::params![
            s.name,
            s.qualified_name,
            s.kind,
            s.file,
            s.start_line as i64,
            s.end_line as i64,
            s.parent,
            s.signature,
        ])?;
    }
    Ok(())
}

fn insert_calls(conn: &Connection, calls: &[CallRecord]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO calls (caller_file, caller_symbol, callee, callee_qname, kind, line_num)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for c in calls {
        stmt.execute(rusqlite::params![
            c.caller_file,
            c.caller_symbol,
            c.callee,
            c.callee_qname,
            c.kind,
            c.line_num as i64,
        ])?;
    }
    Ok(())
}

fn insert_imports(conn: &Connection, imports: &[ImportRecord]) -> Result<()> {
    let mut stmt =
        conn.prepare("INSERT INTO imports (file, module, line_num) VALUES (?1, ?2, ?3)")?;
    for i in imports {
        stmt.execute(rusqlite::params![i.file, i.module, i.line_num as i64])?;
    }
    Ok(())
}

fn insert_defs(conn: &Connection, defs: &[DefRecord]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO defs (symbol, file, line_num, var, kind) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for d in defs {
        stmt.execute(rusqlite::params![
            d.symbol,
            d.file,
            d.line_num as i64,
            d.var,
            d.kind,
        ])?;
    }
    Ok(())
}

fn populate_directories(conn: &Connection) -> Result<()> {
    use std::collections::HashMap;

    let mut stmt = conn.prepare("SELECT file, lines FROM files")?;
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut dirs: HashMap<String, (i64, i64)> = HashMap::new(); // (file_count, total_lines)
    for (file, lines) in &rows {
        // Collect all ancestor directories
        let path = std::path::Path::new(file);
        let mut ancestors: Vec<String> = Vec::new();
        if let Some(parent) = path.parent() {
            let mut p = parent;
            loop {
                let s = p.to_string_lossy().to_string();
                if s.is_empty() {
                    ancestors.push(".".to_string());
                    break;
                }
                ancestors.push(s);
                match p.parent() {
                    Some(pp) => p = pp,
                    None => break,
                }
            }
        } else {
            ancestors.push(".".to_string());
        }
        for dir in ancestors {
            let entry = dirs.entry(dir).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += lines;
        }
    }

    let mut stmt = conn
        .prepare("INSERT INTO directories (path, file_count, total_lines) VALUES (?1, ?2, ?3)")?;
    for (path, (file_count, total_lines)) in &dirs {
        stmt.execute(rusqlite::params![path, file_count, total_lines])?;
    }
    Ok(())
}

use proc_macro2;
use syn::spanned::Spanned;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    const SAMPLE: &str = r#"
use std::fs;
use std::collections::HashMap;

struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let p = Point::new(1.0, 2.0);
    let q = Point::new(4.0, 6.0);
    let d = p.distance(&q);
    println!("distance: {}", d);
    let sum = add(3, 4);
    println!("sum: {}", sum);
}
"#;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    fn index_sample(conn: &Connection) {
        let syntax = syn::parse_file(SAMPLE).unwrap();
        index_lines(conn, "test.rs", SAMPLE, &syntax).unwrap();
        let mut visitor = Extractor::new("test.rs".to_string(), SAMPLE);
        visitor.visit_file(&syntax);
        insert_symbols(conn, &visitor.symbols).unwrap();
        insert_calls(conn, &visitor.calls).unwrap();
        insert_imports(conn, &visitor.imports).unwrap();
        insert_defs(conn, &visitor.defs).unwrap();
        let line_count = SAMPLE.lines().count() as i64;
        conn.execute(
            "INSERT INTO files (file, lines, language, indexed_mtime) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["test.rs", line_count, "rust", 0.0],
        )
        .unwrap();
    }

    #[test]
    fn symbols_extracted() {
        let conn = setup_db();
        index_sample(&conn);

        let syms: Vec<(String, String)> = conn
            .prepare("SELECT name, kind FROM symbols ORDER BY start_line")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let names: Vec<&str> = syms.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"Point"), "missing struct Point");
        assert!(names.contains(&"main"), "missing fn main");
        assert!(names.contains(&"add"), "missing fn add");
        assert!(names.contains(&"new"), "missing method new");
        assert!(names.contains(&"distance"), "missing method distance");

        let kind_pairs: Vec<(&str, &str)> =
            syms.iter().map(|(n, k)| (n.as_str(), k.as_str())).collect();
        assert!(kind_pairs.contains(&("Point", "struct")));
        assert!(kind_pairs.contains(&("Point", "impl")));
        assert!(kind_pairs.contains(&("main", "function")));
        assert!(kind_pairs.contains(&("add", "function")));
        assert!(kind_pairs.contains(&("new", "method")));
        assert!(kind_pairs.contains(&("distance", "method")));
    }

    #[test]
    fn qualified_names_for_methods() {
        let conn = setup_db();
        index_sample(&conn);

        let qname: Option<String> = conn
            .query_row(
                "SELECT qualified_name FROM symbols WHERE name = 'new' AND kind = 'method'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(qname.as_deref(), Some("Point::new"));

        let qname: Option<String> = conn
            .query_row(
                "SELECT qualified_name FROM symbols WHERE name = 'distance' AND kind = 'method'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(qname.as_deref(), Some("Point::distance"));
    }

    #[test]
    fn imports_extracted() {
        let conn = setup_db();
        index_sample(&conn);

        let imports: Vec<String> = conn
            .prepare("SELECT module FROM imports WHERE file = 'test.rs' ORDER BY module")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(imports.contains(&"std::collections::HashMap".to_string()));
        assert!(imports.contains(&"std::fs".to_string()));
    }

    #[test]
    fn calls_extracted() {
        let conn = setup_db();
        index_sample(&conn);

        let callees: Vec<String> = conn
            .prepare("SELECT DISTINCT callee FROM calls ORDER BY callee")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(callees.contains(&"Point::new".to_string()));
        assert!(callees.contains(&"add".to_string()));
        assert!(
            callees.contains(&"distance".to_string()),
            "missing method call"
        );
        assert!(
            callees.contains(&"println".to_string()),
            "missing macro call"
        );
    }

    #[test]
    fn defs_extracted() {
        let conn = setup_db();
        index_sample(&conn);

        let defs: Vec<(String, String)> = conn
            .prepare("SELECT var, kind FROM defs WHERE file = 'test.rs' ORDER BY var")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        let vars: Vec<&str> = defs.iter().map(|(v, _)| v.as_str()).collect();
        assert!(vars.contains(&"p"), "missing let p");
        assert!(vars.contains(&"q"), "missing let q");
        assert!(vars.contains(&"d"), "missing let d");
        assert!(vars.contains(&"sum"), "missing let sum");
        assert!(vars.contains(&"dx"), "missing let dx");
        assert!(vars.contains(&"dy"), "missing let dy");
    }

    #[test]
    fn lines_have_symbol_attribution() {
        let conn = setup_db();
        index_sample(&conn);

        // A line inside fn main should be attributed to "main"
        let main_sym: Option<String> = conn
            .query_row(
                "SELECT symbol FROM lines WHERE file = 'test.rs' AND content LIKE '%Point::new(1.0%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(main_sym.as_deref(), Some("main"));

        // A line inside distance method should be attributed to it
        let dist_sym: Option<String> = conn
            .query_row(
                "SELECT symbol FROM lines WHERE file = 'test.rs' AND content LIKE '%dx * dx%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dist_sym.as_deref(), Some("Point::distance"));

        // The struct definition line should not be inside a function
        let struct_sym: Option<String> = conn
            .query_row(
                "SELECT symbol FROM lines WHERE file = 'test.rs' AND content LIKE '%struct Point%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            struct_sym.is_none(),
            "struct line should have no symbol scope"
        );
    }

    #[test]
    fn fts_search_works() {
        let conn = setup_db();
        index_sample(&conn);
        finalize(&conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM lines_fts WHERE content MATCH '\"distance\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            count >= 2,
            "expected at least 2 FTS hits for 'distance', got {count}"
        );
    }

    #[test]
    fn directories_populated() {
        let conn = setup_db();
        index_sample(&conn);
        populate_directories(&conn).unwrap();

        let (file_count, total_lines): (i64, i64) = conn
            .query_row(
                "SELECT file_count, total_lines FROM directories WHERE path = '.'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(file_count, 1);
        assert_eq!(total_lines, SAMPLE.lines().count() as i64);
    }

    #[test]
    fn freshen_detects_stale_file() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join(".indexnudge-rust.db");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let rs_file = src_dir.join("lib.rs");
        fs::write(&rs_file, "fn hello() {}\n").unwrap();

        let dir_path = dir.path().to_path_buf();
        full_index(&dir_path, &db_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM symbols WHERE name = 'hello'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        drop(conn);

        // Modify file and freshen
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(&rs_file, "fn goodbye() {}\nfn hello() {}\n").unwrap();
        freshen(&dir_path, &db_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM symbols WHERE name = 'goodbye'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "freshen should have picked up new symbol");
    }

    #[test]
    fn freshen_removes_deleted_file() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join(".indexnudge-rust.db");
        let rs_file = dir.path().join("gone.rs");
        fs::write(&rs_file, "fn ephemeral() {}\n").unwrap();

        let dir_path = dir.path().to_path_buf();
        full_index(&dir_path, &db_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        drop(conn);

        fs::remove_file(&rs_file).unwrap();
        freshen(&dir_path, &db_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "freshen should have removed deleted file");
        let sym_count: i64 = conn
            .query_row("SELECT count(*) FROM symbols", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sym_count, 0, "symbols from deleted file should be gone");
    }

    #[test]
    fn collect_rs_files_skips_target() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("target/debug")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.path().join("target/debug/build.rs"), "fn build() {}\n").unwrap();

        let files = collect_rs_files(&dir.path().to_path_buf());
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"main.rs".to_string()));
        assert!(
            !names.contains(&"build.rs".to_string()),
            "should skip target/"
        );
    }
}
