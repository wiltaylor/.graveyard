# CLI Interface Contract: MSSQL Database Dump Tool

**Version**: 1.0  
**Date**: 2025-10-14  
**Tool Name**: `mssqldump`

## Overview

The tool provides a command-line interface for exporting MSSQL Server databases to SQL scripts. The interface follows System.CommandLine conventions with clear help text, validation, and error messages.

## Command Structure

```
mssqldump [command] [options]
```

## Commands

### `dump` (Main Command)

Export a MSSQL database to SQL scripts.

**Syntax**:
```bash
mssqldump dump --server <server> --database <database> [options]
```

**Required Options**:

| Option | Alias | Type | Description |
|--------|-------|------|-------------|
| `--server` | `-s` | string | SQL Server hostname or IP address |
| `--database` | `-d` | string | Name of the database to export |

**Authentication Options** (one required):

| Option | Alias | Type | Description |
|--------|-------|------|-------------|
| `--windows-auth` | `-w` | flag | Use Windows Authentication (default on Windows) |
| `--user` | `-u` | string | SQL Server authentication username |
| `--password` | `-p` | string | SQL Server authentication password |

**Output Options**:

| Option | Alias | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--output` | `-o` | string | `./output` | Output directory path |
| `--schema-only` | | flag | false | Export schema without data |

**Object Type Filters**:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `--tables` | flag | true | Include tables |
| `--views` | flag | true | Include views |
| `--procedures` | flag | true | Include stored procedures |
| `--functions` | flag | true | Include functions |
| `--triggers` | flag | true | Include triggers |

**Pattern Filters** (Priority P4):

| Option | Type | Description |
|--------|------|-------------|
| `--include` | string[] | Include objects matching patterns (regex) |
| `--exclude` | string[] | Exclude objects matching patterns (regex) |

**Advanced Options**:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `--batch-size` | int | 1000 | Rows per INSERT statement (data export) |
| `--timeout` | int | 30 | Connection timeout in seconds |
| `--retries` | int | 3 | Number of connection retry attempts |

---

## Examples

### Example 1: Schema-Only Export (P1)

Export only the database schema to version control:

```bash
mssqldump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --schema-only \
  --output ./schema
```

**Output**:
```
./schema/
├── tables/
│   ├── dbo.Customers.sql
│   ├── dbo.Orders.sql
│   └── ...
├── views/
│   └── dbo.CustomerOrders.sql
├── procedures/
│   └── dbo.uspGetCustomers.sql
├── functions/
│   └── dbo.fnGetOrderTotal.sql
├── triggers/
│   └── dbo.trgCustomersAudit.sql
└── _execution_order.txt
```

---

### Example 2: Full Database Backup (P2)

Export schema and data for complete backup:

```bash
mssqldump dump \
  --server sqlserver.example.com \
  --database ProductionDB \
  --user sa \
  --password 'YourStrong!Pass' \
  --output ./backup/2025-10-14
```

**Output** (includes data/ directory):
```
./backup/2025-10-14/
├── tables/
├── data/
│   ├── dbo.Customers.sql    # Contains INSERT statements
│   └── ...
├── views/
├── procedures/
├── functions/
├── triggers/
└── _execution_order.txt
```

---

### Example 3: Selective Export (P4)

Export only stored procedures matching a pattern:

```bash
mssqldump dump \
  --server localhost \
  --database MyApp \
  --windows-auth \
  --schema-only \
  --tables=false \
  --views=false \
  --functions=false \
  --triggers=false \
  --procedures \
  --include "^usp.*" \
  --output ./procedures
```

---

### Example 4: Custom Batch Size for Large Tables (P2)

Export with smaller batch size for databases with wide tables:

```bash
mssqldump dump \
  --server localhost \
  --database BigData \
  --windows-auth \
  --batch-size 500 \
  --output ./export
```

---

## Output Format

### Directory Structure

```
{output}/
├── tables/              # Table CREATE scripts
│   ├── {schema}.{table}.sql
│   └── ...
├── data/                # INSERT scripts (if not --schema-only)
│   ├── {schema}.{table}.sql
│   └── ...
├── views/               # View CREATE scripts
│   ├── {schema}.{view}.sql
│   └── ...
├── procedures/          # Stored procedure scripts
│   ├── {schema}.{procedure}.sql
│   └── ...
├── functions/           # Function scripts
│   ├── {schema}.{function}.sql
│   └── ...
├── triggers/            # Trigger scripts
│   ├── {schema}.{trigger}.sql
│   └── ...
└── _execution_order.txt # Ordered list of files to execute
```

### File Naming Convention

- **Format**: `{SchemaName}.{ObjectName}.sql`
- **Quoting**: Use filesystem-safe names (replace `[`, `]`, special chars with `_`)
- **Case**: Preserve original casing from database

### _execution_order.txt Format

```
# MSSQL Database Dump: Northwind
# Generated: 2025-10-14 10:30:00
# Server: localhost
# Database: Northwind
#
# Execute files in this order for correct dependency resolution:

tables/dbo.Customers.sql
tables/dbo.Orders.sql
tables/dbo.Order_Details.sql
data/dbo.Customers.sql
data/dbo.Orders.sql
data/dbo.Order_Details.sql
views/dbo.CustomerOrders.sql
procedures/dbo.uspGetCustomers.sql
functions/dbo.fnGetOrderTotal.sql
triggers/dbo.trgCustomersAudit.sql
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success - Export completed without errors |
| 1 | Connection error - Could not connect to SQL Server |
| 2 | Authentication error - Invalid credentials |
| 3 | Database not found - Specified database does not exist |
| 4 | Permission error - Insufficient permissions to read schema |
| 5 | I/O error - Could not write to output directory |
| 6 | Validation error - Invalid arguments or options |
| 99 | Unknown error - Unexpected exception |

---

## Progress Output

The tool provides real-time progress feedback using Spectre.Console:

```
MSSQL Database Dump Tool v1.0.0
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Server:   localhost
Database: Northwind
Output:   ./output
Mode:     Schema + Data

Connecting to SQL Server... ✓
Detecting SQL Server version... SQL Server 2019 (15.0.2000)
Reading database schema... ✓

[Tables]     ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 13/13 100%
[Data]       ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  8/13  62%
[Views]      ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  3/3  100%
[Procedures] ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  7/7  100%
[Functions]  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  4/4  100%
[Triggers]   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  2/2  100%

Export complete! ✓
Total files: 51
Output directory: ./output

To restore this database:
  1. Create an empty database
  2. Execute files listed in _execution_order.txt in order
```

---

## Error Messages

### Connection Errors

```
ERROR: Could not connect to SQL Server

Server: localhost
Database: Northwind
Error: A network-related or instance-specific error occurred while establishing a connection to SQL Server.

Suggestions:
  - Verify the server name is correct
  - Ensure SQL Server is running
  - Check firewall settings
  - Verify network connectivity
```

### Authentication Errors

```
ERROR: Authentication failed

Server: localhost
User: sa
Error: Login failed for user 'sa'.

Suggestions:
  - Verify username and password are correct
  - Check if SQL Server authentication is enabled
  - Ensure the login has access to the database
  - Try using --windows-auth if on Windows
```

### Permission Errors

```
ERROR: Insufficient permissions

Server: localhost
Database: Northwind
User: readonly_user
Error: The user does not have permission to access sys.tables.

Required Permissions:
  - VIEW DEFINITION on database
  - SELECT on system catalog views (sys.tables, sys.columns, etc.)
  - SELECT on user tables (if exporting data)

Suggestions:
  - Grant VIEW DEFINITION permission
  - Use a login with db_owner or db_datareader role
```

### I/O Errors

```
ERROR: Could not write to output directory

Output: /readonly/path
Error: Access to the path '/readonly/path' is denied.

Suggestions:
  - Verify the directory exists
  - Check write permissions on the directory
  - Ensure sufficient disk space
  - Try a different output directory with --output
```

---

## Validation Rules

### Server Name
- **Required**: Yes
- **Format**: Hostname, IP, or `hostname\instance`
- **Examples**: `localhost`, `192.168.1.100`, `SQLSERVER\INSTANCE1`

### Database Name
- **Required**: Yes
- **Format**: Valid SQL identifier (letters, numbers, underscore)
- **Max Length**: 128 characters

### Output Directory
- **Required**: No (default: `./output`)
- **Format**: Valid file system path
- **Validation**: Must be writable

### Batch Size
- **Required**: No (default: 1000)
- **Range**: 1 - 10,000
- **Recommendation**: 500-2000 for tables with many columns

### Connection Timeout
- **Required**: No (default: 30)
- **Range**: 1 - 300 seconds

### Retry Attempts
- **Required**: No (default: 3)
- **Range**: 0 - 10

---

## Help Output

```bash
$ mssqldump dump --help

Description:
  Export a MSSQL database to SQL scripts

Usage:
  mssqldump dump [options]

Options:
  -s, --server <server>        (REQUIRED) SQL Server hostname or IP
  -d, --database <database>    (REQUIRED) Database name to export
  -w, --windows-auth           Use Windows Authentication
  -u, --user <user>            SQL authentication username
  -p, --password <password>    SQL authentication password
  -o, --output <output>        Output directory [default: ./output]
  --schema-only                Export schema only (no data)
  --tables <true|false>        Include tables [default: true]
  --views <true|false>         Include views [default: true]
  --procedures <true|false>    Include procedures [default: true]
  --functions <true|false>     Include functions [default: true]
  --triggers <true|false>      Include triggers [default: true]
  --include <pattern>          Include pattern (regex, multiple allowed)
  --exclude <pattern>          Exclude pattern (regex, multiple allowed)
  --batch-size <size>          Rows per INSERT [default: 1000]
  --timeout <seconds>          Connection timeout [default: 30]
  --retries <count>            Retry attempts [default: 3]
  -?, -h, --help               Show help and usage information

Examples:
  # Schema only export
  mssqldump dump -s localhost -d Northwind -w --schema-only

  # Full backup with data
  mssqldump dump -s localhost -d MyDB -u sa -p Pass123

  # Export only procedures matching pattern
  mssqldump dump -s localhost -d MyDB -w --procedures --tables=false \
    --views=false --functions=false --triggers=false --include "^usp.*"
```

---

## Version Command

```bash
$ mssqldump --version

MSSQL Database Dump Tool v1.0.0
.NET Runtime: 8.0.0
SQL Server Client: Microsoft.Data.SqlClient 5.1.1
```

---

## Contract Summary

| Aspect | Specification |
|--------|--------------|
| Primary Command | `dump` |
| Required Args | `--server`, `--database` |
| Auth Methods | Windows Auth or SQL Auth (user/password) |
| Output | Organized directories (tables/, data/, views/, procedures/, functions/, triggers/) |
| File Naming | `{schema}.{object}.sql` |
| Execution Order | `_execution_order.txt` file |
| Progress | Spectre.Console progress bars and status |
| Exit Codes | 0-6, 99 (see table above) |
| Error Messages | Detailed with suggestions |
| Help | System.CommandLine auto-generated |

**CLI contract complete. Ready for quickstart guide.**
