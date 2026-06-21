// Exercise create / method call / property get / release against
// Scripting.Dictionary (shipped by scrrun.dll, present in wine).
@export
fn test() {
    let d = com_create("Scripting.Dictionary");
    if d == 0 {
        test_fail("could not create Scripting.Dictionary: " + com_last_error());
        return;
    }
    let add_rc = com_call_i2si(d, "Add", "answer", 42);
    let add_err = com_last_error();
    if add_err.len() != 0 {
        test_fail("Add failed: " + add_err);
        return;
    }
    let exists = com_call_i1s(d, "Exists", "answer");
    if exists == 0 {
        let ex_err = com_last_error();
        test_fail("Exists(\"answer\") returned false after Add; err=[" + ex_err + "]");
        return;
    }
    let got = com_call_i1s(d, "Item", "answer");
    if got != 42 {
        let item_err = com_last_error();
        test_fail("expected Item(\"answer\") == 42; err=[" + item_err + "]");
        return;
    }
    let n = com_get_i(d, "Count");
    if n != 1 {
        test_fail("expected Count == 1");
        return;
    }
    com_release(d);
    test_pass();
}
