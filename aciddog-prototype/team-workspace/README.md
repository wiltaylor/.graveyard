# aciddog team workspace

The workspace the prototype ran its own development team from (`/home/wil/agents/Applications/aciddog`):
`aciddog.wcl` defines the agent team (intake, planning, development, testing and integration roles, with their
prompts and pipeline), `justfile` wrapped the install and skill sync, and `scripts/idle_watchdog.py` nudged idle
agents. Runtime state (`.aciddog/` chat logs and memory, `aciddog.log`) was not kept.
