# MSSQL Database Dump Tool

A production-ready command-line tool for exporting Microsoft SQL Server databases to organized SQL scripts, enabling version control, environment provisioning, and database portability.

## Features

- ✅ **Schema-Only Export** - Export complete database structure (tables, constraints, indexes) in dependency order
- ✅ **Schema + Data Export** - Include data with batched INSERT statements (configurable batch size)
- ✅ **Programmable Objects** - Export stored procedures, functions, triggers, and views with dependencies
- ✅ **Selective Filtering** - Filter by object types, schemas, and regex patterns
- ✅ **Idempotent Scripts** - DROP IF EXISTS patterns for safe re-execution
- ✅ **Version Compatibility** - SQL Server 2008-2022 support with version-aware syntax
- ✅ **Dependency Ordering** - Automatically resolves foreign key dependencies and circular references
- ✅ **Progress Feedback** - Beautiful console progress bars and real-time status updates
- ✅ **Robust Connection** - Built-in retry logic and connection pooling

## Table of Contents

- [Quick Start](#quick-start)
- [Installation](#installation)
- [Usage Examples](#usage-examples)
- [CLI Reference](#cli-reference)
- [Output Structure](#output-structure)
- [Troubleshooting](#troubleshooting)
- [Development](#development)

## Quick Start

### Prerequisites

- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0) or later
- SQL Server 2008 or later (local or remote)
- [Docker](https://www.docker.com/) (optional, for testing)
- [Just](https://github.com/casey/just) (optional, for build automation)

### Installation

```bash
# Clone the repository
git clone <repository-url>
cd speckit-test

# Build the project
dotnet build src/MsSqlDump/MsSqlDump.csproj

# Run the tool
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump --help
```

**Using Just:**
```bash
just build
just run dump --help
```

## Usage Examples

### User Story 1: Schema-Only Export

Export database schema without data for version control:

```bash
# Windows Authentication
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --schema-only \
  --output ./schema

# SQL Authentication
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d Northwind \
  -u sa \
  -p 'YourPassword' \
  --schema-only \
  -o ./schema
```

**Output:**
```
schema/
├── tables/
│   ├── dbo.Customers.sql
│   ├── dbo.Orders.sql
│   └── dbo.OrderDetails.sql
└── execution_order.txt
```

### User Story 2: Schema + Data Export

Export complete database with data for environment provisioning:

```bash
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --output ./backup \
  --batch-size 500

# With custom timeout and retry settings
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d Northwind \
  -u sa \
  -p 'YourPassword' \
  -o ./backup \
  --batch-size 1000 \
  --timeout 60 \
  --retries 5
```

**Output:**
```
backup/
├── tables/
│   ├── dbo.Customers.sql       (CREATE TABLE)
│   └── dbo.Orders.sql          (CREATE TABLE)
├── data/
│   ├── dbo.Customers.sql       (INSERT statements)
│   └── dbo.Orders.sql          (INSERT statements)
└── execution_order.txt
```

### User Story 3: Export Programmable Objects

Export stored procedures, functions, triggers, and views:

```bash
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --output ./complete \
  --schema-only

# Export only specific object types
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d Northwind \
  -w \
  -o ./procs-only \
  --schema-only \
  --no-tables \
  --no-views \
  --no-functions \
  --no-triggers
```

**Output:**
```
complete/
├── tables/
├── views/
│   ├── dbo.CustomerOrders.sql
│   └── sales.MonthlySummary.sql
├── procedures/
│   ├── dbo.GetCustomerOrders.sql
│   └── sales.CalculateTotal.sql
├── functions/
│   ├── dbo.FormatPhone.sql       (scalar function)
│   ├── dbo.GetCustomers.sql       (table-valued function)
│   └── dbo.SplitString.sql        (inline table-valued)
├── triggers/
│   └── dbo.Orders_UpdateTrigger.sql
└── execution_order.txt
```

### User Story 4: Selective Filtering

Export only specific schemas or objects matching patterns:

```bash
# Export only 'dbo' and 'sales' schemas
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --output ./filtered \
  --include "dbo\..*" \
  --include "sales\..*"

# Export all except temporary and audit tables
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d Northwind \
  -w \
  -o ./filtered \
  --exclude ".*Temp.*" \
  --exclude ".*Audit.*" \
  --exclude ".*_bak"

# Combine filters
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d Northwind \
  -w \
  -o ./customer-related \
  --include "dbo\.Customer.*" \
  --include "dbo\.Order.*" \
  --exclude ".*History"
```

### Advanced Options

```bash
# Verbose output for debugging
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d Northwind \
  -w \
  -o ./backup \
  --verbose

# Check version
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- --version

# Export large database with custom batch size
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d LargeDB \
  -u sa \
  -p 'YourPassword' \
  -o ./large-export \
  --batch-size 5000 \
  --timeout 120
```

## CLI Reference

### Connection Options

| Option | Short | Required | Description |
|--------|-------|----------|-------------|
| `--server` | `-s` | Yes | SQL Server hostname or IP address |
| `--database` | `-d` | Yes | Database name to export |
| `--user` | `-u` | No* | SQL authentication username |
| `--password` | `-p` | No* | SQL authentication password |
| `--windows-auth` | `-w` | No* | Use Windows authentication |
| `--timeout` | | No | Connection timeout in seconds (default: 30) |
| `--retries` | | No | Connection retry attempts (default: 3) |

*Either `--windows-auth` or `--user` + `--password` is required.

### Export Options

| Option | Short | Description |
|--------|-------|-------------|
| `--output` | `-o` | Output directory path (default: ./output) |
| `--schema-only` | | Export schema without data |
| `--batch-size` | | Rows per INSERT batch (default: 1000) |

### Object Type Filters

| Option | Description |
|--------|-------------|
| `--no-tables` | Skip table export |
| `--no-views` | Skip view export |
| `--no-procedures` | Skip stored procedure export |
| `--no-functions` | Skip function export |
| `--no-triggers` | Skip trigger export |

### Pattern Filters

| Option | Description |
|--------|-------------|
| `--include` | Regex pattern for objects to include (can specify multiple) |
| `--exclude` | Regex pattern for objects to exclude (can specify multiple) |

Patterns match against `[schema].[objectname]` format (e.g., `dbo.Customers`).

### Other Options

| Option | Description |
|--------|-------------|
| `--verbose` | Show detailed logging output |
| `--version` | Display tool version |
| `--help` | Show help information |

## Output Structure

The tool creates an organized directory structure:

```
output/
├── tables/
│   ├── schema.tablename.sql    (CREATE TABLE + constraints + indexes)
├── data/                        (only if not --schema-only)
│   ├── schema.tablename.sql    (INSERT statements)
├── views/
│   ├── schema.viewname.sql     (CREATE VIEW)
├── procedures/
│   ├── schema.procname.sql     (CREATE PROCEDURE)
├── functions/
│   ├── schema.funcname.sql     (CREATE FUNCTION)
├── triggers/
│   ├── schema.triggername.sql  (CREATE TRIGGER)
└── execution_order.txt          (recommended execution sequence)
```

### Execution Order File

The `execution_order.txt` file lists scripts in dependency-safe order:

```
# Tables (in dependency order)
tables/dbo.Customers.sql
tables/dbo.Orders.sql
tables/dbo.OrderDetails.sql

# Data (if included)
data/dbo.Customers.sql
data/dbo.Orders.sql
data/dbo.OrderDetails.sql

# Views (in dependency order)
views/dbo.CustomerOrders.sql

# Procedures
procedures/dbo.GetCustomerOrders.sql

# Functions
functions/dbo.FormatPhone.sql

# Triggers
triggers/dbo.Orders_UpdateTrigger.sql
```

## Troubleshooting

### Connection Issues

**Error: Login failed for user**
```bash
# Check authentication method
# For Windows auth:
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump -s localhost -d Northwind -w -o ./out

# For SQL auth, ensure correct password:
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump -s localhost -d Northwind -u sa -p 'YourPassword' -o ./out
```

**Error: Connection timeout**
```bash
# Increase timeout and retries:
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s remote-server \
  -d Northwind \
  -u sa \
  -p 'YourPassword' \
  -o ./out \
  --timeout 60 \
  --retries 5
```

**Error: Cannot open database**
```bash
# Verify database name (case-sensitive on Linux SQL Server):
sqlcmd -S localhost -U sa -P 'YourPassword' -Q "SELECT name FROM sys.databases"
```

### Export Issues

**Error: Permission denied on output directory**
```bash
# Ensure directory is writable:
mkdir -p ./output
chmod 755 ./output
```

**Error: Invalid pattern in --include/--exclude**
```bash
# Use valid regex patterns (escaped dots):
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d Northwind \
  -w \
  -o ./out \
  --include "dbo\.Customer.*"  # Correct: escaped dot
```

**Large database export is slow**
```bash
# Increase batch size and use schema-only initially:
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d LargeDB \
  -u sa \
  -p 'YourPassword' \
  -o ./out \
  --schema-only  # Export schema first to verify

# Then export data with larger batches:
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d LargeDB \
  -u sa \
  -p 'YourPassword' \
  -o ./out-data \
  --batch-size 5000
```

### Debugging

Enable verbose output to see detailed progress:

```bash
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost \
  -d Northwind \
  -w \
  -o ./debug \
  --verbose
```

This shows:
- Connection details
- SQL version detection
- Object discovery statistics
- Dependency analysis (including circular dependencies)
- Export progress with file sizes
- Total export size and object counts
```bash
just run dump --server localhost --database Northwind --windows-auth --output ./filtered --include "dbo\.Customer.*" --exclude ".*Temp.*"
```

## Development

### Setup Test Environment

The project includes a complete Docker-based test environment with multiple SQL Server versions:

```bash
# Start SQL Server test containers
just docker-up
# OR
docker-compose -f tests/TestDatabases/docker-compose.yml up -d

# Setup Northwind test database
just setup-testdb
# OR
sqlcmd -S localhost -U sa -P 'YourStrong!Passw0rd' -i tests/TestDatabases/Northwind/setup.sql

# Verify containers are running
docker ps
```

**Available Test Databases:**
- SQL Server 2017 (port 1433) - Version 14
- SQL Server 2019 (port 1434) - Version 15
- SQL Server 2022 (port 1435) - Version 16

Default credentials: `sa` / `YourStrong!Passw0rd`

### Build Commands

Using Just (recommended):
```bash
# Build the project
just build

# Run the tool
just run dump --help

# Run tests
just test

# Clean build artifacts
just clean

# Publish standalone executable
just publish

# Stop Docker containers
just docker-down
```

Using dotnet CLI directly:
```bash
# Build
dotnet build src/MsSqlDump/MsSqlDump.csproj

# Run
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump --help

# Test
dotnet test tests/MsSqlDump.Tests/MsSqlDump.Tests.csproj

# Clean
dotnet clean

# Publish
dotnet publish src/MsSqlDump/MsSqlDump.csproj -c Release -o ./publish
```

### Running Tests

```bash
# Run all tests
just test

# Run specific test
dotnet test tests/MsSqlDump.Tests/MsSqlDump.Tests.csproj --filter "FullyQualifiedName~SchemaReader"

# Run tests with verbose output
dotnet test tests/MsSqlDump.Tests/MsSqlDump.Tests.csproj --logger "console;verbosity=detailed"
```

### Testing Against Different SQL Server Versions

```bash
# Test against SQL Server 2017
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost:1433 \
  -d Northwind \
  -u sa \
  -p 'YourStrong!Passw0rd' \
  -o ./test-2017 \
  --verbose

# Test against SQL Server 2019
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost:1434 \
  -d Northwind \
  -u sa \
  -p 'YourStrong!Passw0rd' \
  -o ./test-2019 \
  --verbose

# Test against SQL Server 2022
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- dump \
  -s localhost:1435 \
  -d Northwind \
  -u sa \
  -p 'YourStrong!Passw0rd' \
  -o ./test-2022 \
  --verbose
```

## Project Structure

```
speckit-test/
├── src/
│   └── MsSqlDump/              # Main application
│       ├── Commands/           # CLI command implementation
│       ├── Core/               # Core functionality (connection, schema reading, dependencies)
│       ├── Exporters/          # Export engines (table, data, programmable objects)
│       ├── Models/             # Data models
│       ├── Utils/              # Utilities (progress, quoter, filter)
│       └── Writers/            # File output (script writer, directory organizer)
├── tests/
│   ├── MsSqlDump.Tests/        # Unit and integration tests
│   └── TestDatabases/          # Docker test environment
│       ├── docker-compose.yml  # Multi-version SQL Server containers
│       └── Northwind/          # Sample database setup
├── docker/                     # Docker configurations
├── specs/                      # Feature specifications and planning
└── Justfile                    # Build automation
```

### Key Components

- **DatabaseConnection**: Connection management with retry logic
- **SqlVersionDetector**: SQL Server version detection for syntax compatibility
- **SchemaReader**: Reads metadata from system tables (sys.tables, sys.columns, etc.)
- **DependencyResolver**: Topological sort with circular dependency detection (Tarjan's algorithm)
- **TableExporter**: CREATE TABLE script generation
- **DataExporter**: Batched INSERT statement generation
- **View/Procedure/Function/TriggerExporter**: Programmable object export
- **ObjectFilter**: Regex-based pattern matching for selective export
- **ScriptWriter**: File writing with UTF-8 encoding
- **DirectoryOrganizer**: Output directory structure creation
- **DumpCommand**: Main CLI command orchestration

## Architecture

### SQL Version Compatibility

The tool automatically detects SQL Server version and generates compatible syntax:

- **SQL Server 2016+**: Uses `DROP IF EXISTS` syntax
- **SQL Server 2008-2014**: Uses `IF EXISTS (SELECT...) DROP` pattern
- **All versions**: Handles data type differences (e.g., DATE availability, NVARCHAR(MAX))

### Dependency Resolution

Tables are exported in dependency order to ensure foreign key constraints can be created successfully:

1. Build directed graph from foreign key relationships
2. Perform topological sort
3. Detect circular dependencies using Tarjan's algorithm
4. Export independent tables first, then dependent tables
5. For circular dependencies, temporarily disable FK constraints during data import

### Data Export Strategy

- Stream results to avoid memory issues with large tables
- Batch INSERT statements (default: 1,000 rows per batch)
- Handle identity columns with `SET IDENTITY_INSERT`
- Properly escape and format all SQL data types
- Generate NULL values correctly
- Handle circular FK dependencies with constraint disable/enable scripts

## Contributing

Contributions are welcome! Please follow these guidelines:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Write tests for new functionality
4. Ensure all tests pass (`just test`)
5. Follow C# coding conventions and .NET 8 best practices
6. Add XML documentation comments to public APIs
7. Update README.md for new features
8. Commit changes (`git commit -m 'Add amazing feature'`)
9. Push to branch (`git push origin feature/amazing-feature`)
10. Open a Pull Request

## License

MIT License - See LICENSE file for details.

## Support

For issues, questions, or feature requests:
- Open an issue on GitHub
- Check [specs/001-mssql-database-dump/](specs/001-mssql-database-dump/) for detailed documentation
- See [troubleshooting section](#troubleshooting) above

## Roadmap

Future enhancements:
- [ ] Extended properties export
- [ ] XML schema collection support
- [ ] User-defined types
- [ ] Partitioned tables support
- [ ] Compression support
- [ ] Parallel export for large databases
- [ ] Differential/incremental export
- [ ] Export to other formats (JSON, CSV)

## Acknowledgments

Built with:
- [.NET 8](https://dotnet.microsoft.com/)
- [Microsoft.Data.SqlClient](https://www.nuget.org/packages/Microsoft.Data.SqlClient)
- [System.CommandLine](https://www.nuget.org/packages/System.CommandLine)
- [Spectre.Console](https://spectreconsole.net/)
