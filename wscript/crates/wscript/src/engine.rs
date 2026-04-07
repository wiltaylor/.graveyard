use crate::bindings::BindingRegistry;
use crate::runtime::value::Value;
use std::sync::Arc;

/// The main entry point for the Wscript engine.
pub struct Engine {
    bindings: BindingRegistry,
    config: EngineConfig,
    #[cfg(feature = "runtime")]
    script_engine: Option<crate::runtime::vm::ScriptEngine>,
}

#[derive(Debug, Clone, Default)]
pub struct EngineConfig {
    pub debug_mode: bool,
    pub max_fuel: Option<u64>,
}

impl Engine {
    pub fn new() -> Self {
        #[cfg(feature = "runtime")]
        let script_engine = crate::runtime::vm::ScriptEngine::new().ok();

        Self {
            bindings: BindingRegistry::new(),
            config: EngineConfig::default(),
            #[cfg(feature = "runtime")]
            script_engine,
        }
    }

    pub fn with_config(config: EngineConfig) -> Self {
        #[cfg(feature = "runtime")]
        let script_engine = crate::runtime::vm::ScriptEngine::new().ok();

        Self {
            bindings: BindingRegistry::new(),
            config,
            #[cfg(feature = "runtime")]
            script_engine,
        }
    }

    pub fn debug_mode(mut self, enabled: bool) -> Self {
        self.config.debug_mode = enabled;
        self
    }

    pub fn max_fuel(mut self, fuel: u64) -> Self {
        self.config.max_fuel = Some(fuel);
        self
    }

    /// Register a host function. The closure returns `Ok(None)` for unit,
    /// `Ok(Some(v))` for a value, or `Err(msg)` to trap the script.
    pub fn register_fn_raw(
        &mut self,
        name: &str,
        params: Vec<crate::bindings::ParamInfo>,
        return_type: crate::bindings::ScriptType,
        closure: impl Fn(&[Value]) -> Result<Option<Value>, String> + Send + Sync + 'static,
    ) -> &mut Self {
        use crate::bindings::HostFnBinding;
        let binding = HostFnBinding {
            name: name.to_string(),
            params,
            return_type,
            doc: None,
            param_docs: vec![],
            return_doc: None,
            examples: vec![],
            closure: Arc::new(closure),
        };
        self.bindings.functions.insert(name.to_string(), binding);
        self
    }

    /// Compile source text into a CompileResult.
    pub fn load(
        &self,
        source: &str,
    ) -> Result<crate::compiler::CompileResult, Vec<crate::compiler::Diagnostic>> {
        crate::compiler::compile(source, &self.bindings, self.config.debug_mode)
    }

    /// Compile source text and, if WASM bytes are produced, create a CompiledScript.
    #[cfg(feature = "runtime")]
    pub fn load_script(
        &self,
        source: &str,
    ) -> Result<LoadResult, Vec<crate::compiler::Diagnostic>> {
        let compile_result =
            crate::compiler::compile(source, &self.bindings, self.config.debug_mode)?;

        if compile_result.has_errors() {
            return Ok(LoadResult {
                diagnostics: compile_result.diagnostics,
                script: None,
            });
        }

        let script = if let (Some(wasm_bytes), Some(script_engine)) =
            (&compile_result.wasm_bytes, &self.script_engine)
        {
            let source_map = crate::compiler::source_map::SourceMap::new();
            let layouts = Arc::new(compile_result.type_layouts);
            // Clone the bindings registry into an Arc so the CompiledScript
            // can hold it for linker wiring at instantiate time.
            let bindings = Arc::new(clone_bindings(&self.bindings));
            match crate::runtime::vm::CompiledScript::from_wasm_bytes(
                script_engine,
                wasm_bytes,
                source.to_string(),
                source_map,
                layouts,
                bindings,
            ) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::error!("Failed to compile WASM module: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(LoadResult {
            diagnostics: compile_result.diagnostics,
            script,
        })
    }

    /// Convenience method: compile, instantiate, call, and dispose.
    #[cfg(feature = "runtime")]
    pub fn run(
        &self,
        source: &str,
        fn_name: &str,
        args: &[Value],
    ) -> Result<Option<Value>, Box<dyn std::error::Error>> {
        let load_result = self.load_script(source).map_err(|diags| {
            let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
            format!("Compilation failed:\n{}", msgs.join("\n"))
        })?;

        let script = load_result
            .script
            .ok_or("No WASM module produced (compilation may have had errors)")?;

        let script_engine = self
            .script_engine
            .as_ref()
            .ok_or("ScriptEngine not available")?;

        let mut vm = script
            .instantiate(script_engine)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        vm.call(fn_name, args)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }

    /// Get a reference to the ScriptEngine, if available.
    #[cfg(feature = "runtime")]
    pub fn script_engine(&self) -> Option<&crate::runtime::vm::ScriptEngine> {
        self.script_engine.as_ref()
    }

    /// Get a reference to the binding registry.
    pub fn bindings(&self) -> &BindingRegistry {
        &self.bindings
    }

    /// Get a mutable reference to the binding registry.
    pub fn bindings_mut(&mut self) -> &mut BindingRegistry {
        &mut self.bindings
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of loading a script (compilation + optional WASM module).
#[cfg(feature = "runtime")]
pub struct LoadResult {
    pub diagnostics: Vec<crate::compiler::Diagnostic>,
    pub script: Option<crate::runtime::vm::CompiledScript>,
}

#[cfg(feature = "runtime")]
impl LoadResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == crate::compiler::DiagnosticSeverity::Error)
    }
}

/// Clone a BindingRegistry (Arcs of closures + metadata) into a new owned one
/// so `CompiledScript` can hold an independent copy.
fn clone_bindings(src: &BindingRegistry) -> BindingRegistry {
    use crate::bindings::HostFnBinding;
    let mut out = BindingRegistry::new();
    for (name, hf) in &src.functions {
        out.functions.insert(
            name.clone(),
            HostFnBinding {
                name: hf.name.clone(),
                params: hf
                    .params
                    .iter()
                    .map(|p| crate::bindings::ParamInfo {
                        name: p.name.clone(),
                        ty: p.ty.clone(),
                    })
                    .collect(),
                return_type: hf.return_type.clone(),
                doc: hf.doc.clone(),
                param_docs: hf.param_docs.clone(),
                return_doc: hf.return_doc.clone(),
                examples: hf.examples.clone(),
                closure: hf.closure.clone(),
            },
        );
    }
    out
}
