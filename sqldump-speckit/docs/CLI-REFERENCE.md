# CLI Reference

Complete command-line reference for the MSSQL Database Dump Tool.

## Command Structure

```bash
mssql-dump dump [options]
```

## Options

### Connection Options

#### `--server, -s` (Required)
SQL Server instance to connect to.

- **Type**: String
- **Required**: Yes
- **Example**: `localhost`, `localhost:1433`, `server.domain.com`, `server.domain.com\INSTANCE`

```bash
mssql-dump dump --server localhost --database Northwind
```

#### `--database, -d` (Required)
Database name to export.

- **Type**: String
- **Required**: Yes
- **Example**: `Northwind`, `AdventureWorks`, `MyDatabase`

```bash
mssql-dump dump --server localhost --database Northwind
```

### Authentication Options

#### `--windows-auth, -w`
Use Windows Authentication (default).

- **Type**: Flag
- **Default**: true
- **Mutually Exclusive**: With `--user` and `--password`

```bash
mssql-dump dump --server localhost --database Northwind --windows-auth
```

#### `--user, -u`
SQL Server authentication username.

- **Type**: String
- **Required**: When not using Windows Authentication
- **Example**: `sa`, `dbuser`

```bash
mssql-dump dump --server localhost --database Northwind --user sa --password 'YourPassword'
```

#### `--password, -p`
SQL Server authentication password.

- **Type**: String
- **Required**: When not using Windows Authentication
- **Security Note**: Consider using environment variables or prompts for passwords in production

```bash
# Command line (less secure)
mssql-dump dump -s localhost -d Northwind -u sa -p 'MyPassword123'

# Better: Use environment variable
export MSSQL_PASSWORD='MyPassword123'
mssql-dump dump -s localhost -d Northwind -u sa -p "$MSSQL_PASSWORD"
```

### Output Options

#### `--output, -o`
Output directory for exported scripts.

- **Type**: String
- **Default**: `./output`
- **Example**: `./backup`, `/var/backups/db`, `C:\Backups\DB`

```bash
mssql-dump dump --server localhost --database Northwind --output ./my-export
```

#### `--schema-only`
Export only schema (tables, constraints, indexes), skip data.

- **Type**: Flag
- **Default**: false
- **Use Case**: Version control, schema comparison, lightweight backups

```bash
mssql-dump dump --server localhost --database Northwind --schema-only --output ./schema
```

### Object Type Filters

#### `--tables`
Include tables in export.

- **Type**: Flag
- **Default**: true

```bash
# Export only tables (no views, procedures, etc.)
mssql-dump dump -s localhost -d Northwind --tables --no-views --no-procedures --no-functions --no-triggers
```

#### `--no-tables`
Exclude tables from export.

- **Type**: Flag
- **Default**: false

```bash
# Export only programmable objects (no tables)
mssql-dump dump -s localhost -d Northwind --no-tables --views --procedures --functions
```

#### `--views`
Include views in export.

- **Type**: Flag
- **Default**: true

```bash
mssql-dump dump -s localhost -d Northwind --views
```

#### `--no-views`
Exclude views from export.

- **Type**: Flag
- **Default**: false

#### `--procedures`
Include stored procedures in export.

- **Type**: Flag
- **Default**: true

```bash
mssql-dump dump -s localhost -d Northwind --procedures
```

#### `--no-procedures`
Exclude stored procedures from export.

- **Type**: Flag
- **Default**: false

#### `--functions`
Include user-defined functions in export.

- **Type**: Flag
- **Default**: true

#### `--no-functions`
Exclude functions from export.

- **Type**: Flag
- **Default**: false

#### `--triggers`
Include triggers in export.

- **Type**: Flag
- **Default**: true

#### `--no-triggers`
Exclude triggers from export.

- **Type**: Flag
- **Default**: false

### Pattern Filtering

#### `--include`
Regex patterns for objects to include (can be specified multiple times).

- **Type**: String[]
- **Format**: Regex pattern matching `[schema].[object]` format
- **Example**: `dbo\..*`, `.*Customer.*`, `sales\..*`

```bash
# Include only dbo schema objects
mssql-dump dump -s localhost -d Northwind --include "dbo\..*"

# Include multiple patterns
mssql-dump dump -s localhost -d Northwind \
  --include "dbo\.Customer.*" \
  --include "dbo\.Order.*"

# Include objects containing "Report"
mssql-dump dump -s localhost -d Northwind --include ".*Report.*"
```

#### `--exclude`
Regex patterns for objects to exclude (can be specified multiple times).

- **Type**: String[]
- **Format**: Regex pattern matching `[schema].[object]` format
- **Example**: `.*Temp.*`, `.*\_old`, `test\..*`

```bash
# Exclude temporary tables
mssql-dump dump -s localhost -d Northwind --exclude ".*Temp.*"

# Exclude multiple patterns
mssql-dump dump -s localhost -d Northwind \
  --exclude ".*\_backup" \
  --exclude ".*\_old" \
  --exclude "test\..*"

# Combine include and exclude
mssql-dump dump -s localhost -d Northwind \
  --include "dbo\..*" \
  --exclude ".*Temp.*"
```

### Performance Options

#### `--batch-size`
Number of rows per INSERT batch for data export.

- **Type**: Integer
- **Default**: 1000
- **Range**: 1 - 100000
- **Recommendation**: 
  - Small tables: 5000-10000
  - Large tables: 500-1000
  - Wide tables: 100-500

```bash
# Small batches for wide tables or slow networks
mssql-dump dump -s localhost -d Northwind --batch-size 500

# Larger batches for better performance on fast networks
mssql-dump dump -s localhost -d Northwind --batch-size 5000
```

#### `--timeout`
Connection timeout in seconds.

- **Type**: Integer
- **Default**: 30
- **Range**: 1 - 3600

```bash
# Increase timeout for slow networks or VPNs
mssql-dump dump -s remote-server -d LargeDB --timeout 120
```

#### `--retries`
Number of connection retry attempts.

- **Type**: Integer
- **Default**: 3
- **Range**: 0 - 10

```bash
# More retries for unreliable connections
mssql-dump dump -s unstable-server -d MyDB --retries 5
```

### Diagnostic Options

#### `--verbose, -v`
Enable verbose logging output.

- **Type**: Flag
- **Default**: false
- **Output**: SQL queries executed, objects processed, timing information, file sizes

```bash
mssql-dump dump -s localhost -d Northwind --verbose
```

#### `--version`
Display tool version information.

- **Type**: Flag

```bash
mssql-dump --version
```

#### `--help, -h, -?`
Display help information.

- **Type**: Flag

```bash
mssql-dump dump --help
```

## Output Directory Structure

The tool creates an organized directory structure:

```
output/
├── tables/
│   ├── dbo.Customers.sql
│   ├── dbo.Orders.sql
│   └── dbo.OrderDetails.sql
├── data/
│   ├── dbo.Customers.sql
│   ├── dbo.Orders.sql
│   └── dbo.OrderDetails.sql
├── views/
│   ├── dbo.CustomerOrders.sql
│   └── dbo.ProductSales.sql
├── procedures/
│   ├── dbo.uspGetCustomerOrders.sql
│   └── dbo.uspUpdateInventory.sql
├── functions/
│   ├── dbo.fnCalculateTotal.sql
│   └── dbo.fnGetCustomerName.sql
├── triggers/
│   ├── dbo.trg_AuditLog.sql
│   └── dbo.trg_UpdateTimestamp.sql
└── _execution_order.txt
```

### `_execution_order.txt`

This file contains the recommended order for executing scripts to ensure dependencies are respected:

```
# Tables (in dependency order)
tables/dbo.Customers.sql
tables/dbo.Products.sql
tables/dbo.Orders.sql
tables/dbo.OrderDetails.sql

# Data (in same order as tables)
data/dbo.Customers.sql
data/dbo.Products.sql
data/dbo.Orders.sql
data/dbo.OrderDetails.sql

# Views
views/dbo.CustomerOrders.sql
views/dbo.ProductSales.sql

# Procedures
procedures/dbo.uspGetCustomerOrders.sql

# Functions
functions/dbo.fnCalculateTotal.sql

# Triggers
triggers/dbo.trg_AuditLog.sql
```

## Exit Codes

- **0**: Success
- **1**: Error (connection failure, permission denied, invalid arguments, etc.)

## Examples

See [EXAMPLES.md](EXAMPLES.md) for comprehensive usage examples.
