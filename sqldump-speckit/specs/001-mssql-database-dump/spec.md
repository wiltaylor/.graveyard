# Feature Specification: MSSQL Database Dump Tool

**Feature Branch**: `001-mssql-database-dump`  
**Created**: 2025-10-14  
**Status**: Draft  
**Input**: User description: "I want you to create a commandline tool for dumping the contents of a MSSQL database to a series of SQL scripts that can be used to recreate the database again. It needs to take into account the order of objects being created and it needs to have an option to just create the schema or optionally reinsert all the data that was in the tables. It should also support recation of all other object types in the database like stored procs, functions, etc."

## Clarifications

### Session 2025-10-14

- Q: Output File Organization Strategy → A: Separate directories by type with one file per object inside - organized but complex structure
- Q: Data Export Batching for Large Tables → A: Batch data in chunks of 1,000 rows per INSERT statement - balanced performance and safety
- Q: Connection Timeout and Retry Strategy → A: 30 second connection timeout with 3 retry attempts (5 second delay) - balanced and industry standard
- Q: Script Idempotency Pattern → A: DROP IF EXISTS then CREATE pattern - safest for re-runs, explicit replacement of objects
- Q: SQL Server Version Support → A: Support SQL Server 2008 and up

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Schema-Only Export (Priority: P1)

A database administrator needs to export the complete schema structure of a production MSSQL database to version control or to recreate the database structure in a different environment without any production data.

**Why this priority**: This is the core functionality - creating reproducible database schemas is the foundation for disaster recovery, environment setup, and version control. This story alone provides immediate value for schema versioning and environment provisioning.

**Independent Test**: Can be fully tested by connecting to a MSSQL database with tables, views, and constraints, running the tool in schema-only mode, and verifying that the generated SQL scripts successfully recreate an identical empty database structure when executed.

**Acceptance Scenarios**:

1. **Given** a MSSQL database with multiple tables, **When** the user runs the dump tool with schema-only mode, **Then** the tool generates SQL scripts that create all tables in the correct dependency order
2. **Given** a database with foreign key constraints, **When** the schema dump is executed, **Then** constraints are created after all referenced tables exist
3. **Given** a database with indexes and unique constraints, **When** the schema dump runs, **Then** all indexes and constraints are recreated exactly as in the source
4. **Given** the generated schema scripts, **When** executed on an empty database, **Then** a database with identical structure is created with zero data rows

---

### User Story 2 - Schema and Data Export (Priority: P2)

A database administrator needs to create a complete backup of a database including both schema and all data, allowing full database restoration from the generated SQL scripts.

**Why this priority**: This extends the MVP to support full backups and data migration scenarios. While schema-only export provides immediate value, data export is essential for complete backup/restore capabilities and database migrations.

**Independent Test**: Can be fully tested by connecting to a MSSQL database with populated tables, running the tool with data export enabled, and verifying that the generated scripts recreate both the schema and exact data when executed on an empty database.

**Acceptance Scenarios**:

1. **Given** a database with populated tables, **When** the user runs the dump tool with data export enabled, **Then** the tool generates INSERT statements for all table data
2. **Given** tables with foreign key relationships, **When** data is exported, **Then** INSERT statements are ordered to satisfy referential integrity constraints
3. **Given** a table with 10,000 rows, **When** data export runs, **Then** all rows are included in the generated scripts
4. **Given** the generated schema and data scripts, **When** executed sequentially on an empty database, **Then** a database with identical structure and data is created

---

### User Story 3 - Stored Procedures and Functions Export (Priority: P3)

A database administrator needs to export programmable objects (stored procedures, functions, triggers, views) to include in database version control or migration scripts.

**Why this priority**: Programmable objects are critical for complete database portability, but the tool already provides core value with schema and data export. This story makes the tool comprehensive for complex databases with business logic.

**Independent Test**: Can be fully tested by connecting to a MSSQL database containing stored procedures, user-defined functions, triggers, and views, running the tool, and verifying all programmable objects are scripted and can be recreated.

**Acceptance Scenarios**:

1. **Given** a database with stored procedures, **When** the dump tool runs, **Then** all stored procedures are scripted with their complete definitions
2. **Given** a database with user-defined functions (scalar and table-valued), **When** the dump runs, **Then** all functions are included in the output scripts
3. **Given** a database with triggers on tables, **When** the dump completes, **Then** triggers are scripted and associated with their correct tables
4. **Given** a database with views, **When** the dump executes, **Then** views are scripted in dependency order (views referencing other views are created after their dependencies)
5. **Given** the generated scripts for all object types, **When** executed on an empty database, **Then** all programmable objects are recreated and functional

---

### User Story 4 - Selective Export Options (Priority: P4)

A database administrator needs fine-grained control over what gets exported, such as excluding specific tables, limiting data export to certain tables, or exporting only specific object types.

**Why this priority**: This is an enhancement for advanced use cases. The core value is already delivered by P1-P3 stories. Selective export improves usability for large databases but isn't required for the MVP.

**Independent Test**: Can be fully tested by running the tool with various filter options (e.g., exclude tables matching pattern, export only specific schemas, data-only for specific tables) and verifying the output matches the specified filters.

**Acceptance Scenarios**:

1. **Given** a database with 50 tables, **When** the user specifies to exclude tables starting with "temp_", **Then** those tables are omitted from the export
2. **Given** a database with multiple schemas, **When** the user specifies a single schema to export, **Then** only objects in that schema are included
3. **Given** a large database, **When** the user specifies data export only for specific tables, **Then** schema is exported for all tables but data only for the specified ones
4. **Given** export filter options, **When** the user requests only stored procedures, **Then** the tool exports only stored procedures and ignores other object types

---

### Edge Cases

- What happens when a table contains binary data (VARBINARY, IMAGE types)?
- How does the tool handle tables with millions of rows (memory management for data export)?
- What happens when circular foreign key dependencies exist?
- How does the tool handle schemas with special characters or reserved keywords in object names?
- What happens when the database connection is lost during export?
- How are collation settings preserved in the export?
- What happens with objects that have permissions or ownership metadata?
- How are computed columns handled in the schema export?
- What happens when exporting encrypted stored procedures or functions?
- How does the tool handle SQL Server version-specific features (e.g., features available in 2016+ but not in 2008)?
- How are deprecated data types (e.g., TEXT, NTEXT, IMAGE in SQL 2008) handled in the export?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Tool MUST connect to a MSSQL database using provided connection parameters (server, database, authentication)
- **FR-002**: Tool MUST analyze database schema and determine correct dependency order for object creation
- **FR-003**: Tool MUST generate SQL scripts for table definitions including columns, data types, nullability, and default values
- **FR-004**: Tool MUST generate SQL scripts for primary keys, foreign keys, unique constraints, and check constraints
- **FR-005**: Tool MUST generate SQL scripts for indexes (clustered and non-clustered)
- **FR-006**: Tool MUST support schema-only mode that excludes all data export
- **FR-007**: Tool MUST support data export mode that generates INSERT statements for all table rows, batching data in chunks of 1,000 rows per INSERT statement to manage memory efficiently
- **FR-008**: Tool MUST order data INSERT statements to satisfy foreign key constraints
- **FR-009**: Tool MUST generate SQL scripts for stored procedures with complete procedure definitions
- **FR-010**: Tool MUST generate SQL scripts for user-defined functions (scalar, inline table-valued, multi-statement table-valued)
- **FR-011**: Tool MUST generate SQL scripts for triggers associated with tables
- **FR-012**: Tool MUST generate SQL scripts for views in correct dependency order
- **FR-013**: Tool MUST output scripts to separate directories organized by object type (tables/, views/, procedures/, functions/, triggers/, data/) with one file per object inside each directory
- **FR-014**: Tool MUST handle database object names with special characters by using appropriate SQL quoting
- **FR-015**: Tool MUST provide progress feedback during export operations
- **FR-016**: Tool MUST validate connection before starting export with a 30 second connection timeout and 3 retry attempts with 5 second delays, providing clear error messages on failure
- **FR-017**: Tool MUST generate idempotent scripts using DROP IF EXISTS then CREATE pattern to safely support re-running scripts
- **FR-018**: Tool MUST preserve collation settings for string columns
- **FR-019**: Tool MUST handle identity columns correctly (preserve seed and increment values)
- **FR-020**: Tool MUST support command-line flags to control export behavior (schema-only, include-data, object-types, output-directory)

### Key Entities

- **Database Connection**: Represents connection to source MSSQL database with server, database name, credentials, and connection state
- **Database Schema**: Represents the complete structure of the database including all object definitions and dependencies
- **Table Metadata**: Represents table structure including columns, data types, constraints, indexes, and relationships
- **Programmable Object**: Represents stored procedures, functions, triggers, and views with their definitions and dependencies
- **Dependency Graph**: Represents the order in which objects must be created to satisfy dependencies
- **Export Configuration**: Represents user preferences for what to export (schema-only, include-data, object type filters, output options)
- **SQL Script Output**: Represents the generated SQL files organized by type or sequence

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can successfully export a database schema from any MSSQL database and recreate an identical empty database structure within 5 minutes for databases with up to 100 tables
- **SC-002**: Schema exports correctly handle 100% of standard MSSQL object types (tables, constraints, indexes, stored procedures, functions, triggers, views)
- **SC-003**: Data exports successfully preserve all data with 100% accuracy for tables up to 1 million rows per table
- **SC-004**: Generated scripts execute without errors when run against an empty database and produce an identical copy
- **SC-005**: Users can complete a full database backup (schema + data) of a 1GB database in under 10 minutes
- **SC-006**: Dependency ordering produces zero foreign key constraint violations when scripts are executed in the generated order
- **SC-007**: Tool provides clear error messages and fails gracefully for 100% of connection, permission, or database errors
- **SC-008**: 95% of users can successfully use the tool to export their first database without consulting documentation beyond the help command
- **SC-009**: Tool successfully handles databases with circular dependencies or complex object interdependencies without manual intervention

## Assumptions

- **Database Access**: User has read permissions on all database objects to be exported (tables, procedures, functions, views, metadata tables)
- **MSSQL Version**: Tool targets MSSQL Server 2008 and later, ensuring compatibility with both legacy and modern SQL Server versions
- **Authentication**: Standard MSSQL authentication methods are supported (Windows Authentication and SQL Server Authentication)
- **Output Format**: SQL scripts use T-SQL syntax compatible with SQL Server 2008 and later versions
- **File System**: User has write permissions to the output directory for generated scripts
- **Connection Stability**: Database connection is reasonably stable during export (tool will handle transient errors with retry logic)
- **Character Encoding**: Database uses UTF-8 or UTF-16 encoding; tool will preserve encoding in output scripts
- **Performance**: For very large tables (>10 million rows), users may prefer specialized backup tools; this tool optimizes for readability and version control use cases
- **Schema Complexity**: Tool handles standard MSSQL features; advanced features like full-text indexes or partitioning will be included in schema export but may require testing for complex configurations
- **Development Environment**: Tool will be implemented using .NET 8, with Docker containers for testing using MSSQL images
- **Test Database**: Northwind database will be used as the primary test database, potentially enhanced with additional object types to ensure comprehensive test coverage
- **Build Automation**: Justfile will be used to manage the development lifecycle including building, testing, and running the tool against containerized MSSQL instances
- **End-to-End Testing**: Tests will validate both dump operations and restoration by creating new databases and verifying schema and data integrity after running generated scripts
