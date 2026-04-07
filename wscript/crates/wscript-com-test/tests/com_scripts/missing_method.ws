// Late-bound feature detection: has() returns false for non-existent
// members, so scripts can probe before calling.
@export
fn test() {
    let d = com_create("Scripting.Dictionary");
    if d == 0 {
        test_fail("create failed: " + com_last_error());
        return;
    }
    if com_has(d, "DefinitelyNotARealMethod") {
        test_fail("has() returned true for a missing method");
        return;
    }
    com_release(d);
    test_pass();
}
