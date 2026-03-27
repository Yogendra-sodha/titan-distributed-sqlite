---
description: Run all tests in the Titan workspace
---
// turbo-all

1. Run all workspace tests and capture output:
```
cargo test --workspace 2>&1 | Out-File -FilePath test_output.txt -Encoding utf8
```

2. Read the test output file to check results:
```
Get-Content test_output.txt
```

3. Report the test results summary to the user — how many passed, failed, and any errors.

4. Clean up:
```
Remove-Item test_output.txt -ErrorAction SilentlyContinue
```
