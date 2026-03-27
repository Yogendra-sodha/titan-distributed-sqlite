---
description: General development workflow - write code, build, test, fix autonomously
---
// turbo-all

When given a development task, follow this workflow:

1. Understand the task fully before writing code. Read relevant existing files first.

2. Make the code changes needed.

3. Build to check for compilation errors:
```
cargo build --release 2>&1
```

4. If there are errors, fix them and rebuild. Repeat until clean.

5. Run all tests:
```
cargo test --workspace 2>&1
```

6. If tests fail, fix them and rerun. Repeat until all pass.

7. Report the final status to the user:
   - What files were changed
   - Build status (pass/fail)
   - Test results (X passed, Y failed)
   - Any remaining issues
