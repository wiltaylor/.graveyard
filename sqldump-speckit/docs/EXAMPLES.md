# Usage Examples

Comprehensive examples for common use cases of the MSSQL Database Dump Tool.

## Table of Contents

- [Basic Usage](#basic-usage)
- [Schema-Only Export](#schema-only-export)
- [Selective Exports](#selective-exports)
- [Pattern Filtering](#pattern-filtering)
- [Production Scenarios](#production-scenarios)
- [Troubleshooting Examples](#troubleshooting-examples)

## Basic Usage

### Export Complete Database (Windows Auth)

```bash
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --output ./backups/northwind
```

**Output**: Complete database with schema and data in `./backups/northwind/`

### Export Complete Database (SQL Auth)

```bash
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --user sa \
  --password 'YourStrong!Passw0rd' \
  --output ./backups/northwind
```

### Export Remote Database

```bash
mssql-dump dump \
  --server production-server.company.com \
  --database CustomerDB \
  --user app_user \
  --password 'SecureP@ssw0rd' \
  --timeout 120 \
  --retries 5 \
  --output ./backups/production
```

## Schema-Only Export

### Version Control (Schema Only)

```bash
mssql-dump dump \
  --server localhost \
  --database MyApp \
  --windows-auth \
  --schema-only \
  --output ./source-control/database-schema
```

**Use Case**: Commit database schema to Git for version tracking

### Schema Comparison

```bash
# Export dev schema
mssql-dump dump -s dev-server -d MyApp -w --schema-only -o ./schemas/dev

# Export production schema
mssql-dump dump -s prod-server -d MyApp -w --schema-only -o ./schemas/prod

# Compare with diff tool
diff -r ./schemas/dev ./schemas/prod
```

## Selective Exports

### Tables Only (No Programmable Objects)

```bash
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --tables \
  --no-views \
  --no-procedures \
  --no-functions \
  --no-triggers \
  --output ./tables-only
```

### Programmable Objects Only

```bash
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --no-tables \
  --views \
  --procedures \
  --functions \
  --triggers \
  --schema-only \
  --output ./programmable-objects
```

### Views and Procedures Only

```bash
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --no-tables \
  --views \
  --procedures \
  --no-functions \
  --no-triggers \
  --schema-only \
  --output ./views-and-procs
```

## Pattern Filtering

### Export Specific Schema

```bash
# Export only dbo schema
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --include "dbo\..*" \
  --output ./dbo-only
```

### Export Multiple Schemas

```bash
# Export dbo and sales schemas
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --include "dbo\..*" \
  --include "sales\..*" \
  --output ./dbo-and-sales
```

### Export Specific Tables

```bash
# Export only Customer-related tables
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --include ".*Customer.*" \
  --include ".*Order.*" \
  --output ./customer-orders
```

### Exclude Temporary Objects

```bash
# Exclude all temp/backup/old objects
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --exclude ".*Temp.*" \
  --exclude ".*\_backup" \
  --exclude ".*\_old" \
  --output ./production-objects
```

### Complex Filtering

```bash
# Include dbo schema, exclude temp and test objects
mssql-dump dump \
  --server localhost \
  --database Northwind \
  --windows-auth \
  --include "dbo\..*" \
  --exclude ".*Temp.*" \
  --exclude ".*Test.*" \
  --exclude ".*\_old" \
  --output ./filtered-export
```

## Production Scenarios

### Daily Backup Script

```bash
#!/bin/bash

# Daily backup with timestamp
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="./backups/daily/${DATE}"

mssql-dump dump \
  --server production-server \
  --database ProductionDB \
  --user backup_user \
  --password "${DB_PASSWORD}" \
  --timeout 300 \
  --retries 3 \
  --batch-size 1000 \
  --output "${BACKUP_DIR}"

# Compress backup
tar -czf "./backups/daily/${DATE}.tar.gz" "${BACKUP_DIR}"
rm -rf "${BACKUP_DIR}"

echo "Backup completed: ./backups/daily/${DATE}.tar.gz"
```

### Environment Refresh (Dev from Prod)

```bash
#!/bin/bash

# 1. Export production schema (without data)
mssql-dump dump \
  --server prod-server \
  --database ProductionDB \
  --user admin \
  --password "${PROD_PASSWORD}" \
  --schema-only \
  --output ./prod-schema

# 2. Export sanitized test data from staging
mssql-dump dump \
  --server staging-server \
  --database StagingDB \
  --user admin \
  --password "${STAGING_PASSWORD}" \
  --no-tables \
  --include "dbo\..*TestData.*" \
  --output ./test-data

# 3. Apply to dev environment
# (Execute scripts from prod-schema and test-data)
```

### Schema Migration Script

```bash
#!/bin/bash

# Export current schema for migration
mssql-dump dump \
  --server localhost \
  --database OldDatabase \
  --windows-auth \
  --schema-only \
  --include "dbo\..*" \
  --exclude ".*\_deprecated" \
  --output ./migration/schema

# Review and modify scripts as needed for target database
# Then execute against new database
```

### Audit Trail Export

```bash
# Export only audit-related objects
mssql-dump dump \
  --server localhost \
  --database MainDB \
  --windows-auth \
  --include "audit\..*" \
  --include ".*AuditLog.*" \
  --include ".*AuditTrail.*" \
  --output ./audit-export
```

## Troubleshooting Examples

### Slow Connection (Increase Timeout)

```bash
mssql-dump dump \
  --server slow-vpn-server \
  --database LargeDB \
  --user sa \
  --password 'Password' \
  --timeout 300 \
  --retries 5 \
  --batch-size 500 \
  --output ./export
```

### Large Tables (Smaller Batches)

```bash
mssql-dump dump \
  --server localhost \
  --database BigDataDB \
  --windows-auth \
  --batch-size 100 \
  --output ./large-db-export
```

### Wide Tables (Optimize Batches)

```bash
mssql-dump dump \
  --server localhost \
  --database WideColumnsDB \
  --windows-auth \
  --batch-size 250 \
  --output ./wide-tables-export
```

### Verbose Diagnostics

```bash
mssql-dump dump \
  --server localhost \
  --database MyDB \
  --windows-auth \
  --verbose \
  --output ./debug-export
```

**Output includes**:
- SQL queries executed
- Objects processed with timing
- File sizes
- Dependency analysis details
- Circular dependency warnings

### Test Connection

```bash
# Quick test with minimal export
mssql-dump dump \
  --server test-server \
  --database TestDB \
  --user test_user \
  --password 'TestPass' \
  --schema-only \
  --no-views \
  --no-procedures \
  --no-functions \
  --no-triggers \
  --include "dbo\.sysdiagrams" \
  --output ./connection-test
```

## Advanced Examples

### Multi-Database Backup Loop

```bash
#!/bin/bash

DATABASES=("DB1" "DB2" "DB3" "DB4")
SERVER="localhost"
OUTPUT_BASE="./backups/$(date +%Y%m%d)"

for DB in "${DATABASES[@]}"; do
  echo "Backing up ${DB}..."
  
  mssql-dump dump \
    --server "${SERVER}" \
    --database "${DB}" \
    --windows-auth \
    --output "${OUTPUT_BASE}/${DB}" \
    --verbose
    
  if [ $? -eq 0 ]; then
    echo "✓ ${DB} backup completed"
  else
    echo "✗ ${DB} backup failed"
  fi
done
```

### Incremental Schema Export (Git Tracked)

```bash
#!/bin/bash

# Export schema to version-controlled directory
mssql-dump dump \
  --server localhost \
  --database MyApp \
  --windows-auth \
  --schema-only \
  --output ./database/schema

# Check for changes
cd ./database
git add schema/
git diff --cached --quiet

if [ $? -eq 1 ]; then
  echo "Schema changes detected!"
  git commit -m "Schema update $(date +%Y-%m-%d)"
  git push
else
  echo "No schema changes"
fi
```

### Performance Benchmarking

```bash
#!/bin/bash

# Test different batch sizes
for BATCH in 100 500 1000 5000 10000; do
  echo "Testing batch size: ${BATCH}"
  
  time mssql-dump dump \
    --server localhost \
    --database TestDB \
    --windows-auth \
    --batch-size ${BATCH} \
    --output "./perf-test/batch-${BATCH}"
done
```

## Tips and Best Practices

### 1. Use Environment Variables for Passwords

```bash
# Set in environment
export MSSQL_PASSWORD='YourSecurePassword'

# Use in command
mssql-dump dump \
  --server localhost \
  --database MyDB \
  --user sa \
  --password "${MSSQL_PASSWORD}" \
  --output ./export
```

### 2. Schema-Only for Version Control

Always export schema-only for Git repositories:

```bash
mssql-dump dump -s localhost -d MyApp -w --schema-only -o ./db/schema
```

### 3. Separate Data and Schema Exports

```bash
# Schema
mssql-dump dump -s localhost -d MyDB -w --schema-only -o ./schema

# Data
mssql-dump dump -s localhost -d MyDB -w --no-tables -o ./data
```

### 4. Test Restore Process

Always verify exports can be restored:

```bash
# Export
mssql-dump dump -s localhost -d MyDB -w -o ./export

# Test restore (to test database)
sqlcmd -S localhost -d MyDB_Test -i ./export/_execution_order.txt
```

### 5. Use Filters for Large Databases

Don't export everything if you only need specific objects:

```bash
# Only export application tables and procedures
mssql-dump dump \
  --server localhost \
  --database LargeDB \
  --windows-auth \
  --include "app\..*" \
  --exclude ".*\_log" \
  --output ./app-only
```

## Next Steps

- See [CLI-REFERENCE.md](CLI-REFERENCE.md) for complete option documentation
- See [FAQ.md](FAQ.md) for common questions
- See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for error resolution
