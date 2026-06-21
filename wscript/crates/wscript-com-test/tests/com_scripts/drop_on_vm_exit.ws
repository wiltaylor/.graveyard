// Create an object and deliberately don't release it — the Vm's Drop
// path must Release the IDispatch and CoUninitialize without crashing.
@export
fn test() {
    let d = com_create("Scripting.Dictionary");
    if d == 0 {
        test_fail("create failed: " + com_last_error());
        return;
    }
    let _ = com_call_i2si(d, "Add", "k", 1);
    test_pass();
}
