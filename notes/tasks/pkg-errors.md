title: Check pkg.Errors in loadPackages
status: ready
depends: []
gates:
  - tests
  - commit

C3 partial — we unified the file enumerator but `loadPackages` still silently
ignores `pkg.Errors`. Go packages with type errors get partial results without
any indication. Should log or propagate errors for packages that fail to load.
