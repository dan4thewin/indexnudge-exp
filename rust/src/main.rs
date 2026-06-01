// indexnudge — index a Rust codebase into SQLite for agentic querying
// Uses tree-sitter for parsing. Experiment branch: block spans + references.

use anyhow::{Context, Result};
use clap::Parser;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use tree_sitter::{Node, Parser as TsParser, Tree};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "indexnudge", about = "Index a Rust codebase into SQLite")]
struct Cli {
    /// Directory to index
    #[arg(default_value = ".")]
    dir: PathBuf,

    /// Output database path (default: <dir>/.indexnudge-rust.db)
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

// ─── Index orchestration ───────────────────────────────────────────────────

fn full_index(dir: &PathBuf, db_path: &PathBuf) -> Result<()> {
    let bak_path = db_path.with_extension("db.bak");
    let tmp_path = db_path.with_extension("db.tmp");

    if db_path.exists() {
        fs::rename(db_path, &bak_path)?;
    }
    let _ = fs::remove_file(&tmp_path);

    let result = (|| -> Result<()> {
        let conn = Connection::open(&tmp_path)?;
        create_schema(&conn)?;

        let files = collect_rs_files(dir);
        eprintln!("Indexing {} files...", files.len());

        let mut parser = TsParser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .context("failed to set tree-sitter language")?;

        for file in &files {
            let rel = file.strip_prefix(dir).unwrap_or(file);
            let rel_str = rel.to_string_lossy();
            index_file(&conn, &mut parser, &rel_str, file)?;
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
            eprintln!("Full index failed: {}", e);
            Err(e)
        }
    }
}

fn freshen(dir: &PathBuf, db_path: &PathBuf) -> Result<()> {
    let bak_path = db_path.with_extension("db.bak");

    if !db_path.exists() {
        if bak_path.exists() {
            eprintln!("Another freshen in progress, skipping.");
            return Ok(());
        }
        eprintln!("No existing database, doing full index.");
        return full_index(dir, db_path);
    }

    fs::rename(db_path, &bak_path)?;

    // Check schema version before attempting incremental freshen
    {
        let conn = Connection::open(&bak_path)?;
        if !schema_is_current(&conn) {
            drop(conn);
            fs::rename(&bak_path, db_path)?;
            eprintln!("Schema outdated, doing full reindex.");
            return full_index(dir, db_path);
        }
    }

    let result = (|| -> Result<(u64, u64)> {
        let conn = Connection::open(&bak_path)?;
        conn.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=OFF; PRAGMA busy_timeout=5000;")?;

        conn.execute_batch("BEGIN IMMEDIATE;")?;

        let mut parser = TsParser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .context("failed to set tree-sitter language")?;

        let disk_files = collect_rs_files(dir);
        let disk_set: HashSet<String> = disk_files
            .iter()
            .map(|f| f.strip_prefix(dir).unwrap_or(f).to_string_lossy().to_string())
            .collect();

        let mut stmt = conn.prepare("SELECT file, indexed_mtime, indexed_size FROM files")?;
        let db_files: Vec<(String, f64, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2).unwrap_or(0))))?
            .filter_map(|r| r.ok())
            .collect();
        let db_set: HashSet<String> = db_files.iter().map(|(f, _, _)| f.clone()).collect();

        let mut reindexed = 0u64;
        let mut removed = 0u64;

        for (db_file, _, _) in &db_files {
            if !disk_set.contains(db_file.as_str()) {
                delete_file_rows(&conn, db_file)?;
                removed += 1;
            }
        }

        let db_mtimes: HashMap<&str, (f64, i64)> = db_files.iter().map(|(f, m, s)| (f.as_str(), (*m, *s))).collect();

        for file in &disk_files {
            let rel = file.strip_prefix(dir).unwrap_or(file).to_string_lossy().to_string();
            let disk_mtime = file_mtime(file);
            let disk_size = file_size(file);

            let needs_index = if let Some(&(db_mtime, db_size)) = db_mtimes.get(rel.as_str()) {
                disk_mtime > db_mtime || disk_size != db_size
            } else {
                true
            };

            if needs_index {
                if db_set.contains(&rel) {
                    delete_file_rows(&conn, &rel)?;
                }
                index_file(&conn, &mut parser, &rel, file)?;
                reindexed += 1;
            }
        }

        if reindexed > 0 || removed > 0 {
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
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Freshen failed: {}. Database preserved as .bak.", e);
            Err(e)
        }
    }
}

// ─── File indexing ─────────────────────────────────────────────────────────

fn index_file(conn: &Connection, parser: &mut TsParser, rel_str: &str, abs_path: &PathBuf) -> Result<()> {
    let source = match fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("skip {}: {}", rel_str, e);
            return Ok(());
        }
    };

    let tree = match parser.parse(&source, None) {
        Some(t) => t,
        None => {
            eprintln!("parse error in {}: (indexing lines only)", rel_str);
            return index_file_lines_only(conn, rel_str, &source, abs_path);
        }
    };

    let src = source.as_bytes();

    // Extract everything in one pass
    let mut extractor = Extractor::new(rel_str.to_string(), &source);
    extractor.extract(tree.root_node());

    // Insert lines with symbol attribution
    insert_lines(conn, rel_str, &source, &extractor.symbol_ranges)?;

    // Insert extracted data
    insert_symbols(conn, &extractor.symbols)?;
    insert_calls(conn, &extractor.calls)?;
    insert_imports(conn, &extractor.imports)?;
    insert_defs(conn, &extractor.defs)?;
    insert_blocks(conn, &extractor.blocks)?;
    insert_match_arms(conn, &extractor.match_arms)?;
    insert_references(conn, &extractor.references)?;

    let line_count = source.lines().count() as i64;
    let mtime = file_mtime(abs_path);
    let size = file_size(abs_path);
    conn.execute(
        "INSERT INTO files (file, lines, language, indexed_mtime, indexed_size) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![rel_str, line_count, "rust", mtime, size],
    )?;
    Ok(())
}

fn index_file_lines_only(conn: &Connection, rel_str: &str, source: &str, abs_path: &PathBuf) -> Result<()> {
    let mut stmt = conn.prepare("INSERT INTO lines (file, line_num, content, symbol) VALUES (?1, ?2, ?3, '')")?;
    let mut stmt_fts = conn.prepare("INSERT INTO lines_fts (file, line_num, content, symbol) VALUES (?1, ?2, ?3, '')")?;
    for (i, line) in source.lines().enumerate() {
        let ln = (i + 1) as i64;
        stmt.execute(rusqlite::params![rel_str, ln, line])?;
        stmt_fts.execute(rusqlite::params![rel_str, ln, line])?;
    }
    let line_count = source.lines().count() as i64;
    let mtime = file_mtime(abs_path);
    let size = file_size(abs_path);
    conn.execute(
        "INSERT INTO files (file, lines, language, indexed_mtime, indexed_size, parse_error) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
        rusqlite::params![rel_str, line_count, "rust", mtime, size],
    )?;
    Ok(())
}

// ─── Tree-sitter extraction ───────────────────────────────────────────────

struct SymbolRecord {
    name: String,
    qualified_name: Option<String>,
    kind: String,
    file: String,
    start_line: usize,
    end_line: usize,
    parent: Option<String>,
    signature: Option<String>,
    cyclomatic: Option<i64>,
}

struct CallRecord {
    caller_file: String,
    caller_symbol: Option<String>,
    callee: String,
    callee_qname: Option<String>,
    kind: String,
    line_num: usize,
}

struct DefRecord {
    symbol: Option<String>,
    file: String,
    line_num: usize,
    var: String,
    kind: String,
}

struct ImportRecord {
    file: String,
    module: String,
    line_num: usize,
}

struct BlockRecord {
    file: String,
    open_line: usize,
    close_line: usize,
    kind: String,
    parent_symbol: Option<String>,
}

struct MatchArmRecord {
    file: String,
    parent_fn: Option<String>,
    pattern: String,
    start_line: usize,
    end_line: usize,
    body_start: usize,
    body_end: usize,
    has_early_return: bool,
}

struct ReferenceRecord {
    file: String,
    line_num: usize,
    name: String,
    kind: String, // "use", "def", "type", "call"
}

/// Range of lines covered by a symbol (for line attribution)
struct SymbolRange {
    name: String,
    start_line: usize,
    end_line: usize,
}

struct Extractor {
    file: String,
    source: String,
    symbols: Vec<SymbolRecord>,
    calls: Vec<CallRecord>,
    imports: Vec<ImportRecord>,
    defs: Vec<DefRecord>,
    blocks: Vec<BlockRecord>,
    match_arms: Vec<MatchArmRecord>,
    references: Vec<ReferenceRecord>,
    symbol_ranges: Vec<SymbolRange>,
    scope_stack: Vec<String>,
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
            blocks: Vec::new(),
            match_arms: Vec::new(),
            references: Vec::new(),
            symbol_ranges: Vec::new(),
            scope_stack: Vec::new(),
        }
    }

    fn current_scope(&self) -> Option<String> {
        self.scope_stack.last().cloned()
    }

    fn node_text<'a>(&'a self, node: Node) -> &'a str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    fn extract(&mut self, root: Node) {
        self.walk(root);
    }

    fn walk(&mut self, node: Node) {
        match node.kind() {
            "function_item" => self.extract_function(node),
            "impl_item" => self.extract_impl(node),
            "struct_item" => self.extract_struct(node),
            "enum_item" => self.extract_enum(node),
            "trait_item" => self.extract_trait(node),
            "const_item" => self.extract_const(node, "const"),
            "static_item" => self.extract_const(node, "static"),
            "type_item" => self.extract_const(node, "type"),
            "use_declaration" => self.extract_use(node),
            "call_expression" => self.extract_call(node),
            "macro_invocation" => self.extract_macro_call(node),
            "let_declaration" => self.extract_let(node),
            "match_expression" => self.extract_match(node),
            "block" | "declaration_list" => self.extract_block(node),
            "identifier" | "field_identifier" | "type_identifier" => self.extract_reference(node),
            _ => {
                // Recurse into children
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk(child);
                }
            }
        }
    }

    fn extract_function(&mut self, node: Node) {
        let name = self.find_child_text(node, "name")
            .or_else(|| self.find_field_text(node, "name"))
            .unwrap_or_default();
        if name.is_empty() { return; }

        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;

        let qualified = self.scope_stack.last().map(|p| format!("{}::{}", p, name));
        let parent = self.scope_stack.last().cloned();

        // Build signature from the function node text up to the body
        let sig = self.extract_signature(node);

        let kind = if parent.is_some() { "method" } else { "function" };

        // McCabe cyclomatic complexity over the body
        let cyclomatic = node
            .child_by_field_name("body")
            .map(|body| 1 + count_decision_points(body, self.source.as_bytes()));

        self.symbols.push(SymbolRecord {
            name: name.clone(),
            qualified_name: qualified.clone(),
            kind: kind.to_string(),
            file: self.file.clone(),
            start_line,
            end_line,
            parent,
            signature: Some(sig),
            cyclomatic: Some(cyclomatic.unwrap_or(1) as i64),
        });

        let sym_name = qualified.unwrap_or(name.clone());
        self.symbol_ranges.push(SymbolRange {
            name: sym_name.clone(),
            start_line,
            end_line,
        });

        // Extract parameters as defs
        if let Some(params) = node.child_by_field_name("parameters") {
            self.extract_params(params, &sym_name);
            // Also extract type references from parameter types
            self.walk_for_refs(params);
        }

        self.scope_stack.push(sym_name);
        // Walk body
        if let Some(body) = node.child_by_field_name("body") {
            self.walk(body);
        }
        self.scope_stack.pop();
    }

    fn extract_impl(&mut self, node: Node) {
        let type_node = node.child_by_field_name("type")
            .or_else(|| node.child_by_field_name("name"));
        let type_name = type_node.map(|n| self.node_text(n).to_string()).unwrap_or_default();
        if type_name.is_empty() { return; }

        // Record the type as a reference
        if let Some(tn) = type_node {
            self.references.push(ReferenceRecord {
                file: self.file.clone(),
                line_num: tn.start_position().row + 1,
                name: type_name.clone(),
                kind: "type".to_string(),
            });
        }

        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;

        // Check for trait impl
        let trait_name = node.child_by_field_name("trait").map(|n| self.node_text(n).to_string());
        let kind = if trait_name.is_some() { "trait_impl" } else { "impl" };

        self.symbols.push(SymbolRecord {
            name: type_name.clone(),
            qualified_name: trait_name.as_ref().map(|t| format!("<{} as {}>", type_name, t)),
            kind: kind.to_string(),
            file: self.file.clone(),
            start_line,
            end_line,
            parent: None,
            signature: trait_name.as_ref().map(|t| format!("impl {} for {}", t, type_name)),
            cyclomatic: None,
        });

        self.scope_stack.push(type_name);
        // Walk body (declaration_list)
        if let Some(body) = node.child_by_field_name("body") {
            self.walk(body);
        }
        self.scope_stack.pop();
    }

    fn extract_struct(&mut self, node: Node) {
        let name = self.find_field_text(node, "name").unwrap_or_default();
        if name.is_empty() { return; }
        self.symbols.push(SymbolRecord {
            name: name.clone(),
            qualified_name: None,
            kind: "struct".to_string(),
            file: self.file.clone(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            parent: None,
            signature: None,
            cyclomatic: None,
        });
        // Extract references from field types
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "field_declaration_list" {
                self.walk_for_refs(child);
            }
        }
    }

    fn extract_enum(&mut self, node: Node) {
        let name = self.find_field_text(node, "name").unwrap_or_default();
        if name.is_empty() { return; }
        self.symbols.push(SymbolRecord {
            name,
            qualified_name: None,
            kind: "enum".to_string(),
            file: self.file.clone(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            parent: None,
            signature: None,
            cyclomatic: None,
        });
    }

    fn extract_trait(&mut self, node: Node) {
        let name = self.find_field_text(node, "name").unwrap_or_default();
        if name.is_empty() { return; }
        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;
        self.symbols.push(SymbolRecord {
            name: name.clone(),
            qualified_name: None,
            kind: "trait".to_string(),
            file: self.file.clone(),
            start_line,
            end_line,
            parent: None,
            signature: None,
            cyclomatic: None,
        });
        self.scope_stack.push(name);
        if let Some(body) = node.child_by_field_name("body") {
            self.walk(body);
        }
        self.scope_stack.pop();
    }

    fn extract_const(&mut self, node: Node, kind: &str) {
        let name = self.find_field_text(node, "name").unwrap_or_default();
        if name.is_empty() { return; }
        self.symbols.push(SymbolRecord {
            name,
            qualified_name: None,
            kind: kind.to_string(),
            file: self.file.clone(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            parent: self.current_scope(),
            signature: None,
            cyclomatic: None,
        });
    }

    fn extract_use(&mut self, node: Node) {
        // Flatten use tree into module paths
        let text = self.node_text(node).to_string();
        let line = node.start_position().row + 1;
        // Simple: strip "use " prefix and ";" suffix, record whole path
        let module = text.trim_start_matches("use ")
            .trim_start_matches("pub use ")
            .trim_end_matches(';')
            .trim()
            .to_string();
        self.imports.push(ImportRecord {
            file: self.file.clone(),
            module,
            line_num: line,
        });
        // Also extract references from the use tree
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_use_refs(child);
        }
    }

    fn extract_use_refs(&mut self, node: Node) {
        match node.kind() {
            "identifier" | "type_identifier" => {
                let name = self.node_text(node).to_string();
                if !name.is_empty() && !is_keyword(&name) {
                    self.references.push(ReferenceRecord {
                        file: self.file.clone(),
                        line_num: node.start_position().row + 1,
                        name,
                        kind: "use".to_string(),
                    });
                }
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.extract_use_refs(child);
                }
            }
        }
    }

    fn extract_call(&mut self, node: Node) {
        let func_node = node.child_by_field_name("function");
        if let Some(func) = func_node {
            let callee = self.node_text(func).to_string();
            let kind = if func.kind() == "field_expression" { "method" } else { "function" };
            // For method calls, extract just the method name
            let callee_name = if kind == "method" {
                func.child_by_field_name("field")
                    .map(|n| self.node_text(n).to_string())
                    .unwrap_or(callee.clone())
            } else {
                callee.clone()
            };
            self.calls.push(CallRecord {
                caller_file: self.file.clone(),
                caller_symbol: self.current_scope(),
                callee: callee_name,
                callee_qname: None,
                kind: kind.to_string(),
                line_num: node.start_position().row + 1,
            });
        }
        // Recurse into arguments
        if let Some(args) = node.child_by_field_name("arguments") {
            let mut cursor = args.walk();
            for child in args.children(&mut cursor) {
                self.walk(child);
            }
        }
    }

    fn extract_macro_call(&mut self, node: Node) {
        let macro_node = node.child_by_field_name("macro");
        if let Some(m) = macro_node {
            let name = self.node_text(m).to_string();
            self.calls.push(CallRecord {
                caller_file: self.file.clone(),
                caller_symbol: self.current_scope(),
                callee: name,
                callee_qname: None,
                kind: "macro".to_string(),
                line_num: node.start_position().row + 1,
            });
        }
    }

    fn extract_let(&mut self, node: Node) {
        if let Some(pattern) = node.child_by_field_name("pattern") {
            let var = self.node_text(pattern).to_string();
            let is_mut = var.starts_with("mut ") || {
                let mut cursor = node.walk();
                node.children(&mut cursor).any(|c| c.kind() == "mutable_specifier")
            };
            let clean_var = var.trim_start_matches("mut ").to_string();
            self.defs.push(DefRecord {
                symbol: self.current_scope(),
                file: self.file.clone(),
                line_num: node.start_position().row + 1,
                var: clean_var.clone(),
                kind: if is_mut { "let_mut" } else { "let" }.to_string(),
            });
            // Also record as a reference of kind "def"
            self.references.push(ReferenceRecord {
                file: self.file.clone(),
                line_num: node.start_position().row + 1,
                name: clean_var,
                kind: "def".to_string(),
            });
        }
        // Recurse into value
        if let Some(value) = node.child_by_field_name("value") {
            self.walk(value);
        }
    }

    fn extract_match(&mut self, node: Node) {
        // Record each match arm
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "match_arm" {
                    self.extract_match_arm(child);
                }
            }
        }
        // Also extract the match value expression
        if let Some(value) = node.child_by_field_name("value") {
            self.walk(value);
        }
    }

    fn extract_match_arm(&mut self, node: Node) {
        let pattern_node = node.child_by_field_name("pattern");
        let pattern = pattern_node.map(|n| self.node_text(n).to_string()).unwrap_or_default();

        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;

        // Body: if value is a block, body_start is the line after the opening {
        // otherwise it's the value expression's start line
        let value_node = node.child_by_field_name("value");
        let (body_start, body_end) = if let Some(val) = value_node {
            if val.kind() == "block" {
                // Block: body starts at first line inside {, ends at line before }
                let block_start = val.start_position().row + 1;
                let block_end = val.end_position().row + 1;
                // If block spans multiple lines, body is interior
                if block_end > block_start {
                    (block_start + 1, block_end - 1)
                } else {
                    (block_start, block_end)
                }
            } else {
                (val.start_position().row + 1, val.end_position().row + 1)
            }
        } else {
            (start_line, end_line)
        };

        // Check for early returns (return_expression not inside a closure)
        let has_early_return = value_node
            .map(|val| has_return_outside_closure(val))
            .unwrap_or(false);

        self.match_arms.push(MatchArmRecord {
            file: self.file.clone(),
            parent_fn: self.current_scope(),
            pattern,
            start_line,
            end_line,
            body_start,
            body_end,
            has_early_return,
        });

        // Recurse into the arm value
        if let Some(value) = value_node {
            self.walk(value);
        }
    }

    fn extract_block(&mut self, node: Node) {
        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;

        // Only record multi-line blocks
        if end_line > start_line {
            // Determine block kind from parent
            let kind = node.parent().map(|p| match p.kind() {
                "function_item" => "fn",
                "impl_item" => "impl",
                "trait_item" => "trait",
                "if_expression" => "if",
                "else_clause" => "else",
                "match_expression" => "match",
                "for_expression" => "for",
                "while_expression" => "while",
                "loop_expression" => "loop",
                "closure_expression" => "closure",
                "unsafe_block" => "unsafe",
                _ => "block",
            }).unwrap_or("block");

            self.blocks.push(BlockRecord {
                file: self.file.clone(),
                open_line: start_line,
                close_line: end_line,
                kind: kind.to_string(),
                parent_symbol: self.current_scope(),
            });
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child);
        }
    }

    fn extract_reference(&mut self, node: Node) {
        let name = self.node_text(node).to_string();
        if name.is_empty() || name == "_" { return; }

        // Skip keywords that tree-sitter might classify as identifiers
        if is_keyword(&name) { return; }

        let kind = match node.kind() {
            "type_identifier" => "type",
            "field_identifier" => "field",
            _ => {
                // Check parent context to determine kind
                if let Some(parent) = node.parent() {
                    match parent.kind() {
                        "call_expression" => "call",
                        "function_item" | "let_declaration" => "def",
                        "type_identifier" | "generic_type" => "type",
                        _ => "use",
                    }
                } else {
                    "use"
                }
            }
        };

        self.references.push(ReferenceRecord {
            file: self.file.clone(),
            line_num: node.start_position().row + 1,
            name,
            kind: kind.to_string(),
        });
    }

    fn extract_params(&mut self, params_node: Node, fn_name: &str) {
        let mut cursor = params_node.walk();
        for child in params_node.children(&mut cursor) {
            if child.kind() == "parameter" || child.kind() == "self_parameter" {
                if let Some(pattern) = child.child_by_field_name("pattern") {
                    let var = self.node_text(pattern).to_string();
                    self.defs.push(DefRecord {
                        symbol: Some(fn_name.to_string()),
                        file: self.file.clone(),
                        line_num: child.start_position().row + 1,
                        var: var.clone(),
                        kind: "parameter".to_string(),
                    });
                    self.references.push(ReferenceRecord {
                        file: self.file.clone(),
                        line_num: child.start_position().row + 1,
                        name: var,
                        kind: "def".to_string(),
                    });
                }
            }
        }
    }

    fn extract_signature(&self, node: Node) -> String {
        // Get text from function start up to (but not including) the body block
        if let Some(body) = node.child_by_field_name("body") {
            let start = node.start_byte();
            let end = body.start_byte();
            let sig_bytes = &self.source.as_bytes()[start..end];
            std::str::from_utf8(sig_bytes).unwrap_or("").trim().to_string()
        } else {
            self.node_text(node).to_string()
        }
    }

    fn find_child_text(&self, node: Node, field: &str) -> Option<String> {
        node.child_by_field_name(field).map(|n| self.node_text(n).to_string())
    }

    fn find_field_text(&self, node: Node, field: &str) -> Option<String> {
        node.child_by_field_name(field).map(|n| self.node_text(n).to_string())
    }

    /// Walk a subtree extracting only references (identifiers/types)
    fn walk_for_refs(&mut self, node: Node) {
        match node.kind() {
            "identifier" | "field_identifier" | "type_identifier" => {
                self.extract_reference(node);
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_for_refs(child);
                }
            }
        }
    }
}

/// Check if a subtree contains a `return` expression not nested inside a closure.
fn has_return_outside_closure(node: Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "return_expression" => return true,
            "closure_expression" => continue, // don't recurse into closures
            _ => {
                if has_return_outside_closure(child) {
                    return true;
                }
            }
        }
    }
    false
}

/// Count McCabe decision points inside a function body. The cyclomatic
/// complexity of the function is `1 + count_decision_points(body)`.
///
/// Convention chosen to match Mozilla's `rust-code-analysis` so numbers
/// are comparable to that reference. Counted nodes for tree-sitter-rust:
///
///   - `if_expression` — every `if`, including `if let` (the parser
///     uses a unified `if_expression` node with an optional `let_condition`
///     child for `if let`).
///   - `while_expression` — every `while` and `while let`, same reason.
///   - `for_expression`, `loop_expression` — loop entry is a branch.
///   - `match_arm` — each arm adds one. `|`-alternated patterns
///     (`A | B | C => ...`) count as a single decision, matching
///     McCabe original and Mozilla's `rust-code-analysis`. Counting
///     each alternative was tested against safebash and over-counted
///     against the reference (e.g. lexer `b' ' | b'\t' | ...` arms
///     with 17 alternatives became +17 instead of +1).
///   - `let_declaration` with a `block` child — let-else.
///   - `match_pattern` with named children beyond the pattern itself —
///     the `if cond` guard.
///   - `try_expression` — the `?` operator (success vs. error
///     propagation).
///   - `binary_expression` whose operator is `&&` or `||` — short-circuit.
///
/// Does not recurse into nested function/closure bodies — each nested
/// function is its own symbol with its own complexity row.
/// Count McCabe decision points inside a function body. The cyclomatic
/// complexity of the function is `1 + count_decision_points(body)`.
///
/// Convention chosen to match Mozilla's `rust-code-analysis` per-function
/// `cyclomatic.sum` output (the value safebash's contrib/cyclo.sh and other
/// scripts typically read), so numbers are comparable to that reference.
///
/// Mozilla's per-node Rust rule is exactly:
///
///   If | For | While | Loop | MatchArm | MatchArm2 | TryExpression | && | ||
///
/// Mozilla also treats every `ClosureExpression` and nested `FunctionItem`
/// as a separate "space" with its own CC, then merges child-space sums
/// upward — so a function's reported `cyclomatic.sum` includes the CC of
/// every closure/nested-fn defined inside it. We emulate that by:
///   - counting +1 for the closure or nested-fn itself (its "base 1" CC),
///   - then recursing into its body and counting its decisions as if they
///     were part of the enclosing function.
///
/// What we explicitly do NOT count (Mozilla doesn't either, and counting
/// them was tested empirically and rejected):
///   - `matches!` macro invocations
///   - `let-else` declarations
///   - match arm guards (`pat if cond => ...`)
///   - `|`-alternated patterns (`A | B | C => ...` is one decision)
fn count_decision_points(node: Node, src: &[u8]) -> usize {
    let _ = src; // src reserved for future per-node text checks
    let mut count = 0;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // Closures and nested item-level fns: +1 for the new space's
            // base CC, then recurse so the inner decisions also accumulate.
            // This mirrors Mozilla's behavior of merging child-space
            // cyclomatic_sum into the parent.
            "function_item" | "closure_expression" => {
                count += 1;
                count += count_decision_points(child, src);
            }
            "if_expression"
            | "while_expression"
            | "for_expression"
            | "loop_expression"
            | "match_arm"
            | "try_expression" => {
                count += 1;
                count += count_decision_points(child, src);
            }
            "binary_expression" => {
                if let Some(op) = child.child_by_field_name("operator") {
                    let op_kind = op.kind();
                    if op_kind == "&&" || op_kind == "||" {
                        count += 1;
                    }
                }
                count += count_decision_points(child, src);
            }
            _ => {
                count += count_decision_points(child, src);
            }
        }
    }
    count
}

fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "self" | "Self" | "super" | "crate" | "true" | "false"
            | "fn" | "let" | "mut" | "pub" | "use" | "mod" | "struct"
            | "enum" | "trait" | "impl" | "for" | "while" | "loop"
            | "if" | "else" | "match" | "return" | "break" | "continue"
            | "as" | "in" | "ref" | "where" | "type" | "const" | "static"
            | "async" | "await" | "move" | "dyn" | "unsafe" | "extern"
    )
}

// ─── Line insertion with symbol attribution ───────────────────────────────

fn insert_lines(conn: &Connection, file: &str, source: &str, symbol_ranges: &[SymbolRange]) -> Result<()> {
    // Sort ranges by specificity (smallest range = most specific)
    let mut ranges = symbol_ranges.to_vec();
    ranges.sort_by_key(|r| std::cmp::Reverse(r.end_line - r.start_line));
    // For each line, find the most specific (smallest) enclosing symbol
    // Actually sort by size ascending so we can overwrite with smaller
    ranges.sort_by_key(|r| r.end_line - r.start_line);

    let line_count = source.lines().count();
    let mut line_symbols: Vec<Option<&str>> = vec![None; line_count + 1];

    for range in &ranges {
        for l in range.start_line..=range.end_line.min(line_count) {
            line_symbols[l] = Some(&range.name);
        }
    }

    let mut stmt = conn.prepare("INSERT INTO lines (file, line_num, content, symbol) VALUES (?1, ?2, ?3, ?4)")?;
    let mut stmt_fts = conn.prepare("INSERT INTO lines_fts (file, line_num, content, symbol) VALUES (?1, ?2, ?3, ?4)")?;
    for (i, line_content) in source.lines().enumerate() {
        let ln = (i + 1) as i64;
        let sym = line_symbols.get(i + 1).and_then(|s| *s);
        stmt.execute(rusqlite::params![file, ln, line_content, sym])?;
        stmt_fts.execute(rusqlite::params![file, ln, line_content, sym])?;
    }
    Ok(())
}

impl Clone for SymbolRange {
    fn clone(&self) -> Self {
        SymbolRange {
            name: self.name.clone(),
            start_line: self.start_line,
            end_line: self.end_line,
        }
    }
}

// ─── Schema ───────────────────────────────────────────────────────────────

fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode=DELETE;
        PRAGMA synchronous=OFF;
        PRAGMA user_version=9;

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
            signature      TEXT,
            cyclomatic     INTEGER
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

        CREATE TABLE blocks (
            file          TEXT,
            open_line     INTEGER,
            close_line    INTEGER,
            kind          TEXT,
            parent_symbol TEXT
        );

        CREATE TABLE match_arms (
            file        TEXT,
            parent_fn   TEXT,
            pattern     TEXT,
            start_line  INTEGER,
            end_line    INTEGER,
            body_start  INTEGER,
            body_end    INTEGER,
            has_early_return INTEGER DEFAULT 0
        );

        CREATE TABLE refs (
            file     TEXT,
            line_num INTEGER,
            name     TEXT,
            kind     TEXT
        );

        CREATE INDEX idx_symbols_name ON symbols(name);
        CREATE INDEX idx_symbols_qualified ON symbols(qualified_name);
        CREATE INDEX idx_symbols_file ON symbols(file);
        CREATE INDEX idx_symbols_cyclomatic ON symbols(cyclomatic);
        CREATE INDEX idx_calls_callee ON calls(callee);
        CREATE INDEX idx_calls_caller ON calls(caller_symbol);
        CREATE INDEX idx_defs_var ON defs(var);
        CREATE INDEX idx_imports_module ON imports(module);
        CREATE INDEX idx_blocks_file ON blocks(file);
        CREATE INDEX idx_blocks_open ON blocks(file, open_line);
        CREATE INDEX idx_match_arms_file ON match_arms(file);
        CREATE INDEX idx_refs_file_line ON refs(file, line_num);
        CREATE INDEX idx_refs_name ON refs(name);

        CREATE TABLE files (
            file           TEXT PRIMARY KEY,
            lines          INTEGER,
            language       TEXT,
            indexed_mtime  REAL,
            indexed_size   INTEGER DEFAULT 0,
            parse_error    INTEGER DEFAULT 0
        );

        CREATE TABLE directories (
            path        TEXT PRIMARY KEY,
            file_count  INTEGER,
            total_lines INTEGER
        );

        CREATE VIEW callee_candidates AS
        SELECT
            c.caller_file,
            c.caller_symbol,
            c.callee,
            c.callee_qname,
            c.line_num       AS call_line,
            c.kind           AS call_kind,
            s.qualified_name AS candidate_qname,
            s.file           AS def_file,
            s.start_line     AS def_line,
            s.parent         AS def_parent,
            s.signature
        FROM calls c
        LEFT JOIN symbols s
            ON s.name = c.callee
           AND s.kind IN ('function','method')
           AND (c.callee_qname IS NULL OR s.qualified_name = c.callee_qname);

        CREATE VIEW string_literals AS
        WITH RECURSIVE peeled(file, line_num, symbol, content, value, rest, pos) AS (
            SELECT file, line_num, symbol, content,
                   substr(content,
                          instr(content, '\"') + 1,
                          instr(substr(content, instr(content, '\"') + 1), '\"') - 1),
                   substr(substr(content, instr(content, '\"') + 1),
                          instr(substr(content, instr(content, '\"') + 1), '\"') + 1),
                   1
            FROM lines
            WHERE content LIKE '%\"%\"%'
              AND trim(content) NOT LIKE '//%'
            UNION ALL
            SELECT file, line_num, symbol, content,
                   substr(rest, instr(rest, '\"') + 1,
                          instr(substr(rest, instr(rest, '\"') + 1), '\"') - 1),
                   substr(substr(rest, instr(rest, '\"') + 1),
                          instr(substr(rest, instr(rest, '\"') + 1), '\"') + 1),
                   pos + 1
            FROM peeled
            WHERE rest LIKE '%\"%\"%' AND pos < 8
        )
        SELECT file, line_num, symbol, value, pos,
               CASE
                   WHEN content LIKE '%==%\"' || value || '\"%'
                     OR content LIKE '%!=%\"' || value || '\"%' THEN 'compare'
                   WHEN content LIKE '%\"' || value || '\"%=>%'
                     OR content LIKE '%\"' || value || '\" |%'
                     OR content LIKE '%| \"' || value || '\"%'  THEN 'match_pat'
                   WHEN content LIKE '%format!%'
                     OR content LIKE '%println!%'
                     OR content LIKE '%write!%'
                     OR content LIKE '%writeln!%'
                     OR content LIKE '%eprintln!%'
                     OR content LIKE '%panic!%'               THEN 'fmt'
                   ELSE 'arg'
               END AS context
        FROM peeled
        WHERE value != '';

        CREATE VIEW block_dupes AS
        WITH ln AS (
            SELECT file, line_num, trim(lower(content)) AS norm,
                   ROW_NUMBER() OVER (PARTITION BY file ORDER BY line_num) AS seq
            FROM lines
            WHERE trim(content) != ''
        ),
        sh AS (
            SELECT a.file, a.line_num AS start_line, d.line_num AS end_line,
                   a.norm || char(10) || b.norm || char(10) || c.norm || char(10) || d.norm AS shingle
            FROM ln a
            JOIN ln b ON b.file = a.file AND b.seq = a.seq + 1
            JOIN ln c ON c.file = a.file AND c.seq = a.seq + 2
            JOIN ln d ON d.file = a.file AND d.seq = a.seq + 3
        )
        SELECT shingle,
               COUNT(*) AS occurrences,
               GROUP_CONCAT(file || ':' || start_line, ' | ') AS sites
        FROM sh
        GROUP BY shingle
        HAVING COUNT(*) >= 2;
        ",
    )?;
    Ok(())
}

/// Schema version — bump when tables/columns change.
/// freshen checks this and falls back to full_index if outdated.
const SCHEMA_VERSION: i64 = 9;

/// Returns true if the DB schema is current. If outdated, freshen
/// should abandon incremental update and do a full rebuild.
fn schema_is_current(conn: &Connection) -> bool {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap_or(0);
    version >= SCHEMA_VERSION
}

// ─── DB insertion ─────────────────────────────────────────────────────────

fn insert_symbols(conn: &Connection, symbols: &[SymbolRecord]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO symbols (name, qualified_name, kind, file, start_line, end_line, parent, signature, cyclomatic)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for s in symbols {
        stmt.execute(rusqlite::params![s.name, s.qualified_name, s.kind, s.file, s.start_line as i64, s.end_line as i64, s.parent, s.signature, s.cyclomatic])?;
    }
    Ok(())
}

fn insert_calls(conn: &Connection, calls: &[CallRecord]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO calls (caller_file, caller_symbol, callee, callee_qname, kind, line_num) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for c in calls {
        stmt.execute(rusqlite::params![c.caller_file, c.caller_symbol, c.callee, c.callee_qname, c.kind, c.line_num as i64])?;
    }
    Ok(())
}

fn insert_imports(conn: &Connection, imports: &[ImportRecord]) -> Result<()> {
    let mut stmt = conn.prepare("INSERT INTO imports (file, module, line_num) VALUES (?1, ?2, ?3)")?;
    for i in imports {
        stmt.execute(rusqlite::params![i.file, i.module, i.line_num as i64])?;
    }
    Ok(())
}

fn insert_defs(conn: &Connection, defs: &[DefRecord]) -> Result<()> {
    let mut stmt = conn.prepare("INSERT INTO defs (symbol, file, line_num, var, kind) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for d in defs {
        stmt.execute(rusqlite::params![d.symbol, d.file, d.line_num as i64, d.var, d.kind])?;
    }
    Ok(())
}

fn insert_blocks(conn: &Connection, blocks: &[BlockRecord]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO blocks (file, open_line, close_line, kind, parent_symbol) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for b in blocks {
        stmt.execute(rusqlite::params![b.file, b.open_line as i64, b.close_line as i64, b.kind, b.parent_symbol])?;
    }
    Ok(())
}

fn insert_match_arms(conn: &Connection, arms: &[MatchArmRecord]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO match_arms (file, parent_fn, pattern, start_line, end_line, body_start, body_end, has_early_return) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for a in arms {
        stmt.execute(rusqlite::params![a.file, a.parent_fn, a.pattern, a.start_line as i64, a.end_line as i64, a.body_start as i64, a.body_end as i64, a.has_early_return as i64])?;
    }
    Ok(())
}

fn insert_references(conn: &Connection, refs: &[ReferenceRecord]) -> Result<()> {
    let mut stmt = conn.prepare("INSERT INTO refs (file, line_num, name, kind) VALUES (?1, ?2, ?3, ?4)")?;
    for r in refs {
        stmt.execute(rusqlite::params![r.file, r.line_num as i64, r.name, r.kind])?;
    }
    Ok(())
}

// ─── Utilities ────────────────────────────────────────────────────────────

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

fn delete_file_rows(conn: &Connection, file: &str) -> Result<()> {
    conn.execute("DELETE FROM lines WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM lines_fts WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM symbols WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM calls WHERE caller_file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM imports WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM defs WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM blocks WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM match_arms WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM refs WHERE file = ?1", rusqlite::params![file])?;
    conn.execute("DELETE FROM files WHERE file = ?1", rusqlite::params![file])?;
    Ok(())
}

fn file_mtime(path: &PathBuf) -> f64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64())
        .unwrap_or(0.0)
}

fn file_size(path: &PathBuf) -> i64 {
    fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0)
}

fn finalize(conn: &Connection) -> Result<()> {
    populate_directories(conn)?;
    Ok(())
}

fn populate_directories(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("SELECT file, lines FROM files")?;
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut dirs: HashMap<String, (i64, i64)> = HashMap::new();
    for (file, lines) in &rows {
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

    let mut stmt = conn.prepare("INSERT INTO directories (path, file_count, total_lines) VALUES (?1, ?2, ?3)")?;
    for (path, (file_count, total_lines)) in &dirs {
        stmt.execute(rusqlite::params![path, file_count, total_lines])?;
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"use std::fs;
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

    const MATCH_SAMPLE: &str = r#"fn process(cmd: Command) -> Result<()> {
    match cmd {
        Command::Add { name, value } => {
            println!("adding {} = {}", name, value);
            db.insert(name, value);
            Ok(())
        }
        Command::Remove { name } => {
            db.remove(&name);
            Ok(())
        }
        Command::List => {
            for (k, v) in db.iter() {
                println!("{}: {}", k, v);
            }
            Ok(())
        }
    }
}
"#;

    fn make_parser() -> TsParser {
        let mut parser = TsParser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        parser
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    fn index_sample(conn: &Connection, source: &str) {
        let mut parser = make_parser();
        let tree = parser.parse(source, None).unwrap();
        let mut extractor = Extractor::new("test.rs".to_string(), source);
        extractor.extract(tree.root_node());
        insert_lines(conn, "test.rs", source, &extractor.symbol_ranges).unwrap();
        insert_symbols(conn, &extractor.symbols).unwrap();
        insert_calls(conn, &extractor.calls).unwrap();
        insert_imports(conn, &extractor.imports).unwrap();
        insert_defs(conn, &extractor.defs).unwrap();
        insert_blocks(conn, &extractor.blocks).unwrap();
        insert_match_arms(conn, &extractor.match_arms).unwrap();
        insert_references(conn, &extractor.references).unwrap();
        let line_count = source.lines().count() as i64;
        conn.execute(
            "INSERT INTO files (file, lines, language, indexed_mtime, indexed_size) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["test.rs", line_count, "rust", 0.0, 0i64],
        ).unwrap();
    }

    #[test]
    fn symbols_extracted() {
        let conn = setup_db();
        index_sample(&conn, SAMPLE);

        let syms: Vec<(String, String)> = conn
            .prepare("SELECT name, kind FROM symbols ORDER BY start_line")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let names: Vec<&str> = syms.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"Point"), "missing struct Point: {:?}", names);
        assert!(names.contains(&"main"), "missing fn main: {:?}", names);
        assert!(names.contains(&"add"), "missing fn add: {:?}", names);
        assert!(names.contains(&"new"), "missing method new: {:?}", names);
        assert!(names.contains(&"distance"), "missing method distance: {:?}", names);
    }

    #[test]
    fn blocks_extracted() {
        let conn = setup_db();
        index_sample(&conn, SAMPLE);

        let blocks: Vec<(i64, i64, String)> = conn
            .prepare("SELECT open_line, close_line, kind FROM blocks WHERE file='test.rs' ORDER BY open_line")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(!blocks.is_empty(), "should have extracted blocks");
        // Should have fn blocks
        let fn_blocks: Vec<_> = blocks.iter().filter(|(_, _, k)| k == "fn").collect();
        assert!(fn_blocks.len() >= 3, "expected at least 3 fn blocks (new, distance, add, main), got {:?}", fn_blocks);
    }

    #[test]
    fn match_arms_extracted() {
        let conn = setup_db();
        index_sample(&conn, MATCH_SAMPLE);

        let arms: Vec<(String, i64, i64, i64, i64)> = conn
            .prepare("SELECT pattern, start_line, end_line, body_start, body_end FROM match_arms WHERE file='test.rs' ORDER BY start_line")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(arms.len(), 3, "expected 3 match arms, got {:?}", arms);
        assert!(arms[0].0.contains("Add"), "first arm should be Add: {:?}", arms[0].0);
        assert!(arms[1].0.contains("Remove"), "second arm should be Remove: {:?}", arms[1].0);
        assert!(arms[2].0.contains("List"), "third arm should be List: {:?}", arms[2].0);

        // body_start should be the first line inside the { block, not the { itself
        // MATCH_SAMPLE line 3 is `Command::Add { name, value } => {`
        // line 4 is `            println!(...)` — that's body_start
        let (_, _, _, body_start, body_end) = &arms[0];
        // The Add arm body should start at the println line, not the { line
        assert!(*body_start > arms[0].1, "body_start ({}) should be after arm start_line ({})", body_start, arms[0].1);
    }

    #[test]
    fn references_extracted() {
        let conn = setup_db();
        index_sample(&conn, SAMPLE);

        let refs: Vec<(String, String)> = conn
            .prepare("SELECT DISTINCT name, kind FROM refs WHERE file='test.rs' ORDER BY name")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let ref_names: Vec<&str> = refs.iter().map(|(n, _)| n.as_str()).collect();
        assert!(ref_names.contains(&"Point"), "missing reference to Point: {:?}", ref_names);
        assert!(ref_names.contains(&"HashMap"), "missing reference to HashMap: {:?}", ref_names);
    }

    #[test]
    fn free_vars_query() {
        let conn = setup_db();
        index_sample(&conn, SAMPLE);

        // Free vars for the distance method body: identifiers used but not defined within
        // The distance method should reference 'other', 'self' (params) and 'dx', 'dy' (local defs)
        // A proper free-var query would be:
        let free_vars: Vec<String> = conn
            .prepare(
                "SELECT DISTINCT r.name FROM refs r
                 WHERE r.file='test.rs' AND r.line_num BETWEEN 15 AND 18
                 AND r.name NOT IN (
                     SELECT name FROM refs WHERE file='test.rs' AND line_num BETWEEN 15 AND 18 AND kind='def'
                 )
                 ORDER BY r.name"
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        // Should include references to things defined outside the range
        assert!(!free_vars.is_empty(), "should have some free variables");
    }

    #[test]
    fn imports_extracted() {
        let conn = setup_db();
        index_sample(&conn, SAMPLE);

        let imports: Vec<String> = conn
            .prepare("SELECT module FROM imports WHERE file='test.rs' ORDER BY module")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(imports.iter().any(|m| m.contains("std::fs")), "missing std::fs: {:?}", imports);
        assert!(imports.iter().any(|m| m.contains("HashMap")), "missing HashMap import: {:?}", imports);
    }

    #[test]
    fn lines_have_symbol_attribution() {
        let conn = setup_db();
        index_sample(&conn, SAMPLE);

        let main_sym: Option<String> = conn
            .query_row(
                "SELECT symbol FROM lines WHERE file='test.rs' AND content LIKE '%Point::new(1.0%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(main_sym.as_deref(), Some("main"), "line in main should be attributed to main");
    }

    #[test]
    fn freshen_works() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join(".indexnudge-rust.db");
        let rs_file = dir.path().join("lib.rs");
        fs::write(&rs_file, "fn hello() {}\n").unwrap();

        let dir_path = dir.path().to_path_buf();
        full_index(&dir_path, &db_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn.query_row("SELECT count(*) FROM symbols WHERE name='hello'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        drop(conn);

        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(&rs_file, "fn goodbye() {}\nfn hello() {}\n").unwrap();
        freshen(&dir_path, &db_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn.query_row("SELECT count(*) FROM symbols WHERE name='goodbye'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "freshen should have picked up new symbol");
    }

    #[test]
    fn block_span_query() {
        let conn = setup_db();
        index_sample(&conn, MATCH_SAMPLE);

        // The function body block should span from line 1 to the end
        let close: Option<i64> = conn
            .query_row(
                "SELECT close_line FROM blocks WHERE file='test.rs' AND kind='fn' ORDER BY open_line LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        assert!(close.is_some(), "should find a fn block");
        // Function ends at the last line of MATCH_SAMPLE
        let last_line = MATCH_SAMPLE.lines().count() as i64;
        assert_eq!(close.unwrap(), last_line, "fn block should close at last line");
    }
}

#[cfg(test)]

#[cfg(test)]
mod freshen_size_tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn freshen_detects_same_mtime_different_size() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join(".indexnudge-rust.db");
        let rs_file = dir.path().join("lib.rs");
        fs::write(&rs_file, "fn hello() {}\n").unwrap();

        let dir_path = dir.path().to_path_buf();
        full_index(&dir_path, &db_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn.query_row("SELECT count(*) FROM symbols WHERE name='hello'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        drop(conn);

        // Get the file's mtime as a touch-compatible timestamp
        let original_mtime = file_mtime(&rs_file);
        let secs = original_mtime as u64;

        // Write different (larger) content
        fs::write(&rs_file, "fn goodbye() {}\nfn extra() {}\n").unwrap();

        // Reset mtime to original using touch --date
        Command::new("touch")
            .arg("-d")
            .arg(format!("@{}", secs))
            .arg(rs_file.to_str().unwrap())
            .status()
            .expect("touch failed");

        // Verify mtime looks the same (within 1 second)
        let new_mtime = file_mtime(&rs_file);
        assert!((new_mtime - original_mtime).abs() < 1.1, "mtime should be ~unchanged, got {} vs {}", new_mtime, original_mtime);

        // Freshen should detect via size difference
        freshen(&dir_path, &db_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn.query_row("SELECT count(*) FROM symbols WHERE name='goodbye'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "freshen should detect size change even with same mtime");
    }
}

#[cfg(test)]
mod freshen_stores_size_tests {
    use super::*;

    #[test]
    fn freshen_stores_nonzero_indexed_size() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join(".indexnudge-rust.db");
        let rs_file = dir.path().join("lib.rs");
        fs::write(&rs_file, "fn hello() {}\n").unwrap();

        let dir_path = dir.path().to_path_buf();
        full_index(&dir_path, &db_path).unwrap();

        // Verify full_index stores size
        let conn = Connection::open(&db_path).unwrap();
        let size: i64 = conn.query_row("SELECT indexed_size FROM files WHERE file='lib.rs'", [], |row| row.get(0)).unwrap();
        assert!(size > 0, "full_index should store nonzero indexed_size, got {}", size);
        assert_eq!(size, 14, "expected 14 bytes");
        drop(conn);

        // Edit and freshen
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(&rs_file, "fn hello() {}\nfn world() {}\n").unwrap();
        freshen(&dir_path, &db_path).unwrap();

        // Verify freshen stores updated size
        let conn = Connection::open(&db_path).unwrap();
        let size: i64 = conn.query_row("SELECT indexed_size FROM files WHERE file='lib.rs'", [], |row| row.get(0)).unwrap();
        assert_eq!(size, 28, "freshen should store updated indexed_size, got {}", size);
    }
}

#[cfg(test)]
mod early_return_tests {
    use super::*;

    const EARLY_RETURN_SAMPLE: &str = r#"fn process(x: i32) -> i32 {
    match x {
        1 => {
            println!("one");
            1
        }
        2 => {
            if x > 0 {
                return -1;
            }
            2
        }
        3 => {
            let f = || return 99;
            3
        }
        _ => 0,
    }
}
"#;

    #[test]
    fn early_return_detected() {
        let conn = setup_db();
        index_sample(&conn, EARLY_RETURN_SAMPLE);

        let arms: Vec<(String, i64)> = conn
            .prepare("SELECT pattern, has_early_return FROM match_arms WHERE file='test.rs' ORDER BY start_line")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(arms.len(), 4, "expected 4 arms: {:?}", arms);
        assert_eq!(arms[0].1, 0, "arm 1 (pattern=1) has no early return");
        assert_eq!(arms[1].1, 1, "arm 2 (pattern=2) has early return");
        assert_eq!(arms[2].1, 0, "arm 3 (pattern=3) return is inside closure, not early");
        assert_eq!(arms[3].1, 0, "arm _ has no early return");
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    fn index_sample(conn: &Connection, source: &str) {
        let mut parser = TsParser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut extractor = Extractor::new("test.rs".to_string(), source);
        extractor.extract(tree.root_node());
        insert_lines(conn, "test.rs", source, &extractor.symbol_ranges).unwrap();
        insert_symbols(conn, &extractor.symbols).unwrap();
        insert_calls(conn, &extractor.calls).unwrap();
        insert_imports(conn, &extractor.imports).unwrap();
        insert_defs(conn, &extractor.defs).unwrap();
        insert_blocks(conn, &extractor.blocks).unwrap();
        insert_match_arms(conn, &extractor.match_arms).unwrap();
        insert_references(conn, &extractor.references).unwrap();
        let line_count = source.lines().count() as i64;
        conn.execute(
            "INSERT INTO files (file, lines, language, indexed_mtime, indexed_size) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["test.rs", line_count, "rust", 0.0, 0i64],
        ).unwrap();
    }
}

#[cfg(test)]
mod cyclomatic_tests {
    use super::*;

    const CC_SAMPLE: &str = r#"fn trivial(x: i32) -> i32 {
    x + 1
}

fn linear(x: i32) -> i32 {
    let y = x + 1;
    let z = y * 2;
    z
}

fn one_branch(x: i32) -> i32 {
    if x > 0 { 1 } else { -1 }
}

fn match_three(x: i32) -> i32 {
    match x {
        0 => 0,
        1 => 1,
        _ => 99,
    }
}

fn short_circuit(a: bool, b: bool, c: bool) -> bool {
    a && (b || c)
}

fn loop_and_try(xs: &[i32]) -> Result<i32, ()> {
    let mut total = 0;
    for x in xs {
        let v: i32 = if *x > 0 { *x } else { 0 };
        total += v;
    }
    Ok(total)
}

fn outer_with_closure(x: i32) -> i32 {
    let inner = |y: i32| {
        if y > 0 { y } else { -y }
    };
    inner(x)
}

fn if_let_test(x: Option<i32>) -> i32 {
    if let Some(v) = x { v } else { 0 }
}

fn let_else_test(x: Option<i32>) -> i32 {
    let Some(v) = x else { return -1 };
    v
}

fn match_with_guard(x: i32) -> i32 {
    match x {
        n if n < 0 => -n,
        0 => 0,
        n => n,
    }
}

fn match_with_or_pattern(x: i32) -> i32 {
    match x {
        1 | 2 | 3 => 1,
        4 | 5 => 2,
        _ => 0,
    }
}
"#;

    #[test]
    fn cyclomatic_computed_for_functions() {
        let conn = setup_db();
        index_sample(&conn, CC_SAMPLE);

        let rows: Vec<(String, Option<i64>)> = conn
            .prepare("SELECT name, cyclomatic FROM symbols WHERE file='test.rs' AND kind IN ('function','method') ORDER BY start_line")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let lookup: std::collections::HashMap<_, _> = rows.iter().cloned().collect();

        assert_eq!(lookup.get("trivial"),            Some(&Some(1)),  "trivial: no branches → 1");
        assert_eq!(lookup.get("linear"),             Some(&Some(1)),  "linear: no branches → 1");
        assert_eq!(lookup.get("one_branch"),         Some(&Some(2)),  "one if → 2");
        assert_eq!(lookup.get("match_three"),        Some(&Some(4)),  "1 + 3 arms = 4");
        assert_eq!(lookup.get("short_circuit"),      Some(&Some(3)),  "&& and || each add 1 → 3");
        assert_eq!(lookup.get("loop_and_try"),       Some(&Some(3)),  "for + if = 3");
        assert_eq!(lookup.get("outer_with_closure"), Some(&Some(3)),
                   "outer body has 1 closure (+1 base) and the closure body has 1 if (+1) — matches Mozilla `cyclomatic.sum`");
        assert_eq!(lookup.get("if_let_test"),        Some(&Some(2)),
                   "if let uses unified if_expression → +1");
        assert_eq!(lookup.get("let_else_test"),      Some(&Some(1)),
                   "Mozilla doesn't count let-else as a decision point — confirmed against rust-code-analysis source");
        assert_eq!(lookup.get("match_with_guard"),   Some(&Some(4)),
                   "1 base + 3 arms; Mozilla doesn't count guards");
        assert_eq!(lookup.get("match_with_or_pattern"), Some(&Some(4)),
                   "1 base + 3 arms (or-patterns are NOT counted — Mozilla matches McCabe original)");
    }

    #[test]
    fn cyclomatic_null_for_non_functions() {
        let conn = setup_db();
        index_sample(&conn, "struct Foo { x: i32 }\nenum Bar { A, B }\n");

        let non_fns: Vec<(String, Option<i64>)> = conn
            .prepare("SELECT name, cyclomatic FROM symbols WHERE file='test.rs' AND kind NOT IN ('function','method')")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(!non_fns.is_empty(), "expected struct/enum rows");
        for (name, cc) in &non_fns {
            assert!(cc.is_none(), "{} (kind != fn/method) should have NULL cyclomatic, got {:?}", name, cc);
        }
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    fn index_sample(conn: &Connection, source: &str) {
        let mut parser = TsParser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut extractor = Extractor::new("test.rs".to_string(), source);
        extractor.extract(tree.root_node());
        insert_symbols(conn, &extractor.symbols).unwrap();
    }
}

#[cfg(test)]
mod string_literals_view_tests {
    use super::*;

    const SL_SAMPLE: &str = r#"fn dispatch(name: &str) -> i32 {
    if name == "echo" {
        return 1;
    }
    if name != "true" {
        return 2;
    }
    match name {
        "false" | "noop" => 0,
        "fail" => -1,
        other => other.len() as i32,
    }
}

fn talk() {
    println!("hello {}", "world");
    format!("debug: {}", "info");
}

/// Doc comment mentioning "echo" should be filtered.
fn comment_filter() {}
"#;

    #[test]
    fn string_literals_view_dispatch_buckets() {
        let conn = setup_db();
        index_sample(&conn, SL_SAMPLE);

        let rows: Vec<(String, String, i64)> = conn
            .prepare("SELECT value, context, COUNT(*)
                      FROM string_literals
                      WHERE file='test.rs'
                      GROUP BY value, context
                      ORDER BY value, context")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let lookup: std::collections::HashMap<(String, String), i64> =
            rows.iter().map(|(v, c, n)| ((v.clone(), c.clone()), *n)).collect();

        assert_eq!(lookup.get(&("echo".to_string(),  "compare".to_string())),  Some(&1),
                   "expected one == \"echo\" hit");
        assert_eq!(lookup.get(&("true".to_string(),  "compare".to_string())),  Some(&1),
                   "expected one != \"true\" hit");
        assert_eq!(lookup.get(&("false".to_string(), "match_pat".to_string())), Some(&1),
                   "expected match arm \"false\" | \"noop\" — false counted");
        assert_eq!(lookup.get(&("noop".to_string(),  "match_pat".to_string())), Some(&1),
                   "expected match arm \"false\" | \"noop\" — noop also counted (multi-string line)");
        assert_eq!(lookup.get(&("fail".to_string(),  "match_pat".to_string())), Some(&1),
                   "expected match arm \"fail\"");
        assert_eq!(lookup.get(&("hello {}".to_string(), "fmt".to_string())), Some(&1),
                   "println! format string classified fmt");
        assert_eq!(lookup.get(&("debug: {}".to_string(), "fmt".to_string())), Some(&1),
                   "format! format string classified fmt");
    }

    #[test]
    fn string_literals_view_filters_doc_comments() {
        let conn = setup_db();
        index_sample(&conn, SL_SAMPLE);

        // The /// doc comment line mentions "echo" but should not appear.
        let comment_hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM string_literals
                 WHERE file='test.rs' AND value='echo' AND context='compare'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(comment_hits, 1,
                   "should be exactly 1 dispatch 'echo' (the real if-eq), not 2 with the doc comment");
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    fn index_sample(conn: &Connection, source: &str) {
        let mut parser = TsParser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut extractor = Extractor::new("test.rs".to_string(), source);
        extractor.extract(tree.root_node());
        insert_lines(conn, "test.rs", source, &extractor.symbol_ranges).unwrap();
    }
}

#[cfg(test)]
mod block_dupes_tests {
    use super::*;

    // Two identical 5-line setup blocks + a unique-5-line block
    const DUPE_SAMPLE: &str = r#"fn first() -> i32 {
    let ctx = make_context();
    let mut builder = ctx.builder();
    let result = builder.compile();
    if result.is_err() { return -1; }
    result.unwrap()
}

fn second() -> i32 {
    let ctx = make_context();
    let mut builder = ctx.builder();
    let result = builder.compile();
    if result.is_err() { return -1; }
    result.unwrap() + 1
}

fn third_unique() -> i32 {
    let raw = load_raw_bytes();
    let parsed = parse_or_default(raw);
    let normalized = canonicalize(parsed);
    let hashed = hash_canon(normalized);
    hashed.len() as i32
}
"#;

    #[test]
    fn block_dupes_finds_4line_duplicates() {
        let conn = setup_db();
        index_sample(&conn, DUPE_SAMPLE);

        // The 4-line `let ctx = ... let mut builder ... let result ... if result.is_err()...`
        // sequence appears in both `first` and `second`.
        let occ: i64 = conn.query_row(
            "SELECT MAX(occurrences) FROM block_dupes",
            [], |row| row.get(0)
        ).unwrap_or(0);
        assert!(occ >= 2,
                "expected at least one 4-line block duplicated 2+ times, got max={}", occ);

        // Spot-check: the specific shingle for the make_context setup should appear
        let count: i64 = conn.query_row(
            "SELECT occurrences FROM block_dupes
             WHERE shingle LIKE '%make_context%' AND shingle LIKE '%result.is_err%'",
            [], |row| row.get(0)
        ).unwrap_or(0);
        assert_eq!(count, 2, "expected the make_context+compile shingle to occur twice");
    }

    #[test]
    fn block_dupes_skips_unique_blocks() {
        let conn = setup_db();
        index_sample(&conn, DUPE_SAMPLE);

        // third_unique's body shouldn't appear in block_dupes at all.
        let unique_hits: i64 = conn.query_row(
            "SELECT COUNT(*) FROM block_dupes WHERE shingle LIKE '%canonicalize%'",
            [], |row| row.get(0)
        ).unwrap_or(0);
        assert_eq!(unique_hits, 0, "unique block should not appear in block_dupes");
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    fn index_sample(conn: &Connection, source: &str) {
        let mut parser = TsParser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut extractor = Extractor::new("test.rs".to_string(), source);
        extractor.extract(tree.root_node());
        insert_lines(conn, "test.rs", source, &extractor.symbol_ranges).unwrap();
    }
}
