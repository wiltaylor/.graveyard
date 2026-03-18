# Frequently Asked Questions (FAQ)

Common questions and answers about the MSSQL Database Dump Tool.

## General Questions

### What is this tool for?

The MSSQL Database Dump Tool exports Microsoft SQL Server databases to organized SQL script files, enabling:
- Version control of database schemas
- Database backups in portable text format
- Environment provisioning (dev, test, staging)
- Schema comparison between environments
- Documentation and auditing

### Which SQL Server versions are supported?

SQL Server 2008 through 2022, including:
- SQL Server 2008 / 2008 R2 (version 10)
- SQL Server 2012 (version 11)
- SQL Server 2014 (version 12)
- SQL Server 2016 (version 13)
- SQL Server 2017 (version 14)
- SQL Server 2019 (version 15)
- SQL Server 2022 (version 16)

The tool automatically detects your SQL Server version and generates compatible syntax.

### Does it work with Azure SQL Database?

Yes! The tool works with:
- Azure SQL Database
- Azure SQL Managed Instance
- SQL Server on Azure VMs

Use the same connection syntax: `yourserver.database.windows.net`

### What platforms are supported?

The tool runs on any platform supporting .NET 8:
- Windows (x64, ARM64)
- Linux (x64, ARM64)
- macOS (x64, ARM64)

## Export Questions

### How do I export only the schema without data?

Use the `--schema-only` flag:

```bash
mssql-dump dump --server localhost --database MyDB --windows-auth --schema-only --output ./schema
```

### Can I export specific tables or schemas?

Yes! Use `--include` and `--exclude` patterns:

```bash
# Only dbo schema
mssql-dump dump -s localhost -d MyDB -w --include "dbo\..*" -o ./export

# Only Customer tables
mssql-dump dump -s localhost -d MyDB -w --include ".*Customer.*" -o ./export

# Exclude temporary tables
mssql-dump dump -s localhost -d MyDB -w --exclude ".*Temp.*" -o ./export
```

### What objects are exported?

The tool exports:
- **Tables**: Structure, columns, data types, collations
- **Constraints**: Primary keys, foreign keys, unique, check, default
- **Indexes**: Clustered, non-clustered, with all options
- **Data**: INSERT statements with proper escaping
- **Views**: CREATE VIEW definitions
- **Stored Procedures**: CREATE PROCEDURE definitions
- **Functions**: Scalar, inline table-valued, multi-statement table-valued
- **Triggers**: Table triggers with parent associations

Currently **NOT** exported:
- Extended properties
- XML schema collections
- User-defined types
- Partitions
- Full-text indexes
- Database-level settings
- Security (users, roles, permissions)

### How are dependencies handled?

The tool automatically:
1. Analyzes foreign key relationships
2. Performs topological sort for correct ordering
3. Detects circular dependencies using Tarjan's algorithm
4. Generates `_execution_order.txt` with proper script order
5. Creates FK constraint disable/enable scripts for circular dependencies

### What happens with circular foreign key dependencies?

For tables with circular FK dependencies, the tool:
1. Identifies the cycle
2. Exports table structure normally
3. Generates scripts to disable FKs before INSERT
4. Exports data
5. Generates scripts to re-enable FKs after INSERT

Example output:
```sql
-- Disable foreign key constraints for circular dependency
ALTER TABLE [dbo].[Table1] NOCHECK CONSTRAINT [FK_Table1_Table2];
GO

-- INSERT statements here

-- Re-enable foreign key constraints
ALTER TABLE [dbo].[Table1] CHECK CONSTRAINT [FK_Table1_Table2];
GO
```

### How is data exported?

Data is exported as:
- Batched INSERT statements (default: 1,000 rows per batch)
- Properly escaped values for all SQL data types
- `SET IDENTITY_INSERT ON/OFF` for identity columns
- NULL handling
- Binary data as hex strings
- Date/time in ISO format

### Can I change the batch size for INSERT statements?

Yes! Use `--batch-size`:

```bash
# Small batches (500 rows) for wide tables
mssql-dump dump -s localhost -d MyDB -w --batch-size 500 -o ./export

# Large batches (5000 rows) for narrow tables and fast networks
mssql-dump dump -s localhost -d MyDB -w --batch-size 5000 -o ./export
```

**Recommendations**:
- Wide tables (many columns): 100-500
- Normal tables: 500-1000
- Narrow tables: 5000-10000
- Slow networks: 100-500
- Fast networks/local: 1000-5000

## Connection Questions

### How do I connect with Windows Authentication?

Use the `--windows-auth` or `-w` flag:

```bash
mssql-dump dump --server localhost --database MyDB --windows-auth --output ./export
```

### How do I connect with SQL Authentication?

Provide `--user` and `--password`:

```bash
mssql-dump dump \
  --server localhost \
  --database MyDB \
  --user sa \
  --password 'YourPassword' \
  --output ./export
```

### How do I connect to a named instance?

Use `server\instance` format:

```bash
mssql-dump dump \
  --server localhost\SQLEXPRESS \
  --database MyDB \
  --windows-auth \
  --output ./export
```

### How do I connect to a non-default port?

Use `server:port` or `server,port` format:

```bash
# Colon syntax
mssql-dump dump --server localhost:1434 --database MyDB -w -o ./export

# Comma syntax
mssql-dump dump --server localhost,1434 --database MyDB -w -o ./export
```

### Connection keeps timing out. What should I do?

Increase the timeout and retries:

```bash
mssql-dump dump \
  --server slow-server \
  --database MyDB \
  --user sa \
  --password 'Password' \
  --timeout 300 \
  --retries 5 \
  --output ./export
```

### How do I securely pass passwords?

**Option 1**: Environment variable

```bash
export MSSQL_PASSWORD='YourSecurePassword'
mssql-dump dump -s localhost -d MyDB -u sa -p "$MSSQL_PASSWORD" -o ./export
```

**Option 2**: Script with restricted permissions

```bash
#!/bin/bash
# set-credentials.sh (chmod 700)
export MSSQL_SERVER="production-server"
export MSSQL_DATABASE="ProductionDB"
export MSSQL_USER="backup_user"
export MSSQL_PASSWORD="SecurePassword123"
```

```bash
source ./set-credentials.sh
mssql-dump dump -s "$MSSQL_SERVER" -d "$MSSQL_DATABASE" -u "$MSSQL_USER" -p "$MSSQL_PASSWORD" -o ./export
```

**Option 3**: Use Windows Authentication (recommended)

```bash
mssql-dump dump -s localhost -d MyDB --windows-auth -o ./export
```

## Output Questions

### What is the output directory structure?

```
output/
├── tables/           # CREATE TABLE scripts
├── data/             # INSERT statements
├── views/            # CREATE VIEW scripts
├── procedures/       # CREATE PROCEDURE scripts
├── functions/        # CREATE FUNCTION scripts
├── triggers/         # CREATE TRIGGER scripts
└── _execution_order.txt  # Script execution order
```

### What is `_execution_order.txt`?

A text file listing all exported scripts in dependency order for safe execution:

```
# Tables (in dependency order)
tables/dbo.Customers.sql
tables/dbo.Orders.sql

# Data
data/dbo.Customers.sql
data/dbo.Orders.sql

# Views
views/dbo.CustomerOrders.sql

# Procedures
procedures/dbo.uspGetOrders.sql

# Functions
functions/dbo.fnCalculateTotal.sql

# Triggers
triggers/dbo.trg_AuditLog.sql
```

### Can I execute all scripts at once?

Not directly with the exported files, but you can create a master script:

```bash
# Linux/Mac
cat output/_execution_order.txt | while read -r file; do
  if [[ ! $file =~ ^# ]]; then
    sqlcmd -S localhost -d TargetDB -i "output/$file"
  fi
done
```

```powershell
# Windows PowerShell
Get-Content output\_execution_order.txt | ForEach-Object {
  if (-not $_.StartsWith('#')) {
    sqlcmd -S localhost -d TargetDB -i "output\$_"
  }
}
```

### Are scripts idempotent?

Yes! All CREATE scripts use version-aware DROP IF EXISTS patterns:

**SQL Server 2016+**:
```sql
DROP TABLE IF EXISTS [dbo].[MyTable];
GO

CREATE TABLE [dbo].[MyTable] (...);
```

**SQL Server 2008-2014**:
```sql
IF EXISTS (SELECT * FROM sys.objects WHERE object_id = OBJECT_ID(N'[dbo].[MyTable]') AND type = 'U')
DROP TABLE [dbo].[MyTable];
GO

CREATE TABLE [dbo].[MyTable] (...);
```

## Performance Questions

### How long does an export take?

Depends on:
- Database size
- Number of objects
- Network speed (if remote)
- Batch size setting
- Whether data is included

**Examples**:
- Small DB (10 tables, 10K rows): 5-30 seconds
- Medium DB (100 tables, 1M rows): 1-5 minutes
- Large DB (500 tables, 50M rows): 10-60 minutes

### How can I speed up exports?

1. **Increase batch size** for fast networks:
   ```bash
   mssql-dump dump -s localhost -d MyDB -w --batch-size 5000 -o ./export
   ```

2. **Export schema only** if data isn't needed:
   ```bash
   mssql-dump dump -s localhost -d MyDB -w --schema-only -o ./export
   ```

3. **Filter objects** to export only what's needed:
   ```bash
   mssql-dump dump -s localhost -d MyDB -w --include "dbo\..*" -o ./export
   ```

4. **Use local connection** instead of remote when possible

5. **Run on the database server** to eliminate network latency

### The tool is using a lot of memory. Why?

The tool streams data to avoid memory issues, but large batches or very wide tables can increase memory usage.

**Solutions**:
- Reduce batch size: `--batch-size 100`
- Export tables individually with filtering
- Run on a machine with more RAM

## Troubleshooting Questions

### I get "Login failed for user"

**Causes**:
1. Wrong username/password
2. User doesn't have access to database
3. SQL Authentication not enabled
4. Windows Authentication required but not used

**Solutions**:
```bash
# Verify SQL Authentication is enabled
# Use correct credentials
mssql-dump dump -s localhost -d MyDB -u correct_user -p 'correct_password' -o ./export

# Or use Windows Authentication
mssql-dump dump -s localhost -d MyDB --windows-auth -o ./export
```

### I get "Permission denied" errors

The user needs these permissions:
- `db_datareader` role (minimum for schema + data)
- `VIEW DEFINITION` permission for programmable objects
- Or `db_owner` role (recommended for complete exports)

**Grant permissions**:
```sql
USE [MyDatabase];
GO

-- Grant read access
ALTER ROLE db_datareader ADD MEMBER [export_user];
GO

-- Grant view definition
GRANT VIEW DEFINITION TO [export_user];
GO

-- OR grant db_owner (full access)
ALTER ROLE db_owner ADD MEMBER [export_user];
GO
```

### Some tables are missing from the export

**Possible causes**:
1. Filter patterns excluding them
2. User doesn't have permissions
3. Tables are in different schema than expected

**Solutions**:
```bash
# Remove filters to see all tables
mssql-dump dump -s localhost -d MyDB -w --verbose -o ./export

# Check permissions in SQL Server
# Verify schema names
```

### Export fails with "Connection timeout"

**Solutions**:
```bash
# Increase timeout and retries
mssql-dump dump \
  -s remote-server \
  -d MyDB \
  -u sa \
  -p 'Password' \
  --timeout 300 \
  --retries 5 \
  -o ./export
```

### How do I report bugs or request features?

Open an issue on GitHub with:
- Tool version (`mssql-dump --version`)
- SQL Server version
- Command you ran
- Complete error message
- Expected vs actual behavior

## Advanced Questions

### Can I use this in CI/CD pipelines?

Yes! Example GitHub Actions workflow:

```yaml
name: Database Schema Export

on:
  schedule:
    - cron: '0 2 * * *'  # Daily at 2 AM

jobs:
  export-schema:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup .NET
        uses: actions/setup-dotnet@v3
        with:
          dotnet-version: '8.0.x'
      
      - name: Install tool
        run: dotnet tool install --global mssql-dump
      
      - name: Export schema
        run: |
          mssql-dump dump \
            --server ${{ secrets.SQL_SERVER }} \
            --database ${{ secrets.SQL_DATABASE }} \
            --user ${{ secrets.SQL_USER }} \
            --password ${{ secrets.SQL_PASSWORD }} \
            --schema-only \
            --output ./database/schema
      
      - name: Commit changes
        run: |
          git config user.name "GitHub Actions"
          git config user.email "actions@github.com"
          git add database/schema/
          git diff --cached --quiet || git commit -m "Update database schema"
          git push
```

### Can I compare schemas between environments?

Yes! Export both and use diff:

```bash
# Export dev
mssql-dump dump -s dev-server -d MyDB -w --schema-only -o ./schemas/dev

# Export production
mssql-dump dump -s prod-server -d MyDB -w --schema-only -o ./schemas/prod

# Compare
diff -r ./schemas/dev ./schemas/prod
# Or use a visual diff tool
code --diff ./schemas/dev ./schemas/prod
```

### Can I migrate to a different database engine?

The tool exports SQL Server-specific syntax, so migration requires:
1. Export with this tool
2. Modify scripts for target database (PostgreSQL, MySQL, etc.)
3. Use schema conversion tools
4. Test thoroughly

This tool is best for SQL Server to SQL Server scenarios.

## See Also

- [CLI Reference](CLI-REFERENCE.md) - Complete command-line documentation
- [Examples](EXAMPLES.md) - Comprehensive usage examples
- [Troubleshooting](TROUBLESHOOTING.md) - Detailed error resolution
