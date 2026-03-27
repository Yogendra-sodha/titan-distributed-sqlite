---
description: Auto-fix all compiler warnings in the workspace
---
// turbo-all

1. Run cargo fix on the entire workspace:
```
cargo fix --workspace --allow-dirty 2>&1
```

2. Verify it compiles cleanly:
```
cargo build --release 2>&1
```

3. Run tests to make sure nothing broke:
```
cargo test --workspace 2>&1
```

4. Report results to the user.
