//! Tool JSON for the Responses API — top-level
//! `{type:"function", name, description, parameters, strict:true}`
//! shape, not the nested Chat Completions envelope. See
//! `ToolRegistry::responses_tools` (`ai/tools/mod.rs`) for the shared
//! implementation; this module exists as a placeholder in case we
//! need to layer Responses-API-specific tool types (`local_shell`,
//! `web_search`, `custom`, …) later.
