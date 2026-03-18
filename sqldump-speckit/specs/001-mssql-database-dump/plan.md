# Implementation Plan: MSSQL Database Dump Tool

**Branch**: `001-mssql-database-dump` | **Date**: 2025-10-14 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-mssql-database-dump/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Build a command-line tool in .NET 8 that exports MSSQL Server databases (SQL 2008 and later) to organized SQL scripts supporting schema-only or schema+data modes, with dependency-aware ordering and programmable object export. The tool will output to separate directories by object type (tables/, views/, procedures/, functions/, triggers/, data/) with one file per object, using DROP IF EXISTS pattern for idempotency, and batch data in 1,000-row chunks for memory efficiency.

## Technical Context

**Language/Version**: .NET 8 (C#)  
**Primary Dependencies**: Microsoft.Data.SqlClient (SQL Server connectivity), System.CommandLine (CLI framework), Spectre.Console (progress feedback)  
**Storage**: File system output (organized directory structure with SQL script files)  
**Testing**: xUnit with Docker-based MSSQL containers, Northwind database (enhanced with comprehensive object types)  
**Target Platform**: Cross-platform CLI (Linux, Windows, macOS via .NET 8 runtime)
**Project Type**: Single project (command-line tool)  
**Performance Goals**: Export 100-table database schema in <5 minutes; 1GB database (schema+data) in <10 minutes; handle 1M rows per table  
**Constraints**: SQL Server 2008+ compatibility (avoid 2012+ features); 30s connection timeout with 3 retries; 1,000 row batching for memory efficiency; <200MB memory for typical operations  
**Scale/Scope**: Support databases with 100-1000 tables, complex dependency graphs, circular FK relationships, comprehensive MSSQL object types (tables, views, procedures, functions, triggers)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### ✅ I. Specification-First Development (NON-NEGOTIABLE)
- Complete specification exists at `/specs/001-mssql-database-dump/spec.md`
- 4 prioritized user stories (P1-P4) with independent test criteria
- 20 functional requirements, all testable and measurable
- 9 success criteria with quantitative metrics
- Clarifications completed (5 questions resolved)

### ✅ II. User-Centric Design
- User stories expressed as DBA journeys, not technical implementations
- Each story independently valuable: P1 (schema export), P2 (data export), P3 (programmable objects), P4 (selective filtering)
- Priority-ranked for MVP-first delivery
- All stories independently testable

### ✅ III. Template-Driven Consistency
- Specification follows spec-template.md structure
- This plan follows plan-template.md structure
- All mandatory sections completed

### ✅ IV. Phased Implementation Planning
- Phase 0: Research .NET 8 MSSQL libraries, SQL 2008 compatibility, dependency algorithms
- Phase 1: Data model, CLI contracts, Docker test setup
- Phase 2: Task breakdown by user story (to be generated with `/speckit.tasks`)

### ✅ V. Independent Story Implementation
- P1 (Schema Export) delivers MVP value standalone
- P2 (Data Export) builds on P1 independently
- P3 (Programmable Objects) independent of P2
- P4 (Selective Filters) enhances without blocking P1-P3

### ✅ VI. AI-Assisted Development
- Natural language feature description processed
- Structured specification generated
- Clarifications resolved with recommendations
- Agent context will be updated in Phase 1

### ✅ VII. Quality Gates & Validation
- Specification gate: PASSED (requirements checklist complete)
- Constitution gate: PASSED (this check)
- Design gate: PENDING (Phase 1 completion)
- Implementation gate: PENDING (tasks.md generation)

**Gate Status**: ✅ PASS - All constitution principles satisfied, no violations to justify

## Project Structure

### Documentation (this feature)

```
specs/001-mssql-database-dump/
├── spec.md              # Feature specification
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
│   └── cli-interface.md # Command-line interface specification
├── checklists/          # Quality validation checklists
│   └── requirements.md  # Specification quality checklist (completed)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```
/
├── src/
│   ├── MsSqlDump/
│   │   ├── Program.cs                    # CLI entry point
│   │   ├── Commands/
│   │   │   ├── DumpCommand.cs           # Main dump command
│   │   │   └── CommandOptions.cs        # CLI option definitions
│   │   ├── Core/
│   │   │   ├── DatabaseConnection.cs    # Connection management with retry logic
│   │   │   ├── SchemaReader.cs          # Read database metadata
│   │   │   ├── DependencyResolver.cs    # Build dependency graph
│   │   │   └── SqlVersionDetector.cs    # Detect SQL Server version
│   │   ├── Exporters/
│   │   │   ├── TableExporter.cs         # Export table schemas
│   │   │   ├── DataExporter.cs          # Export table data with batching
│   │   │   ├── ViewExporter.cs          # Export views
│   │   │   ├── ProcedureExporter.cs     # Export stored procedures
│   │   │   ├── FunctionExporter.cs      # Export functions
│   │   │   └── TriggerExporter.cs       # Export triggers
│   │   ├── Writers/
│   │   │   ├── DirectoryOrganizer.cs    # Manage output directory structure
│   │   │   └── ScriptWriter.cs          # Write SQL scripts to files
│   │   ├── Models/
│   │   │   ├── DatabaseSchema.cs        # Complete schema representation
│   │   │   ├── TableMetadata.cs         # Table structure
│   │   │   ├── ColumnMetadata.cs        # Column details
│   │   │   ├── ConstraintMetadata.cs    # Constraints (PK, FK, etc.)
│   │   │   ├── IndexMetadata.cs         # Index definitions
│   │   │   └── ProgrammableObject.cs    # Procedures, functions, triggers, views
│   │   └── Utils/
│   │       ├── SqlQuoter.cs             # Handle special characters in names
│   │       └── ProgressReporter.cs      # Console progress feedback
│   └── MsSqlDump.csproj                 # Project file
├── tests/
│   ├── MsSqlDump.Tests/
│   │   ├── Unit/
│   │   │   ├── DependencyResolverTests.cs
│   │   │   ├── SqlQuoterTests.cs
│   │   │   └── SqlVersionDetectorTests.cs
│   │   ├── Integration/
│   │   │   ├── SchemaExportTests.cs     # Test P1: Schema-only export
│   │   │   ├── DataExportTests.cs       # Test P2: Schema + data export
│   │   │   ├── ProgrammableObjectTests.cs # Test P3: Procedures, functions, etc.
│   │   │   └── SelectiveExportTests.cs  # Test P4: Filtering options
│   │   └── MsSqlDump.Tests.csproj
│   └── TestDatabases/
│       ├── Northwind/
│       │   └── setup.sql                # Enhanced Northwind with all object types
│       └── docker-compose.yml           # MSSQL container configuration
├── docker/
│   ├── mssql-2008/                      # SQL Server 2008 test container (if available)
│   ├── mssql-2012/                      # SQL Server 2012 test container
│   ├── mssql-2019/                      # SQL Server 2019 test container
│   └── mssql-2022/                      # SQL Server 2022 test container
├── Justfile                             # Build automation (build, test, run, docker-up, docker-down)
├── .gitignore
├── README.md
└── global.json                          # Pin .NET 8 SDK version
```

**Structure Decision**: Selected single project structure (Option 1) as this is a standalone command-line tool. The source is organized into logical layers (Commands, Core, Exporters, Writers, Models, Utils) following .NET conventions. Tests are separated into Unit and Integration with dedicated test database setup using Docker containers for multiple SQL Server versions (2008-2022) to ensure compatibility across the supported range.

## Phase 0: Research & Technical Decisions

**Status**: ✅ COMPLETE

**Output**: [`research.md`](research.md)

**Key Decisions Made**:

1. **Database Connectivity**: Microsoft.Data.SqlClient v5.x (modern, cross-platform, SQL 2008-2022 support)
2. **CLI Framework**: System.CommandLine v2.0-beta4+ (official Microsoft library, type-safe, excellent help generation)
3. **Console UI**: Spectre.Console v0.48+ (rich progress bars, tables, tree views)
4. **SQL Version Compatibility**: Version-aware SQL generation with fallback patterns for SQL 2008-2022
5. **Dependency Algorithm**: Topological sort with Tarjan's algorithm for circular dependency detection
6. **Data Batching**: 1,000 rows per INSERT statement (balanced performance and memory)
7. **Docker Testing**: SQL Server 2017/2019/2022 Linux containers (2008 validated through syntax)
8. **Build Automation**: Justfile with docker integration and comprehensive targets
9. **Identifier Quoting**: Always use `[...]` with `]` escaping for consistency
10. **Testing Strategy**: Unit + Integration + End-to-End with xUnit and Docker

All technical unknowns resolved. No blockers for implementation.

---

## Phase 1: Design & Contracts

**Status**: ✅ COMPLETE

**Artifacts Generated**:

### 1. Data Model ([`data-model.md`](data-model.md))

**Core Entities Defined** (13 total):
- `DatabaseConnection`: Connection management with retry logic
- `DatabaseSchema`: Complete database metadata representation
- `TableMetadata`: Table structure with columns, constraints, indexes
- `ColumnMetadata`: Column definitions with types, identity, computed columns
- `ConstraintMetadata`: PK, FK, UNIQUE, CHECK, DEFAULT constraints
- `IndexMetadata`: Clustered and non-clustered indexes
- `ProgrammableObject`: Procedures, functions, views, triggers
- `DependencyGraph`: Topological ordering with cycle detection
- `DependencyNode` / `DependencyEdge`: Graph structure
- `ExportConfiguration`: CLI options and export settings
- `ScriptOutput`: Organized directory structure management

**Key Relationships**:
- DatabaseSchema contains Tables and Programmable Objects
- Tables have Columns, Constraints, and Indexes
- DependencyGraph resolves creation order
- ScriptOutput organizes file structure

**Validation Rules**: Comprehensive validation for each entity (names, types, relationships)

### 2. CLI Interface Contract ([`contracts/cli-interface.md`](contracts/cli-interface.md))

**Command Structure**:
```
mssqldump dump --server <server> --database <database> [options]
```

**Key Features**:
- Required: `--server`, `--database`
- Authentication: Windows Auth or SQL Auth (user/password)
- Output: Organized directories (tables/, data/, views/, procedures/, functions/, triggers/)
- Filtering: Object type toggles, include/exclude patterns (P4)
- Advanced: Batch size, timeout, retries

**Exit Codes**: 0-6, 99 (success, connection, auth, not found, permission, I/O, validation, unknown)

**Output Format**: 
- Directory per object type
- File naming: `{schema}.{object}.sql`
- Execution order: `_execution_order.txt`
- Progress: Spectre.Console progress bars

### 3. Quickstart Guide ([`quickstart.md`](quickstart.md))

**Development Workflow Documented**:
- Prerequisites: .NET 8, Docker, just
- Quick start: 5-minute setup
- Justfile targets: build, test, docker-up, docker-down, run, publish
- Testing scenarios: P1-P4 with examples
- Docker environment: SQL 2017/2019/2022 on ports 1433/1434/1435
- Northwind database: Enhanced with all object types
- IDE setup: VS Code and Visual Studio 2022
- Troubleshooting: Common issues and solutions

### 4. Agent Context Update

**GitHub Copilot Instructions**: `.github/copilot-instructions.md` updated with:
- Language: .NET 8 (C#)
- Frameworks: Microsoft.Data.SqlClient, System.CommandLine, Spectre.Console
- Database: File system output with SQL scripts
- Project type: Single command-line tool

---

## Post-Design Constitution Re-Check

### ✅ Design Gate: PASSED

All Phase 1 artifacts complete:
- ✅ data-model.md: 13 entities, complete relationships, validation rules
- ✅ contracts/cli-interface.md: Command structure, options, examples, error handling
- ✅ quickstart.md: Development workflow, testing scenarios, troubleshooting
- ✅ Agent context: GitHub Copilot instructions updated

No design ambiguities remain. All NEEDS CLARIFICATION items resolved.

**Ready for Phase 2: Task Breakdown (`/speckit.tasks`)**

---

## Implementation Strategy

### MVP Delivery Order (Per Constitution Principle V)

**Phase 2A: P1 - Schema-Only Export** (Deliverable MVP)
- Core value: Database schema versioning
- Independent testing: Export + verify table structure recreation
- Delivers: Tables, constraints, indexes exported correctly

**Phase 2B: P2 - Schema + Data Export** (Full Backup)
- Builds on P1: Adds data export with batching
- Independent testing: Export + restore + verify row counts match
- Delivers: Complete backup/restore capability

**Phase 2C: P3 - Programmable Objects** (Comprehensive Export)
- Builds on P2: Adds procedures, functions, triggers, views
- Independent testing: Export + execute procedures after restore
- Delivers: Version control for all database objects

**Phase 2D: P4 - Selective Export** (Advanced Filtering)
- Enhancement: Include/exclude patterns, object type filtering
- Independent testing: Verify filters work correctly
- Delivers: Optimized exports for large databases

### Parallel Work Opportunities

Tasks can run in parallel when they affect different files:
- Exporters (TableExporter, ViewExporter, ProcedureExporter) - different files
- Unit tests for different components
- Docker container setup vs. code development

Sequential dependencies:
- DependencyResolver must complete before Exporters (uses dependency graph)
- Models must complete before Exporters (depend on metadata classes)
- Core infrastructure (Connection, SchemaReader) before Exporters

---

## Summary

| Phase | Status | Output |
|-------|--------|--------|
| Phase 0: Research | ✅ Complete | research.md with 10 technical decisions |
| Phase 1: Design | ✅ Complete | data-model.md, contracts/, quickstart.md, agent context |
| Phase 2: Tasks | ⏳ Pending | Run `/speckit.tasks` to generate tasks.md |

**Next Command**: `/speckit.tasks`

This will generate a story-organized task breakdown enabling independent P1-P4 implementation.
