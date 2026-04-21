pub mod claude_code;
pub mod core;
pub mod event;
pub mod tag_call;

pub use claude_code::ClaudeCodeCore;
pub use core::{AICore, AIError};
pub use event::{parse_line, AIEvent};
