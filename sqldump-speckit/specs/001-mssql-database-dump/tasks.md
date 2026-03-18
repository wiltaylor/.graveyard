---
description: "Task list for MSSQL Database Dump Tool implementation"
---

# Tasks: MSSQL Database Dump Tool

**Input**: Design documents from `/specs/001-mssql-database-dump/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/cli-interface.md

**Tests**: Tests are NOT explicitly requested in the specification, so test tasks are NOT included. Focus on implementation only.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`
- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions
- **Single project**: `src/`, `tests/` at repository root

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure

- [X] T001 Create project directory structure: `src/MsSqlDump/`, `tests/MsSqlDump.Tests/`, `docker/`, `tests/TestDatabases/`
- [X] T002 Initialize .NET 8 project in `src/MsSqlDump/MsSqlDump.csproj` with NuGet packages: Microsoft.Data.SqlClient v5.1+, System.CommandLine v2.0-beta4+, Spectre.Console v0.48+
- [X] T003 [P] Create `global.json` to pin .NET 8 SDK version
- [X] T004 [P] Create `Justfile` with build, test, run, docker-up, docker-down, setup-testdb targets
- [X] T005 [P] Create `.gitignore` for .NET projects
- [X] T006 [P] Create `README.md` with project overview and quick start instructions
- [X] T007 Create Docker Compose configuration in `tests/TestDatabases/docker-compose.yml` for MSSQL 2017/2019/2022 containers
- [X] T008 Create enhanced Northwind test database setup script in `tests/TestDatabases/Northwind/setup.sql` with tables, views, procedures, functions, triggers

**Checkpoint**: Project structure ready, Docker configuration complete

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T009 Create `DatabaseConnection` class in `src/MsSqlDump/Core/DatabaseConnection.cs` with connection string builder, retry logic (30s timeout, 3 retries, 5s delay), and connection lifecycle management
- [X] T010 [P] Create `SqlVersionDetector` class in `src/MsSqlDump/Core/SqlVersionDetector.cs` to detect SQL Server version (2008-2022) and return major version number
- [X] T011 [P] Create `SqlQuoter` utility in `src/MsSqlDump/Utils/SqlQuoter.cs` to handle identifier quoting with `[...]` and `]` escaping
- [X] T012 [P] Create `ProgressReporter` utility in `src/MsSqlDump/Utils/ProgressReporter.cs` using Spectre.Console for progress bars and status updates
- [X] T013 [P] Create `DirectoryOrganizer` class in `src/MsSqlDump/Writers/DirectoryOrganizer.cs` to manage output directory structure (tables/, views/, procedures/, functions/, triggers/, data/)
- [X] T014 Create base model classes in `src/MsSqlDump/Models/`: `ColumnMetadata.cs`, `ConstraintMetadata.cs`, `IndexMetadata.cs`, `IndexColumnMetadata.cs`
- [X] T015 Create `TableMetadata` class in `src/MsSqlDump/Models/TableMetadata.cs` that uses column, constraint, and index metadata classes
- [X] T016 [P] Create `ProgrammableObject` class in `src/MsSqlDump/Models/ProgrammableObject.cs` for procedures, functions, triggers, views
- [X] T017 Create `DependencyGraph`, `DependencyNode`, `DependencyEdge` classes in `src/MsSqlDump/Core/DependencyResolver.cs` for topological sort and circular dependency detection using Tarjan's algorithm
- [X] T018 Create `DatabaseSchema` class in `src/MsSqlDump/Models/DatabaseSchema.cs` to hold complete database metadata (tables, views, procedures, functions, triggers, dependency graph)
- [X] T019 Create `ExportConfiguration` class in `src/MsSqlDump/Models/ExportConfiguration.cs` to hold CLI options and export settings
- [X] T020 [P] Create `ScriptWriter` class in `src/MsSqlDump/Writers/ScriptWriter.cs` for writing SQL scripts to files with UTF-8 encoding

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Schema-Only Export (Priority: P1) 🎯 MVP

**Goal**: Export complete database schema structure (tables with constraints and indexes) to SQL scripts in dependency order, enabling version control and environment provisioning without data.

**Independent Test**: Connect to Northwind database in Docker, run tool with `--schema-only`, verify generated scripts in tables/ directory can recreate identical empty database structure when executed.

### Implementation for User Story 1

- [X] T021 [US1] Create `SchemaReader` class in `src/MsSqlDump/Core/SchemaReader.cs` with methods to query sys.tables, sys.columns, sys.indexes, sys.foreign_keys, sys.check_constraints, sys.default_constraints from SQL Server metadata
- [X] T022 [US1] Implement `SchemaReader.ReadTableMetadata()` method to populate `TableMetadata` objects with columns, constraints, and indexes for all tables in the database
- [X] T023 [US1] Implement `DependencyResolver.BuildDependencyGraph()` method to analyze foreign key relationships and create dependency graph with topological ordering
- [X] T024 [US1] Implement `DependencyResolver.DetectCircularDependencies()` method using Tarjan's algorithm to identify strongly connected components (circular FK relationships)
- [X] T025 [US1] Create `TableExporter` class in `src/MsSqlDump/Exporters/TableExporter.cs` with method to generate CREATE TABLE scripts including columns, data types, nullability, defaults, identity columns, computed columns
- [X] T026 [US1] Implement `TableExporter.GenerateConstraintScripts()` method to generate ALTER TABLE ADD CONSTRAINT scripts for primary keys, foreign keys, unique constraints, check constraints in correct order
- [X] T027 [US1] Implement `TableExporter.GenerateIndexScripts()` method to generate CREATE INDEX scripts for non-clustered indexes (clustered index typically part of PK)
- [X] T028 [US1] Implement version-aware DROP script generation in `TableExporter` (DROP TABLE IF EXISTS for SQL 2016+, IF EXISTS pattern for SQL 2008-2015)
- [X] T029 [US1] Create `CommandOptions` class in `src/MsSqlDump/Commands/CommandOptions.cs` with properties for all CLI options from contracts/cli-interface.md
- [X] T030 [US1] Create `DumpCommand` class in `src/MsSqlDump/Commands/DumpCommand.cs` implementing System.CommandLine command with required options (--server, --database, --windows-auth or --user/--password)
- [X] T031 [US1] Implement `DumpCommand.ExecuteAsync()` method orchestrating: connect to database, detect version, read schema, build dependency graph, export tables to output directory in dependency order
- [X] T032 [US1] Create `Program.cs` in `src/MsSqlDump/Program.cs` with CLI entry point using System.CommandLine, registering dump command, handling --help and error messages
- [X] T033 [US1] Implement `DirectoryOrganizer.CreateOutputStructure()` to create tables/ directory and write _execution_order.txt file with ordered list of scripts
- [X] T034 [US1] Add progress reporting to schema export using `ProgressReporter` to show table export progress with Spectre.Console
- [X] T035 [US1] Add error handling and validation: connection failures, permission errors, invalid options, write failures with clear error messages

**Checkpoint**: At this point, User Story 1 should be fully functional - can export schema-only dumps of any MSSQL database with proper dependency ordering and idempotent DROP IF EXISTS patterns

---

## Phase 4: User Story 2 - Schema and Data Export (Priority: P2)

**Goal**: Extend schema export to include data export with INSERT statements batched in 1,000 row chunks, enabling complete database backup and restoration including all table data.

**Independent Test**: Connect to populated Northwind database, run tool without `--schema-only`, verify data/ directory contains INSERT statements, execute scripts to recreate database with identical data and row counts.

### Implementation for User Story 2

- [X] T036 [P] [US2] Add `--schema-only` flag support to `CommandOptions` and `DumpCommand` (defaults to false, meaning data export is enabled by default)
- [X] T037 [US2] Create `DataExporter` class in `src/MsSqlDump/Exporters/DataExporter.cs` with method to generate INSERT statements for table data
- [X] T038 [US2] Implement `DataExporter.ExportTableData()` method to query table rows using `SELECT *` and generate batched INSERT statements (1,000 rows per INSERT, configurable via --batch-size)
- [X] T039 [US2] Implement proper data escaping in `DataExporter` for string values (single quote escaping), binary data (hex format), NULL values, datetime formats, and special characters
- [X] T040 [US2] Implement data export ordering based on dependency graph to ensure INSERTs respect foreign key constraints (same order as table creation)
- [X] T041 [US2] Add special handling for identity columns in data export: SET IDENTITY_INSERT ON/OFF wrapper around INSERT statements for tables with identity columns
- [X] T042 [US2] Add special handling for tables in circular dependency groups: disable foreign key constraints with ALTER TABLE NOCHECK CONSTRAINT, insert data, re-enable with ALTER TABLE CHECK CONSTRAINT
- [X] T043 [US2] Update `DumpCommand.ExecuteAsync()` to call `DataExporter` after schema export when `--schema-only` is not specified
- [X] T044 [US2] Update `DirectoryOrganizer` to create data/ directory and organize data scripts by table name (same naming as schema scripts: {schema}.{table}.sql)
- [X] T045 [US2] Add progress reporting for data export showing table name, row count, and progress with Spectre.Console live updates
- [X] T046 [US2] Update _execution_order.txt to include data scripts in correct order after all schema scripts (tables → indexes → data → constraints)
- [X] T047 [US2] Add memory efficiency: stream rows and write to file incrementally instead of loading all data into memory (important for large tables)

**Checkpoint**: At this point, User Stories 1 AND 2 should both work - can export schema-only OR schema+data with proper batching and dependency ordering

---

## Phase 5: User Story 3 - Stored Procedures and Functions Export (Priority: P3)

**Goal**: Export programmable database objects (stored procedures, user-defined functions, triggers, views) to enable complete database portability with business logic.

**Independent Test**: Connect to database with stored procedures, functions, triggers, and views, run tool, verify programmable objects are scripted in separate directories and can be recreated successfully.

### Implementation for User Story 3

- [X] T048 [P] [US3] Extend `SchemaReader` with `ReadViews()` method to query sys.views and INFORMATION_SCHEMA.VIEWS for view definitions and dependencies
- [X] T049 [P] [US3] Extend `SchemaReader` with `ReadStoredProcedures()` method to query sys.procedures and sys.sql_modules for procedure definitions
- [X] T050 [P] [US3] Extend `SchemaReader` with `ReadFunctions()` method to query sys.objects (type FN, IF, TF) and sys.sql_modules for function definitions (scalar, inline, multi-statement)
- [X] T051 [P] [US3] Extend `SchemaReader` with `ReadTriggers()` method to query sys.triggers and sys.sql_modules for trigger definitions and table associations
- [X] T052 [US3] Update `DependencyResolver` to analyze view dependencies (views can reference other views, tables) and add to dependency graph
- [X] T053 [P] [US3] Create `ViewExporter` class in `src/MsSqlDump/Exporters/ViewExporter.cs` to generate CREATE VIEW scripts with version-aware DROP patterns
- [X] T054 [P] [US3] Create `ProcedureExporter` class in `src/MsSqlDump/Exporters/ProcedureExporter.cs` to generate CREATE PROCEDURE scripts with version-aware DROP patterns (DROP IF EXISTS for SQL 2016+, IF EXISTS for SQL 2008-2015)
- [X] T055 [P] [US3] Create `FunctionExporter` class in `src/MsSqlDump/Exporters/FunctionExporter.cs` to generate CREATE FUNCTION scripts for scalar, inline table-valued, and multi-statement table-valued functions
- [X] T056 [P] [US3] Create `TriggerExporter` class in `src/MsSqlDump/Exporters/TriggerExporter.cs` to generate CREATE TRIGGER scripts with proper table association
- [X] T057 [US3] Update `DatabaseSchema` to include views, procedures, functions, triggers in the schema representation
- [X] T058 [US3] Update `DumpCommand.ExecuteAsync()` to export views (after tables), procedures, functions, and triggers to their respective directories
- [X] T059 [US3] Update `DirectoryOrganizer` to create views/, procedures/, functions/, triggers/ directories in output structure
- [X] T060 [US3] Update _execution_order.txt to include programmable objects in correct order: tables → indexes → data → views (dependency order) → procedures → functions → triggers
- [X] T061 [US3] Add progress reporting for programmable object export showing object type and count

**Checkpoint**: All core user stories (1-3) should now be independently functional - tool can export complete databases including schema, data, and all programmable objects

---

## Phase 6: User Story 4 - Selective Export Options (Priority: P4)

**Goal**: Provide fine-grained control over export scope with filters for tables, schemas, and object types, improving usability for large databases and focused exports.

**Independent Test**: Run tool with various filter options (--exclude, --include patterns, object type flags) and verify output contains only matching objects.

### Implementation for User Story 4

- [X] T062 [P] [US4] Add object type filter flags to `CommandOptions`: --tables, --views, --procedures, --functions, --triggers (all default to true)
- [X] T063 [P] [US4] Add pattern filter options to `CommandOptions`: --include (string array), --exclude (string array) for regex-based filtering
- [X] T064 [US4] Create `ObjectFilter` utility in `src/MsSqlDump/Utils/ObjectFilter.cs` to evaluate include/exclude patterns against object names using regex matching
- [X] T065 [US4] Update `SchemaReader` methods to accept filter configuration and skip objects that don't match filters (apply filters during metadata reading)
- [X] T066 [US4] Update `DumpCommand` to apply object type filters: skip entire exporter calls for disabled object types (e.g., if --tables=false, skip TableExporter)
- [X] T067 [US4] Update pattern matching to work on fully qualified names: `[schema].[object]` format for accurate filtering
- [X] T068 [US4] Add CLI validation: ensure at least one object type is enabled, validate regex patterns are valid before starting export
- [X] T069 [US4] Update progress reporting to show filtered counts: "Exporting 15/50 tables (35 excluded by filters)"
- [X] T070 [US4] Update _execution_order.txt generation to only include files for objects that passed filters

**Checkpoint**: All user stories (1-4) complete - tool supports full-featured selective exports with comprehensive filtering

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories and overall quality

- [X] T071 [P] Add comprehensive error messages for common failure scenarios: connection timeout, authentication failure, permission denied, invalid object names, disk full, invalid output path
- [X] T072 [P] Add `--version` flag to CLI showing tool version in `Program.cs`
- [X] T073 [P] Add `--verbose` flag to CLI for detailed logging output (SQL queries executed, objects processed, timing information)
- [X] T074 [P] Create comprehensive README.md with installation instructions, usage examples for all 4 user stories, troubleshooting guide
- [X] T075 [P] Add code comments and XML documentation to all public classes and methods for API documentation
- [X] T076 Validate all quickstart.md scenarios: run each example command and verify expected output matches
- [X] T077 [P] Add collation preservation: ensure collation settings are included in column definitions for string types
- [X] T078 [P] Performance optimization: use async/await throughout for database queries to improve responsiveness
- [X] T079 [P] Add connection string validation and sanitization in `DatabaseConnection` to prevent SQL injection in connection parameters
- [X] T080 Create usage documentation in docs/ directory with detailed CLI reference, examples, FAQ, and troubleshooting

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phases 3-6)**: All depend on Foundational phase completion
  - User stories CAN proceed in parallel if staffed appropriately
  - Recommended sequential order for single developer: US1 → US2 → US3 → US4 (by priority)
- **Polish (Phase 7)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1 - Schema Export)**: Can start after Foundational (Phase 2) - No dependencies on other stories - **THIS IS THE MVP**
- **User Story 2 (P2 - Data Export)**: Can start after Foundational (Phase 2) - Builds on US1 (uses TableExporter) but independently testable
- **User Story 3 (P3 - Programmable Objects)**: Can start after Foundational (Phase 2) - Extends US1 (uses DependencyResolver) but independent of US2
- **User Story 4 (P4 - Selective Filters)**: Can start after Foundational (Phase 2) - Enhances all previous stories but independently testable

### Within Each User Story

- Models before services
- Services before exporters
- Exporters before command orchestration
- Core implementation before progress reporting and error handling
- Story complete and tested before moving to next priority

### Parallel Opportunities

**Phase 1 (Setup)**: T003, T004, T005, T006 can run in parallel

**Phase 2 (Foundational)**: T010, T011, T012, T013, T016, T020 can run in parallel (all separate files)

**Phase 3 (US1)**: No parallel opportunities - tasks have sequential dependencies through SchemaReader → DependencyResolver → Exporters → Command

**Phase 4 (US2)**: T036 and T037 can run in parallel (separate files)

**Phase 5 (US3)**: T048, T049, T050, T051 can run in parallel (all extend SchemaReader in separate methods); T053, T054, T055, T056 can run in parallel (separate exporter files)

**Phase 6 (US4)**: T062 and T063 can run in parallel (both modify CommandOptions)

**Phase 7 (Polish)**: T071, T072, T073, T074, T075, T077, T078, T079 can all run in parallel (separate concerns)

**Cross-Phase Parallel**: Once Foundational (Phase 2) completes, multiple developers can work on different user stories simultaneously:
- Developer A: User Story 1 (T021-T035)
- Developer B: User Story 2 (T036-T047)  
- Developer C: User Story 3 (T048-T061)
- Developer D: User Story 4 (T062-T070)

---

## Parallel Example: Phase 2 (Foundational)

```bash
# Launch all independent foundational components together:
Task T010: "Create SqlVersionDetector class in src/MsSqlDump/Core/SqlVersionDetector.cs"
Task T011: "Create SqlQuoter utility in src/MsSqlDump/Utils/SqlQuoter.cs"
Task T012: "Create ProgressReporter utility in src/MsSqlDump/Utils/ProgressReporter.cs"
Task T013: "Create DirectoryOrganizer class in src/MsSqlDump/Writers/DirectoryOrganizer.cs"
Task T016: "Create ProgrammableObject class in src/MsSqlDump/Models/ProgrammableObject.cs"
Task T020: "Create ScriptWriter class in src/MsSqlDump/Writers/ScriptWriter.cs"

# Then proceed with dependent tasks:
Task T009: "Create DatabaseConnection" (may use SqlQuoter)
Task T014-T015: "Create base models" (independent)
Task T017-T019: "Create dependency graph and schema models" (depend on base models)
```

---

## Parallel Example: Phase 5 (User Story 3)

```bash
# Launch all schema readers together:
Task T048: "Extend SchemaReader with ReadViews() method"
Task T049: "Extend SchemaReader with ReadStoredProcedures() method"
Task T050: "Extend SchemaReader with ReadFunctions() method"
Task T051: "Extend SchemaReader with ReadTriggers() method"

# Then launch all exporters together:
Task T053: "Create ViewExporter class"
Task T054: "Create ProcedureExporter class"
Task T055: "Create FunctionExporter class"
Task T056: "Create TriggerExporter class"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T008)
2. Complete Phase 2: Foundational (T009-T020) - **CRITICAL GATE**
3. Complete Phase 3: User Story 1 (T021-T035)
4. **STOP and VALIDATE**: 
   - Run `just docker-up && just setup-testdb`
   - Export Northwind schema: `just run dump -s localhost -d Northwind -w --schema-only -o ./test-output`
   - Verify tables/ directory structure
   - Execute scripts against empty database
   - Confirm identical schema created
5. **MVP COMPLETE** - Tool provides immediate value for schema versioning and environment setup

### Incremental Delivery (Recommended)

1. **Foundation** (T001-T020): Setup + Foundational → Test Docker setup works
2. **MVP** (T021-T035): Add User Story 1 → Test schema export independently → **SHIP IT** 🚀
3. **Backup/Restore** (T036-T047): Add User Story 2 → Test data export independently → **SHIP IT** 🚀
4. **Complete Portability** (T048-T061): Add User Story 3 → Test programmable objects independently → **SHIP IT** 🚀
5. **Advanced Features** (T062-T070): Add User Story 4 → Test selective filters → **SHIP IT** 🚀
6. **Production Ready** (T071-T080): Polish phase → Final release 🎯

Each increment adds value without breaking previous functionality.

### Parallel Team Strategy

With 4 developers after Foundational phase (T020) completes:

- **Dev A (Week 1)**: User Story 1 (T021-T035) - Schema export
- **Dev B (Week 1)**: User Story 2 (T036-T047) - Data export (will integrate with Dev A's TableExporter)
- **Dev C (Week 1)**: User Story 3 (T048-T061) - Programmable objects (will integrate with Dev A's DependencyResolver)
- **Dev D (Week 1)**: User Story 4 (T062-T070) - Filters (will enhance all other exporters)
- **All Devs (Week 2)**: Integration testing, bug fixes, Polish phase (T071-T080)

---

## Summary Statistics

- **Total Tasks**: 80 tasks
- **Setup Phase**: 8 tasks (T001-T008)
- **Foundational Phase**: 12 tasks (T009-T020) - BLOCKS all user stories
- **User Story 1 (P1 - MVP)**: 15 tasks (T021-T035) - Schema export
- **User Story 2 (P2)**: 12 tasks (T036-T047) - Data export
- **User Story 3 (P3)**: 14 tasks (T048-T061) - Programmable objects
- **User Story 4 (P4)**: 9 tasks (T062-T070) - Selective filters
- **Polish Phase**: 10 tasks (T071-T080) - Cross-cutting improvements

### Parallel Opportunities Identified

- **Phase 1**: 4 tasks can run in parallel
- **Phase 2**: 6 tasks can run in parallel
- **Phase 3**: Sequential (dependency chain)
- **Phase 4**: 2 tasks can run in parallel
- **Phase 5**: 4 + 4 tasks can run in parallel (schema readers, then exporters)
- **Phase 6**: 2 tasks can run in parallel
- **Phase 7**: 8 tasks can run in parallel

### MVP Scope

**Minimum Viable Product = User Story 1 only**: 35 tasks total (Setup + Foundational + US1)

This delivers a working schema dump tool that provides immediate value for:
- Database schema version control
- Environment setup automation
- Schema documentation
- Disaster recovery (schema portion)

**Recommended First Release = User Stories 1 + 2**: 47 tasks total (adds data export)

This delivers complete backup/restore capability.

---

## Notes

- **[P] tasks** = different files, no dependencies within the phase
- **[Story] label** maps task to specific user story for traceability and independent delivery
- Each user story is independently completable and testable
- No tests included as specification doesn't explicitly request testing
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Tasks use exact file paths from plan.md structure
- Avoid vague tasks, same file conflicts, cross-story dependencies that break independence
