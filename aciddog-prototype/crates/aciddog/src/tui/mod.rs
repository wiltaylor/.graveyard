pub(crate) mod app;
mod blocks;
pub(crate) mod boot;
mod channel_log;
mod chat_log;
mod clipboard;
mod commands;
mod compaction;
mod cores_view;
mod cron_view;
mod diff;
mod inbox_view;
mod markdown;
mod memory_view;
mod reload;
pub(crate) mod repo_diff_view;
mod tasks_view;
pub mod transcript;
mod ui;
pub(crate) mod workflow_view;

pub use app::run;
#[doc(hidden)]
pub use app::test_support;
pub use blocks::{ChatBlock, MailDirection, SystemKind, TaskOpKind};
pub use cores_view::agents_using_core;
pub use inbox_view::InboxMessage;
pub use transcript::build_restart_transcript;
