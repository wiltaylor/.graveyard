# Data Model: MSSQL Database Dump Tool

**Date**: 2025-10-14  
**Feature**: MSSQL Database Dump Tool  
**Purpose**: Define core entities, relationships, and data structures

## Overview

This tool models the SQL Server database metadata as a domain model to enable dependency analysis, ordered script generation, and organized file output. The model captures the complete database schema including tables, constraints, indexes, and programmable objects.

## Core Entities

### 1. DatabaseConnection

Represents a connection to the source MSSQL database.

**Properties**:
- `Server`: string - SQL Server hostname or IP
- `Database`: string - Database name to export
- `UserId`: string (optional) - SQL authentication username
- `Password`: string (optional) - SQL authentication password
- `UseWindowsAuth`: bool - Whether to use Windows Authentication
- `ConnectionTimeout`: int - Timeout in seconds (default: 30)
- `MaxRetries`: int - Maximum retry attempts (default: 3)
- `RetryDelay`: int - Delay between retries in seconds (default: 5)

**Relationships**:
- Creates → `DatabaseSchema` (1:1)

**Validation Rules**:
- Server must not be empty
- Database must not be empty
- If not UseWindowsAuth, UserId and Password required
- ConnectionTimeout must be > 0
- MaxRetries must be >= 0
- RetryDelay must be >= 0

**State Transitions**:
- Disconnected → Connecting → Connected
- Connected → Disconnected (on error or explicit close)
- Connecting → Failed (after MaxRetries exceeded)

---

### 2. DatabaseSchema

Represents the complete structure of the source database.

**Properties**:
- `DatabaseName`: string - Name of the database
- `ServerVersion`: string - SQL Server version (e.g., "10.50.6000" for SQL 2008 R2)
- `ServerMajorVersion`: int - Major version number (e.g., 10 for SQL 2008)
- `Collation`: string - Default database collation
- `Tables`: List<TableMetadata> - All tables in the database
- `Views`: List<ProgrammableObject> - All views
- `Procedures`: List<ProgrammableObject> - All stored procedures
- `Functions`: List<ProgrammableObject> - All functions (scalar, table-valued)
- `Triggers`: List<ProgrammableObject> - All triggers
- `DependencyGraph`: DependencyGraph - Resolved object dependencies

**Relationships**:
- Contains → `TableMetadata` (1:many)
- Contains → `ProgrammableObject` (1:many)
- Has → `DependencyGraph` (1:1)

**Methods**:
- `BuildDependencyGraph()`: Analyzes foreign keys and object dependencies
- `GetCreationOrder()`: Returns ordered list of objects for script generation

---

### 3. TableMetadata

Represents a single database table with its complete structure.

**Properties**:
- `SchemaName`: string - Schema name (e.g., "dbo")
- `TableName`: string - Table name
- `FullName`: string - Computed: `[SchemaName].[TableName]`
- `Columns`: List<ColumnMetadata> - All columns in order
- `PrimaryKey`: ConstraintMetadata (nullable) - Primary key constraint
- `ForeignKeys`: List<ConstraintMetadata> - All foreign key constraints
- `UniqueConstraints`: List<ConstraintMetadata> - UNIQUE constraints
- `CheckConstraints`: List<ConstraintMetadata> - CHECK constraints
- `DefaultConstraints`: List<ConstraintMetadata> - DEFAULT constraints
- `Indexes`: List<IndexMetadata> - All indexes (clustered and non-clustered)
- `HasIdentityColumn`: bool - Whether table has IDENTITY column
- `RowCount`: long - Approximate row count (from sys.dm_db_partition_stats)

**Relationships**:
- Has → `ColumnMetadata` (1:many, ordered)
- Has → `ConstraintMetadata` (1:many)
- Has → `IndexMetadata` (1:many)
- References → `TableMetadata` (via foreign keys)

**Methods**:
- `GetDependencies()`: Returns list of tables this table depends on (via FKs)
- `GenerateCreateScript(int sqlVersion)`: Generate CREATE TABLE statement
- `GenerateDataScript(int sqlVersion, int batchSize)`: Generate INSERT statements

---

### 4. ColumnMetadata

Represents a single column in a table.

**Properties**:
- `ColumnName`: string - Column name
- `OrdinalPosition`: int - Position in table (1-based)
- `DataType`: string - SQL data type (e.g., "int", "nvarchar", "decimal")
- `MaxLength`: int - Maximum length for string/binary types (-1 for MAX)
- `Precision`: int - Numeric precision
- `Scale`: int - Numeric scale
- `IsNullable`: bool - Whether column allows NULL
- `DefaultValue`: string (nullable) - Default constraint expression
- `IsIdentity`: bool - Whether column is IDENTITY
- `IdentitySeed`: long (nullable) - IDENTITY seed value
- `IdentityIncrement`: long (nullable) - IDENTITY increment value
- `IsComputed`: bool - Whether column is computed
- `ComputedExpression`: string (nullable) - Computed column expression
- `Collation`: string (nullable) - Column collation (for string types)

**Validation Rules**:
- ColumnName must not be empty
- OrdinalPosition must be > 0
- DataType must not be empty
- If IsIdentity, IdentitySeed and IdentityIncrement must be set
- If IsComputed, ComputedExpression must not be empty

**Methods**:
- `GetFullDefinition()`: Returns complete column definition for CREATE TABLE
- `GetDataTypeDefinition()`: Returns data type with length/precision (e.g., "nvarchar(50)")

---

### 5. ConstraintMetadata

Represents constraints (PK, FK, UNIQUE, CHECK, DEFAULT).

**Properties**:
- `ConstraintName`: string - Name of the constraint
- `ConstraintType`: ConstraintType - Enum: PrimaryKey, ForeignKey, Unique, Check, Default
- `TableName`: string - Table that owns this constraint
- `Columns`: List<string> - Column names involved in constraint
- `ReferencedTable`: string (nullable) - Referenced table for FK (SchemaName.TableName)
- `ReferencedColumns`: List<string> (nullable) - Referenced columns for FK
- `OnDeleteAction`: string (nullable) - FK delete action (CASCADE, SET NULL, etc.)
- `OnUpdateAction`: string (nullable) - FK update action
- `CheckExpression`: string (nullable) - CHECK constraint expression
- `DefaultExpression`: string (nullable) - DEFAULT constraint expression

**Relationships**:
- Belongs to → `TableMetadata` (many:1)
- References → `TableMetadata` (for foreign keys)

**Validation Rules**:
- ConstraintName must not be empty
- Columns list must not be empty
- If ConstraintType is ForeignKey, ReferencedTable and ReferencedColumns required
- If ConstraintType is Check, CheckExpression required
- If ConstraintType is Default, DefaultExpression required

**Methods**:
- `GenerateCreateScript()`: Generate ALTER TABLE ADD CONSTRAINT statement
- `GenerateDropScript(int sqlVersion)`: Generate version-appropriate DROP statement

---

### 6. IndexMetadata

Represents an index (clustered or non-clustered).

**Properties**:
- `IndexName`: string - Name of the index
- `TableName`: string - Table that owns this index
- `IsClustered`: bool - Whether index is clustered
- `IsUnique`: bool - Whether index is unique
- `IsPrimaryKey`: bool - Whether index supports primary key
- `Columns`: List<IndexColumnMetadata> - Columns in the index with sort order
- `IncludedColumns`: List<string> - Non-key included columns (covering index)
- `FilterExpression`: string (nullable) - Filtered index predicate (SQL 2008+)

**Relationships**:
- Belongs to → `TableMetadata` (many:1)
- Has → `IndexColumnMetadata` (1:many, ordered)

**Methods**:
- `GenerateCreateScript()`: Generate CREATE INDEX statement
- `GenerateDropScript(int sqlVersion)`: Generate DROP INDEX statement

---

### 7. IndexColumnMetadata

Represents a column within an index.

**Properties**:
- `ColumnName`: string - Name of the column
- `OrdinalPosition`: int - Position in index (1-based)
- `IsDescending`: bool - Whether sorted descending (default: ascending)

---

### 8. ProgrammableObject

Represents stored procedures, functions, views, and triggers.

**Properties**:
- `SchemaName`: string - Schema name
- `ObjectName`: string - Object name
- `ObjectType`: ProgrammableObjectType - Enum: StoredProcedure, ScalarFunction, TableFunction, View, Trigger
- `Definition`: string - Full T-SQL definition (CREATE statement body)
- `DependsOn`: List<string> - Objects this depends on (for ordering)
- `TriggerTable`: string (nullable) - For triggers: table name the trigger is on

**Relationships**:
- Depends on → `ProgrammableObject` (many:many, for views/functions)
- Attached to → `TableMetadata` (for triggers)

**Methods**:
- `GenerateCreateScript(int sqlVersion)`: Generate version-appropriate CREATE statement
- `GenerateDropScript(int sqlVersion)`: Generate DROP statement

---

### 9. DependencyGraph

Represents the dependency relationships between database objects.

**Properties**:
- `Nodes`: List<DependencyNode> - All objects in the graph
- `Edges`: List<DependencyEdge> - Dependency relationships
- `StronglyConnectedComponents`: List<List<DependencyNode>> - Cycles detected by Tarjan

**Relationships**:
- Contains → `DependencyNode` (1:many)
- Contains → `DependencyEdge` (1:many)

**Methods**:
- `AddTable(TableMetadata)`: Add table to graph
- `AddDependency(from, to)`: Add edge (from depends on to)
- `DetectCycles()`: Run Tarjan's algorithm, populate StronglyConnectedComponents
- `GetTopologicalOrder()`: Return creation order (handles cycles)

---

### 10. DependencyNode

Represents a node in the dependency graph.

**Properties**:
- `ObjectType`: string - "Table", "View", "Procedure", "Function"
- `FullName`: string - Schema.ObjectName
- `Metadata`: object - Reference to TableMetadata or ProgrammableObject
- `Index`: int - Tarjan's algorithm: DFS index
- `LowLink`: int - Tarjan's algorithm: lowest index reachable
- `OnStack`: bool - Tarjan's algorithm: whether on recursion stack

---

### 11. DependencyEdge

Represents a dependency relationship in the graph.

**Properties**:
- `From`: DependencyNode - Dependent object
- `To`: DependencyNode - Dependency (From depends on To)
- `EdgeType`: string - "ForeignKey", "ViewReference", "ProcedureReference"

---

### 12. ExportConfiguration

Represents user-provided export options from CLI.

**Properties**:
- `Server`: string
- `Database`: string
- `UserId`: string (nullable)
- `Password`: string (nullable)
- `UseWindowsAuth`: bool
- `OutputDirectory`: string - Where to write script files
- `SchemaOnly`: bool - Export schema without data
- `IncludeData`: bool - Export data (inverse of SchemaOnly)
- `IncludeTables`: bool
- `IncludeViews`: bool
- `IncludeProcedures`: bool
- `IncludeFunctions`: bool
- `IncludeTriggers`: bool
- `ExcludePatterns`: List<string> - Regex patterns for excluding objects
- `IncludePatterns`: List<string> - Regex patterns for including objects
- `BatchSize`: int - Rows per INSERT (default: 1000)

**Validation Rules**:
- OutputDirectory must be writable
- At least one object type must be included
- BatchSize must be > 0 and <= 10000

---

### 13. ScriptOutput

Represents the organized output structure.

**Properties**:
- `RootDirectory`: string - Base output directory
- `TableScripts`: Dictionary<string, string> - Table name → file path
- `DataScripts`: Dictionary<string, string> - Table name → data file path
- `ViewScripts`: Dictionary<string, string>
- `ProcedureScripts`: Dictionary<string, string>
- `FunctionScripts`: Dictionary<string, string>
- `TriggerScripts`: Dictionary<string, string>
- `ExecutionOrderFile`: string - Path to _execution_order.txt

**Directory Structure**:
```
{RootDirectory}/
├── tables/
│   ├── dbo.Customers.sql
│   ├── dbo.Orders.sql
│   └── ...
├── data/
│   ├── dbo.Customers.sql
│   ├── dbo.Orders.sql
│   └── ...
├── views/
│   ├── dbo.CustomerOrders.sql
│   └── ...
├── procedures/
│   ├── dbo.uspGetCustomers.sql
│   └── ...
├── functions/
│   ├── dbo.fnGetOrderTotal.sql
│   └── ...
├── triggers/
│   ├── dbo.trgCustomersAudit.sql
│   └── ...
└── _execution_order.txt  # Lists files in correct execution order
```

**Methods**:
- `CreateDirectoryStructure()`: Create all subdirectories
- `GetFilePath(objectType, schemaName, objectName)`: Get path for object
- `GenerateExecutionOrder(DependencyGraph)`: Create execution order file

---

## Enumerations

### ConstraintType
- `PrimaryKey`
- `ForeignKey`
- `Unique`
- `Check`
- `Default`

### ProgrammableObjectType
- `StoredProcedure`
- `ScalarFunction`
- `InlineTableFunction`
- `MultiStatementTableFunction`
- `View`
- `Trigger`

---

## Entity Relationships Diagram (Text)

```
DatabaseConnection (1) ─── creates ──> (1) DatabaseSchema
                                              │
                                              ├─ contains ─> (many) TableMetadata
                                              │                  │
                                              │                  ├─ has ─> (many) ColumnMetadata
                                              │                  ├─ has ─> (many) ConstraintMetadata
                                              │                  └─ has ─> (many) IndexMetadata
                                              │                              │
                                              │                              └─ has ─> (many) IndexColumnMetadata
                                              │
                                              ├─ contains ─> (many) ProgrammableObject
                                              │
                                              └─ has ─> (1) DependencyGraph
                                                           │
                                                           ├─ contains ─> (many) DependencyNode
                                                           └─ contains ─> (many) DependencyEdge

ExportConfiguration (1) ─── configures ──> (1) Export Process

ScriptOutput (1) ─── organizes ──> (many) Generated Files
```

---

## Data Flow

1. **Connection Phase**:
   - User provides `ExportConfiguration`
   - System creates `DatabaseConnection`
   - Connection established with retry logic

2. **Schema Reading Phase**:
   - System queries SQL Server system catalogs
   - Creates `DatabaseSchema` with all metadata
   - Populates `TableMetadata`, `ProgrammableObject` lists

3. **Dependency Analysis Phase**:
   - System builds `DependencyGraph` from foreign keys and object references
   - Runs Tarjan's algorithm to detect cycles
   - Generates topological order for script execution

4. **Script Generation Phase**:
   - For each object in dependency order:
     - Generate version-appropriate SQL scripts
     - Write to appropriate directory in `ScriptOutput`
   - Generate `_execution_order.txt` file

5. **Data Export Phase** (if `IncludeData`):
   - For each table in dependency order:
     - Query data in batches
     - Generate batched INSERT statements
     - Write to data/ directory

---

## Validation Rules Summary

| Entity | Key Validations |
|--------|----------------|
| DatabaseConnection | Server/database not empty, credentials if SQL auth |
| TableMetadata | SchemaName and TableName not empty |
| ColumnMetadata | Name, type, position valid; identity/computed logic |
| ConstraintMetadata | Type-specific properties set (FK refs, check expr, etc.) |
| IndexMetadata | Columns not empty, valid clustered/unique combination |
| ProgrammableObject | Name, type, definition not empty |
| ExportConfiguration | Output writable, at least one object type selected |
| DependencyGraph | No unresolvable cycles beyond foreign key handling |

---

## Performance Considerations

- **RowCount** in `TableMetadata`: Use `sys.dm_db_partition_stats` for fast approximate counts
- **Batching**: 1,000 rows default balances memory and performance
- **Parallelization**: Data export can parallelize across tables (future optimization)
- **Memory**: Stream data export, don't load entire tables into memory
- **Progress**: Report progress every 100 objects or 10% completion

**Data model complete. Ready for contract definition.**
