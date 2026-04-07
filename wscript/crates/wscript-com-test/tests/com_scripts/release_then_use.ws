// After release, a call on the same handle must fail cleanly (no crash,
// non-empty com_last_error).
@export
fn test() {
    let d = com_create("Scripting.Dictionary");
    if d == 0 {
        test_fail("create failed: " + com_last_error());
        return;
    }
    com_release(d);
    let _ = com_call_i2si(d, "Add", "k", 1);
    let err = com_last_error();
    // Use string equality against a literal: lower.rs detects string eq
    // when either operand is a literal, whereas `.len()` on a call-result
    // local currently mis-dispatches to array len.
    if err == "" {
        test_fail("expected non-empty com_last_error after use-after-release");
        return;
    }
    if com_has(d, "Add") {
        test_fail("com_has should return false on a released handle");
        return;
    }
    test_pass();
}
