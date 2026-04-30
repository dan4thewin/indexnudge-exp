title: Recursive grep scopes to target directory
status: ready
depends: []
gates:
  - tests
  - commit

`grep -r pattern subdir/` queries all files in the DB, not just
files under `subdir/`. Should add `AND file LIKE 'subdir/%'` to
the SQL query when a target directory is specified. Currently
returns results from the entire indexed tree regardless of the
grep target.
