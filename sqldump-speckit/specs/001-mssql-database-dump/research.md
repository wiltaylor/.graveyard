# Technical Research: MSSQL Database Dump Tool

**Date**: 2025-10-14  
**Feature**: MSSQL Database Dump Tool  
**Purpose**: Resolve all technical unknowns and establish best practices for implementation

## Research Areas

### 1. .NET 8 MSSQL Connectivity Libraries

**Decision**: Use **Microsoft.Data.SqlClient** (v5.x)

**Rationale**:
- Microsoft.Data.SqlClient is the modern, actively maintained library (successor to System.Data.SqlClient)
- Full support for SQL Server 2008-2022+ including Azure SQL
- Better performance and async/await support
- Active development with security updates
- Cross-platform (.NET Core, .NET 5+)
- Supports connection resiliency and retry logic natively

**Alternatives Considered**:
- System.Data.SqlClient: Deprecated, legacy .NET Framework library, no new features
- Dapper: ORM layer, adds unnecessary complexity for schema introspection
- Entity Framework Core: Heavy ORM, not suitable for raw SQL script generation

**Implementation Notes**:
- Use `SqlConnectionStringBuilder` for connection string construction
- Leverage `SqlConnection.RetryLogic` for built-in retry with exponential backoff
- Use async methods (`ExecuteReaderAsync`, `ExecuteNonQueryAsync`) for better performance

**NuGet Package**: `Microsoft.Data.SqlClient` version 5.1.x or later

---

### 2. Command-Line Interface Framework

**Decision**: Use **System.CommandLine** (v2.0.0-beta4 or later)

**Rationale**:
- Official Microsoft library for CLI applications in .NET
- Modern, type-safe command definition with attributes or fluent API
- Built-in help generation, tab completion, suggestions
- Excellent validation and error handling
- Integrates well with dependency injection

**Alternatives Considered**:
- CommandLineParser: Older library, less modern API
- Spectre.Console.Cli: Good for simple cases, but System.CommandLine is more powerful
- Manual parsing: Error-prone, no help generation, poor UX

**Implementation Notes**:
```csharp
// Example command structure
var rootCommand = new RootCommand("MSSQL Database Dump Tool");
var dumpCommand = new Command("dump", "Export database to SQL scripts");

dumpCommand.AddOption(new Option<string>("--server", "SQL Server hostname"));
dumpCommand.AddOption(new Option<string>("--database", "Database name"));
dumpCommand.AddOption(new Option<bool>("--schema-only", "Export schema without data"));
dumpCommand.AddOption(new Option<string>("--output", "Output directory path"));
```

**NuGet Package**: `System.CommandLine` version 2.0.0-beta4.22272.1 or later

---

### 3. Console Output and Progress Feedback

**Decision**: Use **Spectre.Console** (v0.48+)

**Rationale**:
- Rich, beautiful console output with colors, tables, progress bars
- Cross-platform ANSI support
- Excellent progress reporting for long-running operations
- Tree views for dependency visualization (debugging)
- Status spinners and live updates

**Alternatives Considered**:
- Console.WriteLine: Basic, no visual feedback for progress
- ShellProgressBar: Good but less feature-rich than Spectre.Console
- Kurukuru: Lightweight spinners only

**Implementation Notes**:
```csharp
// Example progress reporting
await AnsiConsole.Progress()
    .StartAsync(async ctx =>
    {
        var task = ctx.AddTask("[green]Exporting tables[/]");
        foreach (var table in tables)
        {
            await ExportTable(table);
            task.Increment(100.0 / tables.Count);
        }
    });
```

**NuGet Package**: `Spectre.Console` version 0.48.0 or later

---

### 4. SQL Server Version Compatibility (2008-2022)

**Decision**: Implement version-aware SQL generation with fallback patterns

**Rationale**:
- SQL Server 2008 lacks many modern features (e.g., `DROP TABLE IF EXISTS`, `CREATE OR ALTER`)
- Must query `@@VERSION` or `SERVERPROPERTY('ProductVersion')` to detect version
- Generate version-appropriate T-SQL syntax

**Key Compatibility Considerations**:

**SQL 2008 Limitations**:
- No `DROP IF EXISTS` for tables (introduced SQL 2016)
- No `CREATE OR ALTER` for procedures/functions (introduced SQL 2016)
- Must use `IF EXISTS (SELECT * FROM sys.objects WHERE ...) DROP TABLE ...`
- No `FORMAT()` function
- No sequences (introduced SQL 2012)

**Compatibility Matrix**:

| Feature | SQL 2008 | SQL 2012+ | SQL 2016+ |
|---------|----------|-----------|-----------|
| DROP TABLE IF EXISTS | ❌ Use IF EXISTS | ❌ Use IF EXISTS | ✅ Native |
| DROP PROCEDURE IF EXISTS | ❌ Use IF EXISTS | ❌ Use IF EXISTS | ✅ Native |
| CREATE OR ALTER | ❌ Use DROP+CREATE | ❌ Use DROP+CREATE | ✅ Native |
| Sequences | ❌ N/A | ✅ Supported | ✅ Supported |
| Column Store Indexes | ❌ N/A | ✅ Supported | ✅ Supported |

**Implementation Strategy**:
```csharp
// Version detection
var version = await connection.QuerySingleAsync<string>(
    "SELECT SERVERPROPERTY('ProductVersion')");
var majorVersion = int.Parse(version.Split('.')[0]);

// Version-aware DROP pattern
string GenerateDropTable(string tableName, int sqlVersion)
{
    if (sqlVersion >= 13) // SQL 2016+
        return $"DROP TABLE IF EXISTS [{tableName}];";
    else
        return $@"IF EXISTS (SELECT * FROM sys.objects 
                  WHERE object_id = OBJECT_ID(N'[{tableName}]') 
                  AND type = 'U')
                  DROP TABLE [{tableName}];";
}
```

**System Catalog Queries** (compatible with SQL 2008+):
- `sys.tables`, `sys.columns`, `sys.indexes`, `sys.foreign_keys`
- `sys.check_constraints`, `sys.default_constraints`
- `sys.procedures`, `sys.views`, `sys.triggers`
- `INFORMATION_SCHEMA` views (older but universally supported)

---

### 5. Dependency Graph Algorithm

**Decision**: Implement **Topological Sort with Tarjan's Algorithm** for circular dependency detection

**Rationale**:
- Database schemas can have circular dependencies via foreign keys
- Topological sort handles DAGs (directed acyclic graphs)
- Tarjan's algorithm detects strongly connected components (cycles)
- Must disable constraints during import for circular dependencies

**Algorithm Overview**:
1. Build dependency graph from foreign key relationships
2. Detect cycles using Tarjan's algorithm (strongly connected components)
3. For acyclic portions: Use topological sort (Kahn's algorithm or DFS-based)
4. For cycles: Group tables in the cycle, disable FK checks during import

**Implementation Pseudocode**:
```
1. Query all tables and foreign keys from sys.foreign_keys
2. Build adjacency list: table -> [tables it depends on]
3. Run Tarjan's algorithm:
   - Identify strongly connected components (SCC)
   - Each SCC is a group of tables with circular dependencies
4. Create ordered list:
   - First: Tables with no dependencies
   - Middle: Tables in topological order
   - Special handling: SCC groups (disable FK checks)
5. Generate scripts:
   - Create tables in dependency order
   - For SCC groups: 
     a) Create tables
     b) Add data with NOCHECK constraints
     c) Re-enable and check constraints
```

**Libraries**:
- Use QuikGraph (NuGet: `QuikGraph`) for graph algorithms
- Alternative: Implement manually (150-200 lines of code)

**NuGet Package (Optional)**: `QuikGraph` version 2.5.0 or implement custom

---

### 6. Data Batching Strategy

**Decision**: Batch 1,000 rows per INSERT with parameterized multi-row inserts

**Rationale**:
- Single-row INSERTs: ~50-100 inserts/sec (very slow for large tables)
- Multi-row INSERTs: ~10,000-50,000 rows/sec (much faster)
- 1,000 rows balances:
  - Transaction log size (each batch is one transaction)
  - Memory usage (parameter limits)
  - Readability of generated SQL

**SQL Server Parameter Limits**:
- Max 2,100 parameters per query
- For table with 10 columns: 2,100 / 10 = 210 rows max per batch
- Using 1,000 rows requires careful column counting

**Alternative Approach** (if many columns):
- For tables with >20 columns: Reduce batch to 500 rows
- For tables with >100 columns: Use single-row INSERT

**Implementation Pattern**:
```sql
-- Batch of 1,000 rows
INSERT INTO [TableName] ([Col1], [Col2], [Col3])
VALUES 
    (@p0, @p1, @p2),
    (@p3, @p4, @p5),
    ...
    (@p2997, @p2998, @p2999);  -- 1000 rows * 3 columns = 3000 params
```

**Alternative for Large Tables** (>1M rows):
- Consider generating BULK INSERT scripts with CSV files
- Defer this to P4 (Selective Export) as optimization

---

### 7. Docker Test Environment Setup

**Decision**: Use **official Microsoft SQL Server Linux containers** with docker-compose

**Rationale**:
- mcr.microsoft.com/mssql/server provides official images
- Linux containers are lighter and faster than Windows containers
- docker-compose allows multi-version test matrix
- Volumes for persistent Northwind database

**Supported Versions**:
- SQL Server 2019: `mcr.microsoft.com/mssql/server:2019-latest`
- SQL Server 2022: `mcr.microsoft.com/mssql/server:2022-latest`
- SQL Server 2017: `mcr.microsoft.com/mssql/server:2017-latest`
- **Note**: SQL Server 2008 R2 not available as Linux container
  - Use 2017 as minimum for testing
  - Document 2008 compatibility through syntax checking

**docker-compose.yml Structure**:
```yaml
version: '3.8'
services:
  mssql-2017:
    image: mcr.microsoft.com/mssql/server:2017-latest
    environment:
      ACCEPT_EULA: Y
      SA_PASSWORD: YourStrong!Passw0rd
    ports:
      - "1433:1433"
  
  mssql-2019:
    image: mcr.microsoft.com/mssql/server:2019-latest
    environment:
      ACCEPT_EULA: Y
      SA_PASSWORD: YourStrong!Passw0rd
    ports:
      - "1434:1433"
  
  mssql-2022:
    image: mcr.microsoft.com/mssql/server:2022-latest
    environment:
      ACCEPT_EULA: Y
      SA_PASSWORD: YourStrong!Passw0rd
    ports:
      - "1435:1433"
```

**Northwind Database Setup**:
- Download Northwind script from Microsoft samples
- Enhance with missing object types:
  - Add user-defined functions (scalar, table-valued)
  - Add triggers (INSERT, UPDATE, DELETE)
  - Add additional views
  - Add computed columns
  - Add check constraints
- Create setup script that runs on container startup

---

### 8. Justfile Build Automation

**Decision**: Implement comprehensive Justfile with docker integration

**Justfile Targets**:

```just
# Variables
dotnet := "dotnet"
project := "src/MsSqlDump/MsSqlDump.csproj"
test_project := "tests/MsSqlDump.Tests/MsSqlDump.Tests.csproj"

# Default target
default:
    @just --list

# Restore dependencies
restore:
    {{dotnet}} restore {{project}}
    {{dotnet}} restore {{test_project}}

# Build the project
build: restore
    {{dotnet}} build {{project}} --configuration Release

# Run unit tests (no Docker required)
test-unit:
    {{dotnet}} test {{test_project}} --filter "Category=Unit"

# Start Docker containers
docker-up:
    docker-compose -f tests/TestDatabases/docker-compose.yml up -d
    @echo "Waiting for SQL Server to start..."
    @sleep 30

# Stop Docker containers
docker-down:
    docker-compose -f tests/TestDatabases/docker-compose.yml down

# Setup test databases
setup-testdb: docker-up
    {{dotnet}} run --project tests/MsSqlDump.Tests/DatabaseSetup/Setup.csproj

# Run integration tests (requires Docker)
test-integration: docker-up setup-testdb
    {{dotnet}} test {{test_project}} --filter "Category=Integration"

# Run all tests
test: test-unit test-integration

# Clean build artifacts
clean:
    {{dotnet}} clean {{project}}
    {{dotnet}} clean {{test_project}}
    rm -rf **/bin **/obj

# Run the tool (example)
run *ARGS:
    {{dotnet}} run --project {{project}} -- {{ARGS}}

# Publish release build
publish:
    {{dotnet}} publish {{project}} --configuration Release --output ./dist

# Full CI pipeline
ci: clean build test

# Development watch mode
watch:
    {{dotnet}} watch --project {{project}} run
```

**Rationale**:
- `just` is simpler than Make, works cross-platform
- Clear target names matching development workflow
- Docker integration for seamless testing
- Can chain targets (e.g., `test` runs `test-unit` and `test-integration`)

---

### 9. SQL Identifier Quoting Strategy

**Decision**: Always use square bracket quoting `[IdentifierName]` for all identifiers

**Rationale**:
- SQL Server uses square brackets `[...]` for quoted identifiers
- Handles spaces, special characters, reserved keywords
- More reliable than `QUOTENAME()` function for script generation
- Consistent with SQL Server Management Studio (SSMS) scripting

**Escaping Rules**:
- Square bracket `]` inside identifier: Escape as `]]`
- Example: Table name `[My]Table]` becomes `[My]]Table]]]`

**Implementation**:
```csharp
public static string QuoteIdentifier(string identifier)
{
    if (string.IsNullOrEmpty(identifier))
        throw new ArgumentException("Identifier cannot be null or empty");
    
    // Escape any closing brackets
    string escaped = identifier.Replace("]", "]]");
    return $"[{escaped}]";
}

// Usage
string tableName = "Order Details"; // Space in name
string quoted = QuoteIdentifier(tableName); // [Order Details]
```

**Always Quote**:
- Table names, column names, view names
- Procedure names, function names, trigger names
- Schema names, database names
- Even if not strictly necessary (consistency)

---

### 10. Testing Strategy

**Decision**: Three-tier testing approach (Unit, Integration, End-to-End)

**Unit Tests** (No external dependencies):
- DependencyResolver: Graph algorithms, cycle detection
- SqlQuoter: Identifier escaping
- SqlVersionDetector: Version string parsing
- Batching logic: Row count calculations
- Mock SqlConnection and SqlCommand

**Integration Tests** (Docker MSSQL required):
- P1: Schema export → Verify table, constraint, index scripts generated
- P2: Data export → Verify INSERT statements with correct batching
- P3: Programmable objects → Verify procedures, functions, triggers, views
- P4: Selective filtering → Verify include/exclude patterns work
- Test against multiple SQL versions (2017, 2019, 2022)

**End-to-End Tests** (Full dump and restore):
- Export Northwind database (schema + data)
- Create new empty database
- Execute generated scripts in order
- Verify:
  - Table count matches
  - Row counts match
  - Schema matches (use `sp_help` or sys catalogs)
  - Programmable objects functional (execute a procedure)
- Test idempotency: Run scripts twice, should succeed both times

**Test Database Variants**:
- Northwind (basic): Standard tables, views
- Northwind Enhanced: Add triggers, functions, computed columns
- Edge Cases DB: Special characters in names, circular FKs, deprecated types

**xUnit Attributes**:
```csharp
[Fact]  // Unit tests
[Trait("Category", "Unit")]
public void DependencyResolver_DetectsCircularDependencies() { }

[Fact]  // Integration tests
[Trait("Category", "Integration")]
public async Task SchemaExport_GeneratesCorrectTableScripts() { }
```

---

## Summary of Technical Decisions

| Area | Decision | Primary Library/Tool |
|------|----------|---------------------|
| Database Connectivity | Microsoft.Data.SqlClient v5.x | Microsoft.Data.SqlClient |
| CLI Framework | System.CommandLine v2.0-beta4+ | System.CommandLine |
| Console Output | Spectre.Console v0.48+ | Spectre.Console |
| SQL Version Support | 2008-2022 with version detection | Custom version-aware SQL generation |
| Dependency Resolution | Topological sort + Tarjan's algorithm | QuikGraph or custom implementation |
| Data Batching | 1,000 rows per INSERT | Custom batching logic |
| Testing Environment | Docker with SQL 2017/2019/2022 | docker-compose |
| Build Automation | Justfile | just command runner |
| Identifier Quoting | Always use `[...]` with escaping | Custom QuoteIdentifier() |
| Testing Strategy | Unit + Integration + E2E | xUnit with Docker |

**All research complete. Ready for Phase 1: Design & Contracts.**
