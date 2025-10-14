# Troubleshooting Guide

Solutions to common errors and issues with the MSSQL Database Dump Tool.

## Connection Errors

### Error: "Login failed for user"

**Full Error**:
```
Error: Login failed for user 'sa'. (Microsoft.Data.SqlClient)
```

**Causes**:
1. Incorrect username or password
2. SQL Server Authentication not enabled
3. User account is disabled or locked
4. Windows Authentication required

**Solutions**:

**Verify credentials**:
```bash
# Test with sqlcmd first
sqlcmd -S localhost -U sa -P 'YourPassword' -Q "SELECT @@VERSION"

# If that works, try the tool
mssql-dump dump -s localhost -d MyDB -u sa -p 'YourPassword' -o ./export
```

**Enable SQL Server Authentication**:
```sql
-- In SSMS, or via SQL
USE master;
GO
EXEC xp_instance_regwrite 
    N'HKEY_LOCAL_MACHINE', 
    N'Software\Microsoft\MSSQLServer\MSSQLServer',
    N'LoginMode', 
    REG_DWORD, 
    2;  -- Mixed mode
GO
-- Restart SQL Server service
```

**Try Windows Authentication**:
```bash
mssql-dump dump -s localhost -d MyDB --windows-auth -o ./export
```

### Error: "A network-related or instance-specific error occurred"

**Full Error**:
```
Error: A network-related or instance-specific error occurred while establishing a connection to SQL Server.
The server was not found or was not accessible.
```

**Causes**:
1. SQL Server not running
2. Wrong server name or port
3. Firewall blocking connection
4. SQL Browser service not running (for named instances)

**Solutions**:

**Verify SQL Server is running**:
```bash
# Windows
sc query MSSQLSERVER
# or for named instance
sc query MSSQL$INSTANCENAME

# Linux
systemctl status mssql-server
```

**Verify server name and port**:
```bash
# Test connection with sqlcmd
sqlcmd -S localhost -U sa -P 'Password' -Q "SELECT @@SERVERNAME"

# For named instance
sqlcmd -S localhost\SQLEXPRESS -U sa -P 'Password' -Q "SELECT @@SERVERNAME"

# For non-default port
sqlcmd -S localhost:1434 -U sa -P 'Password' -Q "SELECT @@SERVERNAME"
```

**Check firewall**:
```bash
# Windows: Allow SQL Server through firewall
netsh advfirewall firewall add rule name="SQL Server" dir=in action=allow protocol=TCP localport=1433

# Linux: Allow through firewalld
sudo firewall-cmd --zone=public --add-port=1433/tcp --permanent
sudo firewall-cmd --reload
```

**Start SQL Browser** (for named instances):
```bash
# Windows
sc start SQLBrowser

# Set to automatic
sc config SQLBrowser start=auto
```

### Error: "Connection Timeout Expired"

**Full Error**:
```
Error: Timeout expired. The timeout period elapsed prior to completion of the operation.
```

**Causes**:
1. Slow network connection
2. Server overloaded
3. Large database taking long to respond
4. Timeout setting too low

**Solutions**:

**Increase timeout and retries**:
```bash
mssql-dump dump \
  --server remote-server \
  --database LargeDB \
  --user sa \
  --password 'Password' \
  --timeout 300 \
  --retries 5 \
  --output ./export
```

**Test connection speed**:
```bash
# Measure query time
time sqlcmd -S remote-server -U sa -P 'Password' -Q "SELECT COUNT(*) FROM sys.tables"
```

**Run locally** if possible:
```bash
# RDP/SSH to the database server and run tool locally
mssql-dump dump -s localhost -d MyDB -w -o ./export
```

## Permission Errors

### Error: "The SELECT permission was denied"

**Full Error**:
```
Error: The SELECT permission was denied on the object 'TableName', database 'MyDB', schema 'dbo'.
```

**Cause**:
User doesn't have read permissions on database objects.

**Solution**:

Grant necessary permissions:
```sql
USE [MyDB];
GO

-- Grant data reader role
ALTER ROLE db_datareader ADD MEMBER [export_user];
GO

-- Grant view definition for metadata
GRANT VIEW DEFINITION TO [export_user];
GO

-- Verify permissions
SELECT 
    dp.name AS UserName,
    dp.type_desc AS UserType,
    o.name AS ObjectName,
    o.type_desc AS ObjectType,
    permission_name,
    state_desc
FROM sys.database_permissions p
JOIN sys.database_principals dp ON p.grantee_principal_id = dp.principal_id
LEFT JOIN sys.objects o ON p.major_id = o.object_id
WHERE dp.name = 'export_user';
```

**Quick fix** (use account with db_owner):
```bash
# Use an admin account
mssql-dump dump \
  --server localhost \
  --database MyDB \
  --user db_admin \
  --password 'AdminPassword' \
  --output ./export
```

### Error: "VIEW DEFINITION permission denied"

**Full Error**:
```
Error: The user does not have permission to perform this action on object 'ProcedureName'.
```

**Cause**:
User can't view definitions of stored procedures, functions, views, or triggers.

**Solution**:

```sql
USE [MyDB];
GO

-- Grant VIEW DEFINITION at database level
GRANT VIEW DEFINITION TO [export_user];
GO

-- Or grant db_owner role for full access
ALTER ROLE db_owner ADD MEMBER [export_user];
GO
```

## Export Errors

### Error: "Invalid object name"

**Full Error**:
```
Error: Invalid object name 'dbo.TableName'.
```

**Causes**:
1. Table doesn't exist
2. Wrong database selected
3. Schema name incorrect
4. Typo in filter pattern

**Solutions**:

**List available tables**:
```sql
-- Connect to database and list tables
SELECT SCHEMA_NAME(schema_id) + '.' + name AS FullName
FROM sys.tables
ORDER BY name;
```

**Verify database name**:
```bash
# Check current database
sqlcmd -S localhost -U sa -P 'Password' -Q "SELECT DB_NAME()"
```

**Check filter patterns**:
```bash
# Use verbose mode to see what's being filtered
mssql-dump dump \
  --server localhost \
  --database MyDB \
  --user sa \
  --password 'Password' \
  --verbose \
  --include "dbo\.TableName" \
  --output ./export
```

### Error: "String or binary data would be truncated"

**Full Error**:
```
Error: String or binary data would be truncated in table 'MyDB.dbo.MyTable', column 'ColumnName'.
```

**Cause**:
Data in source database exceeds column size defined in metadata (rare, indicates database corruption).

**Solutions**:

**Check data vs column definition**:
```sql
-- Find rows exceeding column size
SELECT *
FROM dbo.MyTable
WHERE LEN(ColumnName) > 50  -- Replace 50 with actual column size
```

**Export without problematic table**:
```bash
mssql-dump dump \
  --server localhost \
  --database MyDB \
  --windows-auth \
  --exclude "dbo\.ProblematicTable" \
  --output ./export
```

### Export is very slow

**Symptoms**:
- Export takes hours for moderate-sized database
- Progress bars stuck
- High CPU/memory usage

**Causes**:
1. Large tables with default batch size
2. Slow network connection
3. Server under load
4. Very wide tables

**Solutions**:

**Reduce batch size** for wide tables:
```bash
mssql-dump dump \
  --server localhost \
  --database MyDB \
  --windows-auth \
  --batch-size 100 \
  --output ./export
```

**Export schema only first**:
```bash
# Quick schema export
mssql-dump dump -s localhost -d MyDB -w --schema-only -o ./schema

# Then export data separately if needed
mssql-dump dump -s localhost -d MyDB -w --no-views --no-procedures --no-functions --no-triggers -o ./data
```

**Filter to specific tables**:
```bash
# Export one schema at a time
mssql-dump dump -s localhost -d MyDB -w --include "dbo\..*" -o ./dbo-export
mssql-dump dump -s localhost -d MyDB -w --include "sales\..*" -o ./sales-export
```

**Use verbose mode** to identify slow tables:
```bash
mssql-dump dump -s localhost -d MyDB -w --verbose -o ./export
```

## File System Errors

### Error: "Access to the path is denied"

**Full Error**:
```
Error: Access to the path '/output/tables/dbo.MyTable.sql' is denied.
```

**Causes**:
1. No write permission to output directory
2. File is open in another program
3. Output directory on read-only volume

**Solutions**:

**Check permissions**:
```bash
# Linux/Mac
ls -la ./output
chmod 755 ./output

# Windows
icacls .\output
```

**Use different output directory**:
```bash
# Try user home directory
mssql-dump dump -s localhost -d MyDB -w -o ~/exports/mydb

# Or temp directory
mssql-dump dump -s localhost -d MyDB -w -o /tmp/mydb-export
```

**Close files** in other programs (text editors, Git clients, etc.)

### Error: "The process cannot access the file because it is being used"

**Cause**:
Output file is open in another program (text editor, IDE, etc.)

**Solution**:

1. Close all programs that might have the file open
2. Delete existing output directory and re-export:
   ```bash
   rm -rf ./output
   mssql-dump dump -s localhost -d MyDB -w -o ./output
   ```

### Error: "Disk full"

**Full Error**:
```
Error: There is not enough space on the disk.
```

**Causes**:
1. Output drive full
2. Export larger than expected
3. Temp directory full

**Solutions**:

**Check disk space**:
```bash
# Linux/Mac
df -h .

# Windows
dir
```

**Export to different drive**:
```bash
# Use drive with more space
mssql-dump dump -s localhost -d MyDB -w -o /mnt/bigdrive/export
```

**Export schema only** to save space:
```bash
mssql-dump dump -s localhost -d MyDB -w --schema-only -o ./schema
```

**Compress exports**:
```bash
# Export and compress
mssql-dump dump -s localhost -d MyDB -w -o ./export
tar -czf export.tar.gz ./export
rm -rf ./export
```

## Data Issues

### Exported INSERT statements fail when executed

**Symptoms**:
- INSERT statements generate errors when run
- Foreign key violations
- Primary key violations

**Causes**:
1. Not following execution order
2. Target database not empty
3. Different collation

**Solutions**:

**Follow execution order**:
```bash
# Use the generated execution order file
cat ./export/_execution_order.txt
```

**Clear target database first**:
```sql
-- Drop all foreign keys first
DECLARE @SQL NVARCHAR(MAX) = '';
SELECT @SQL += 'ALTER TABLE ' + QUOTENAME(OBJECT_SCHEMA_NAME(parent_object_id)) + 
               '.' + QUOTENAME(OBJECT_NAME(parent_object_id)) + 
               ' DROP CONSTRAINT ' + QUOTENAME(name) + ';'
FROM sys.foreign_keys;
EXEC sp_executesql @SQL;

-- Drop all tables
DECLARE @SQL2 NVARCHAR(MAX) = '';
SELECT @SQL2 += 'DROP TABLE ' + QUOTENAME(SCHEMA_NAME(schema_id)) + '.' + QUOTENAME(name) + ';'
FROM sys.tables;
EXEC sp_executesql @SQL2;
```

**Check collation** matches:
```sql
-- Source database collation
SELECT DATABASEPROPERTYEX('SourceDB', 'Collation');

-- Target database collation
SELECT DATABASEPROPERTYEX('TargetDB', 'Collation');
```

### Binary data not importing correctly

**Symptom**:
Binary/varbinary columns have incorrect data after import.

**Cause**:
Binary data is exported as hex strings and must be converted during import.

**Solution**:

The tool exports binary data correctly. Ensure you're running the generated scripts as-is:

```sql
-- Generated by tool (correct)
INSERT INTO [dbo].[MyTable] ([BinaryColumn]) VALUES (0x48656C6C6F);
```

### DateTime values off by timezone

**Symptom**:
Datetime values are different after import.

**Cause**:
Tool exports in ISO format; SQL Server may interpret based on server timezone.

**Solution**:

Check SQL Server timezone settings:
```sql
-- Check timezone
EXEC sp_configure 'show advanced options', 1;
RECONFIGURE;
EXEC sp_configure 'time zone';
```

The tool exports datetime as:
```sql
'2023-10-15T14:30:00.000'
```

This should be timezone-neutral, but verify your source and target servers have matching timezone settings.

## Verbose Mode Diagnostics

### Enable verbose output for troubleshooting

```bash
mssql-dump dump \
  --server localhost \
  --database MyDB \
  --windows-auth \
  --verbose \
  --output ./export
```

**Verbose output shows**:
- Connection parameters (server, database)
- SQL Server version detected
- Include/exclude filter patterns
- Object counts found
- Dependency graph details
- Circular dependency cycles
- File sizes written
- Total export size and object count

**Example verbose output**:
```
Connecting to server: localhost, database: Northwind
SQL Server major version: 16
Reading schema for database: Northwind
Found 13 tables, 8 views, 12 procedures, 5 functions, 3 triggers
Building dependency graph for 13 tables...
Cycle: [dbo].[Employees] -> [dbo].[Orders] -> [dbo].[Employees]
Written 15234 bytes to dbo.Customers.sql
Total export size: 2456789 bytes (2399.21 KB)
Total objects exported: 41
```

## Getting Help

If you're still stuck:

1. **Check the version**:
   ```bash
   mssql-dump --version
   ```

2. **Review logs** with `--verbose`

3. **Simplify the scenario**:
   ```bash
   # Try minimal export
   mssql-dump dump -s localhost -d MyDB -w --schema-only --include "dbo\.sysdiagrams" -o ./test
   ```

4. **Test with sample database**:
   ```bash
   # Try with Northwind or AdventureWorks
   mssql-dump dump -s localhost -d Northwind -w -o ./test
   ```

5. **Check prerequisites**:
   - .NET 8 SDK installed
   - SQL Server accessible
   - Sufficient permissions
   - Disk space available

6. **Report an issue** on GitHub with:
   - Tool version
   - SQL Server version
   - Complete command
   - Complete error message
   - Verbose output if applicable

## See Also

- [CLI Reference](CLI-REFERENCE.md) - Complete command documentation
- [Examples](EXAMPLES.md) - Working examples for common scenarios
- [FAQ](FAQ.md) - Frequently asked questions
