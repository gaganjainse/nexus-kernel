# Issues Artifact

Generated: 2026-08-05T10:54:13+05:30
Total issues: 20

## Fixed Issues

### ISS-001: production-code-unwrap
- **File**: crates/nexusaos-kernel/src/runtime/kernel.rs:340-342
- **Severity**: high
- **Description**: manifest.transition_to(...).unwrap() in submit_task - unrecoverable state-machine failures but should propagate as NexusError
- **Status**: FIXED
- **Fix**: Replaced .unwrap() with ?

### ISS-002: production-code-unwrap
- **File**: crates/nexusaos-kernel/src/worker.rs:208
- **Severity**: medium
- **Description**: worker_idx.unwrap() after is_none() check - unreachable but uses unwrap instead of safe pattern
- **Status**: FIXED
- **Fix**: Replaced with let Some(worker_idx) = ... pattern

### ISS-003: option-unwrap-in-test
- **File**: crates/nexusaos-kernel/src/context.rs:415, 436
- **Severity**: medium
- **Description**: budget.clamp_reason? used on Option in Result-returning test function - E0277 compilation error
- **Status**: FIXED
- **Fix**: Replaced with budget.clamp_reason.ok_or('expected clamp reason')?

### ISS-004: option-unwrap-in-test
- **File**: crates/nexusaos-kernel/src/manifest.rs:353
- **Severity**: medium
- **Description**: store.get(&id).await? on async fn returning Option - E0277 compilation error
- **Status**: FIXED
- **Fix**: Replaced with store.get(&id).await.ok_or('manifest not found')?

### ISS-005: misplaced-ok-return
- **File**: crates/nexusaos-kernel/src/events.rs:359-364, 459-464, 472-479
- **Severity**: medium
- **Description**: Ok(()) placed inside for loop body instead of after loop - E0308 mismatched types
- **Status**: FIXED
- **Fix**: Moved Ok(()) outside loop/match blocks

### ISS-006: misplaced-ok-return
- **File**: crates/nexusaos-kernel/src/model/claude.rs:268-276
- **Severity**: medium
- **Description**: Ok(()) placed inside for loop body instead of after loop
- **Status**: FIXED
- **Fix**: Moved Ok(()) outside loop

### ISS-007: misplaced-ok-return
- **File**: crates/nexusaos-kernel/src/model/openai_compat.rs:536-550
- **Severity**: medium
- **Description**: Ok(()) placed inside for loop body instead of after loop
- **Status**: FIXED
- **Fix**: Moved Ok(()) outside loop

### ISS-008: misplaced-ok-return
- **File**: crates/nexusaos-kernel/src/model/types.rs:84-89
- **Severity**: medium
- **Description**: Ok(()) placed inside for loop body instead of after loop
- **Status**: FIXED
- **Fix**: Moved Ok(()) outside loop


## Pending Issues

### ISS-009: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/config.rs:multiple
- **Severity**: low
- **Description**: Test functions using 18 .expect(), 6 .unwrap(), 1 panic! but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-010: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/projection.rs:multiple
- **Severity**: low
- **Description**: Test functions using 8 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-011: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/snapshot.rs:multiple
- **Severity**: low
- **Description**: Test functions using 27 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-012: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/sqlite_event_store.rs:multiple
- **Severity**: low
- **Description**: Test functions using 28 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-013: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/filesystem.rs:multiple
- **Severity**: low
- **Description**: Test functions using 32 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-014: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/scheduler.rs:multiple
- **Severity**: low
- **Description**: Test functions using 34 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-015: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/replay.rs:multiple
- **Severity**: low
- **Description**: Test functions using 35 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-016: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/event_store.rs:multiple
- **Severity**: low
- **Description**: Test functions using 44 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-017: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/git.rs:multiple
- **Severity**: low
- **Description**: Test functions using 23 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-018: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/worker.rs:multiple
- **Severity**: low
- **Description**: Test functions using 1 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-019: test-unwrap-in-non-result-fn
- **File**: crates/nexusaos-kernel/src/resolver.rs:multiple
- **Severity**: low
- **Description**: Test functions using 1 .unwrap() but not returning Result - syntactically valid but inconsistent with error-handling standards
- **Status**: PENDING
- **Fix**: Convert test functions to return Result<(), Box<dyn std::error::Error>> and replace .unwrap()/.expect() with ?, panic!() with unreachable!()

### ISS-020: unused-parameter
- **File**: multiple:multiple
- **Severity**: low
- **Description**: Unused underscore-prefixed parameters in production code - already prefixed with _ so no warning, but could be cleaned up
- **Status**: PENDING
- **Fix**: Remove unused parameters or use them

