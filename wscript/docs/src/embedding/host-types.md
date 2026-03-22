# Host Types

Host types let you expose Rust structs to scripts as first-class types with methods. Scripts can hold references to host objects, call methods on them, and pass them to other functions.

## Registering a Type

Use the `BindingRegistry` to register a type with methods:

```rust
use spite_script::bindings::{HostTypeBinding, HostFnBinding, ParamInfo, ScriptType};
use std::any::TypeId;

let mut binding = HostTypeBinding {
    name: "DbConnection".to_string(),
    rust_type_id: TypeId::of::<DbConnection>(),
    doc: Some("A connection to the database.".to_string()),
    methods: indexmap::IndexMap::new(),
    debug_display: None,
    debug_children: None,
};

// Add methods to the type binding, then register with engine.bindings_mut()
engine.bindings_mut().types.insert("DbConnection".to_string(), binding);
```

## Using in Scripts

Registered types appear as first-class types in scripts:

```spite
// Assuming DbConnection is registered with a query method:
let rows = db.query("SELECT * FROM users");
```

## Debug Display

For the DAP debugger, you can provide custom display and child inspection:

```rust
binding.debug_display = Some(Arc::new(|obj: &dyn Any| {
    let conn = obj.downcast_ref::<DbConnection>().unwrap();
    format!("DbConnection(host={}, db={})", conn.host, conn.db_name)
}));

binding.debug_children = Some(Arc::new(|obj: &dyn Any| {
    let conn = obj.downcast_ref::<DbConnection>().unwrap();
    vec![
        ("host".into(), DebugValue::String(conn.host.clone())),
        ("db".into(), DebugValue::String(conn.db_name.clone())),
    ]
}));
```

These appear in the VS Code Variables panel when debugging.
