# Quickstart Guide: MSSQL Database Dump Tool

**Date**: 2025-10-14  
**For**: Developers and testers  
**Purpose**: Get started with development, testing, and using the tool

## Prerequisites

- **.NET 8 SDK** installed ([Download](https://dotnet.microsoft.com/download/dotnet/8.0))
- **Docker** installed ([Download](https://docs.docker.com/get-docker/))
- **just** command runner ([Install](https://github.com/casey/just#installation))
- **Git** for version control

## Quick Start (5 Minutes)

### 1. Clone and Setup

```bash
# Clone the repository
git clone <repository-url>
cd mssql-database-dump

# Restore dependencies
just restore

# Start Docker test containers
just docker-up
```

### 2. Build the Project

```bash
just build
```

### 3. Run Tests

```bash
# Run unit tests (no Docker required)
just test-unit

# Run integration tests (requires Docker containers)
just test-integration

# Run all tests
just test
```

### 4. Try the Tool

```bash
# Export Northwind database (schema only)
just run dump -s localhost -d Northwind -w --schema-only -o ./test-output

# View generated files
ls -R ./test-output
```

---

## Development Workflow

### Day-to-Day Commands

```bash
# Start development environment
just docker-up           # Start SQL Server containers
just setup-testdb        # Load Northwind database

# Make code changes...

# Run tests
just test-unit           # Fast unit tests
just test-integration    # Integration tests against Docker

# Clean and rebuild
just clean
just build

# Stop Docker when done
just docker-down
```

### Justfile Targets Reference

| Target | Description |
|--------|-------------|
| `just restore` | Restore NuGet dependencies |
| `just build` | Build the project (Release configuration) |
| `just test-unit` | Run unit tests only |
| `just test-integration` | Run integration tests (requires Docker) |
| `just test` | Run all tests |
| `just docker-up` | Start SQL Server Docker containers |
| `just docker-down` | Stop and remove containers |
| `just setup-testdb` | Initialize Northwind database in containers |
| `just clean` | Clean build artifacts |
| `just run <args>` | Run the tool with arguments |
| `just publish` | Create release build in ./dist |
| `just ci` | Full CI pipeline (clean, build, test) |
| `just watch` | Run in watch mode (auto-rebuild on file changes) |

---

## Testing Scenarios

### Scenario 1: Schema Export (P1)

**Goal**: Verify schema-only export works correctly

```bash
# Start Docker and setup database
just docker-up
just setup-testdb

# Run the tool in schema-only mode
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- \
  dump \
  --server localhost \
  --database Northwind \
  --user sa \
  --password 'YourStrong!Passw0rd' \
  --schema-only \
  --output ./output/schema-test

# Verify output
ls -R ./output/schema-test/

# Expected:
# - tables/ directory with .sql files
# - views/ directory
# - procedures/ directory
# - functions/ directory
# - triggers/ directory
# - _execution_order.txt file
# - NO data/ directory (schema-only mode)
```

**Verification**:
```bash
# Count files
find ./output/schema-test -name "*.sql" | wc -l

# Check execution order exists
cat ./output/schema-test/_execution_order.txt

# Verify idempotency: Run scripts twice
sqlcmd -S localhost -U sa -P 'YourStrong!Passw0rd' \
  -d TempDB -i ./output/schema-test/tables/dbo.Customers.sql
# Should succeed both times
```

---

### Scenario 2: Full Database Export (P2)

**Goal**: Verify schema + data export and restoration

```bash
# Export with data
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- \
  dump \
  --server localhost \
  --database Northwind \
  --user sa \
  --password 'YourStrong!Passw0rd' \
  --output ./output/full-backup

# Create new empty database
sqlcmd -S localhost -U sa -P 'YourStrong!Passw0rd' \
  -Q "CREATE DATABASE NorthwindRestore"

# Execute scripts in order
while read file; do
  echo "Executing $file..."
  sqlcmd -S localhost -U sa -P 'YourStrong!Passw0rd' \
    -d NorthwindRestore \
    -i "./output/full-backup/$file"
done < ./output/full-backup/_execution_order.txt

# Verify row counts match
sqlcmd -S localhost -U sa -P 'YourStrong!Passw0rd' \
  -Q "SELECT 'Original' as DB, COUNT(*) FROM Northwind.dbo.Customers
      UNION ALL
      SELECT 'Restored' as DB, COUNT(*) FROM NorthwindRestore.dbo.Customers"
```

**Expected**: Row counts should match for all tables

---

### Scenario 3: Programmable Objects (P3)

**Goal**: Verify stored procedures, functions, triggers, views are exported

```bash
# Export all object types
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- \
  dump \
  --server localhost \
  --database Northwind \
  --user sa \
  --password 'YourStrong!Passw0rd' \
  --schema-only \
  --output ./output/objects-test

# Check each object type exists
ls ./output/objects-test/procedures/
ls ./output/objects-test/functions/
ls ./output/objects-test/views/
ls ./output/objects-test/triggers/

# Verify a procedure works after restore
sqlcmd -S localhost -U sa -P 'YourStrong!Passw0rd' \
  -d NorthwindRestore \
  -Q "EXEC dbo.uspGetCustomers"
```

---

### Scenario 4: Selective Export (P4)

**Goal**: Verify filtering by pattern and object type

```bash
# Export only procedures matching pattern
dotnet run --project src/MsSqlDump/MsSqlDump.csproj -- \
  dump \
  --server localhost \
  --database Northwind \
  --user sa \
  --password 'YourStrong!Passw0rd' \
  --schema-only \
  --tables=false \
  --views=false \
  --functions=false \
  --triggers=false \
  --procedures \
  --include "^usp.*" \
  --output ./output/procedures-only

# Verify only procedures directory exists
ls ./output/procedures-only/
# Should only see: procedures/ and _execution_order.txt
```

---

## Docker Environment Details

### SQL Server Versions

The Docker setup includes multiple SQL Server versions for compatibility testing:

| Container | Version | Port | Purpose |
|-----------|---------|------|---------|
| mssql-2017 | SQL Server 2017 | 1433 | Minimum supported version |
| mssql-2019 | SQL Server 2019 | 1434 | Common production version |
| mssql-2022 | SQL Server 2022 | 1435 | Latest version testing |

### Connection Strings

```bash
# SQL 2017
Server=localhost,1433;Database=Northwind;User Id=sa;Password=YourStrong!Passw0rd;TrustServerCertificate=True

# SQL 2019
Server=localhost,1434;Database=Northwind;User Id=sa;Password=YourStrong!Passw0rd;TrustServerCertificate=True

# SQL 2022
Server=localhost,1435;Database=Northwind;User Id=sa;Password=YourStrong!Passw0rd;TrustServerCertificate=True
```

### Test Database Setup

The Northwind database is enhanced with additional object types:

**Standard Northwind Objects**:
- 13 tables (Customers, Orders, Products, etc.)
- 3 views
- Original stored procedures

**Enhanced Objects** (for comprehensive testing):
- **Scalar functions**: `dbo.fnGetOrderTotal`, `dbo.fnFormatPhone`
- **Table-valued functions**: `dbo.fnGetCustomerOrders`, `dbo.fnGetProductsByCategory`
- **Triggers**: `dbo.trgCustomersAudit` (audit log), `dbo.trgOrdersUpdate` (timestamp)
- **Additional views**: `dbo.vwCustomerOrderSummary`, `dbo.vwProductInventory`
- **Computed columns**: `Orders.TotalAmount` computed from OrderDetails
- **Check constraints**: `Products.UnitPrice > 0`, `Orders.OrderDate <= ShippedDate`
- **Circular FK**: Create test scenario with self-referencing table

### Manual Docker Commands

```bash
# Start containers
docker-compose -f tests/TestDatabases/docker-compose.yml up -d

# Check container status
docker-compose -f tests/TestDatabases/docker-compose.yml ps

# View logs
docker-compose -f tests/TestDatabases/docker-compose.yml logs mssql-2019

# Connect to container
docker exec -it mssql-2019 /opt/mssql-tools/bin/sqlcmd -S localhost -U sa -P 'YourStrong!Passw0rd'

# Stop containers
docker-compose -f tests/TestDatabases/docker-compose.yml down

# Remove volumes (clean slate)
docker-compose -f tests/TestDatabases/docker-compose.yml down -v
```

---

## IDE Setup

### Visual Studio Code

**Recommended Extensions**:
- C# Dev Kit (Microsoft)
- .NET Core Test Explorer
- Docker (Microsoft)
- Just (skellock.just)

**launch.json** example:
```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "name": "Debug Tool",
      "type": "coreclr",
      "request": "launch",
      "preLaunchTask": "build",
      "program": "${workspaceFolder}/src/MsSqlDump/bin/Debug/net8.0/MsSqlDump.dll",
      "args": [
        "dump",
        "--server", "localhost",
        "--database", "Northwind",
        "--user", "sa",
        "--password", "YourStrong!Passw0rd",
        "--schema-only",
        "--output", "./debug-output"
      ],
      "cwd": "${workspaceFolder}",
      "console": "integratedTerminal",
      "stopAtEntry": false
    }
  ]
}
```

### Visual Studio 2022

1. Open `MsSqlDump.sln`
2. Set `MsSqlDump` as startup project
3. Configure command-line arguments in Project Properties → Debug
4. Press F5 to debug

---

## Troubleshooting

### Docker containers won't start

```bash
# Check Docker is running
docker ps

# Check port conflicts
lsof -i :1433
lsof -i :1434
lsof -i :1435

# Recreate containers
just docker-down
docker system prune -f
just docker-up
```

### SQL Server connection fails

```bash
# Wait longer for SQL to be ready (takes ~30-60 seconds)
sleep 60

# Check SQL Server logs
docker logs mssql-2019

# Test connection manually
sqlcmd -S localhost -U sa -P 'YourStrong!Passw0rd' -Q "SELECT @@VERSION"
```

### Northwind database not found

```bash
# Run setup script manually
just setup-testdb

# Or manually create
sqlcmd -S localhost -U sa -P 'YourStrong!Passw0rd' -i tests/TestDatabases/Northwind/setup.sql
```

### Tests fail with timeout

- Increase timeout in test configuration
- Check Docker resource limits (CPU, memory)
- Ensure no other SQL Server instances conflict

### Build errors

```bash
# Clean and restore
just clean
dotnet restore
dotnet build
```

---

## Common Development Tasks

### Add a New Exporter

1. Create file in `src/MsSqlDump/Exporters/`
2. Implement export logic
3. Add unit tests in `tests/MsSqlDump.Tests/Unit/`
4. Add integration test in `tests/MsSqlDump.Tests/Integration/`
5. Update `DumpCommand.cs` to use new exporter

### Test Against Different SQL Versions

```bash
# Export from SQL 2017
dotnet run -- dump -s localhost,1433 -d Northwind -u sa -p 'YourStrong!Passw0rd' -o ./output-2017

# Export from SQL 2019
dotnet run -- dump -s localhost,1434 -d Northwind -u sa -p 'YourStrong!Passw0rd' -o ./output-2019

# Export from SQL 2022
dotnet run -- dump -s localhost,1435 -d Northwind -u sa -p 'YourStrong!Passw0rd' -o ./output-2022

# Compare outputs
diff -r ./output-2017 ./output-2019
```

### Debug SQL Generation

Enable verbose logging:
```csharp
// In your code
Console.WriteLine($"Generated SQL: {sqlScript}");
```

Or use a debugger breakpoint in exporter classes.

---

## Next Steps

After completing this quickstart:

1. **Review the data model**: See `data-model.md` for entity definitions
2. **Review CLI contract**: See `contracts/cli-interface.md` for interface specification
3. **Check implementation tasks**: Run `/speckit.tasks` to generate task breakdown
4. **Start implementing P1**: Begin with schema-only export (highest priority)

**Quickstart guide complete. Ready for agent context update.**
