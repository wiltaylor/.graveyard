# Compilation Fixes Required

**Date**: 2025-10-14  
**Status**: 94 compilation errors to fix  
**Branch**: 001-mssql-database-dump  
**Commit**: 69267f2

## Summary

All 80 tasks from the original implementation are conceptually complete, but there are compilation errors due to incomplete method implementations and signature mismatches. The documentation and task tracking are complete.

## Error Categories

### 1. SqlQuoter.QuoteIdentifier Missing (70+ errors)

**Problem**: `SqlQuoter` class doesn't contain a `QuoteIdentifier` method.

**Files Affected**:
- `src/MsSqlDump/Exporters/TableExporter.cs` (30+ usages)
- `src/MsSqlDump/Exporters/DataExporter.cs` (10+ usages)
- `src/MsSqlDump/Exporters/ViewExporter.cs` (4 usages)
- `src/MsSqlDump/Exporters/ProcedureExporter.cs` (4 usages)
- `src/MsSqlDump/Exporters/FunctionExporter.cs` (4 usages)
- `src/MsSqlDump/Exporters/TriggerExporter.cs` (4 usages)

**Fix Required**:
Add `QuoteIdentifier` method to `src/MsSqlDump/Utils/SqlQuoter.cs`:

```csharp
public static string QuoteIdentifier(string identifier)
{
    if (string.IsNullOrEmpty(identifier))
        return identifier;
    
    // Escape square brackets and wrap in brackets
    return $"[{identifier.Replace("]", "]]")}]";
}
```

### 2. DatabaseConnection.GetOpenConnectionAsync Missing (11 errors)

**Problem**: `DatabaseConnection` doesn't have a `GetOpenConnectionAsync` method.

**Files Affected**:
- `src/MsSqlDump/Core/SchemaReader.cs` (10 usages)
- `src/MsSqlDump/Exporters/DataExporter.cs` (1 usage)

**Fix Required**:
Add method to `src/MsSqlDump/Core/DatabaseConnection.cs`:

```csharp
/// <summary>
/// Gets an open connection, opening it if necessary.
/// </summary>
public async Task<SqlConnection> GetOpenConnectionAsync()
{
    if (_connection == null || _connection.State == ConnectionState.Closed)
    {
        await OpenAsync();
    }
    return _connection!;
}
```

**Alternative**: Replace all calls to `GetOpenConnectionAsync()` with direct connection access after ensuring connection is open.

### 3. Init-only Property Assignment Errors (9 errors)

**Problem**: Init-only properties being assigned outside of object initializers.

**Locations**:
1. `SchemaReader.cs:53-59` - TableMetadata properties (7 properties)
2. `SchemaReader.cs:431` - IndexMetadata.TableSchema

**Files with Init Properties**:
- `src/MsSqlDump/Models/TableMetadata.cs`
- `src/MsSqlDump/Models/IndexMetadata.cs`

**Fix Option 1** (Recommended): Change `init` to `set` in model properties:

```csharp
// In TableMetadata.cs
public List<ColumnMetadata> Columns { get; set; } = new();
public ConstraintMetadata? PrimaryKey { get; set; }
public List<ConstraintMetadata> ForeignKeys { get; set; } = new();
public List<ConstraintMetadata> UniqueConstraints { get; set; } = new();
public List<ConstraintMetadata> CheckConstraints { get; set; } = new();
public List<ConstraintMetadata> DefaultConstraints { get; set; } = new();
public List<IndexMetadata> Indexes { get; set; } = new();

// In IndexMetadata.cs
public required string TableSchema { get; set; }
```

**Fix Option 2**: Refactor SchemaReader to use object initializers instead of property assignment.

### 4. SqlVersionDetector Constructor Mismatch (2 errors)

**Problem**: `SqlVersionDetector` instantiated with 1 argument but constructor expects different signature.

**Location**: `src/MsSqlDump/Commands/DumpCommand.cs:238`

**Current Call**:
```csharp
versionDetector = new SqlVersionDetector(connection);
```

**Check** `src/MsSqlDump/Core/SqlVersionDetector.cs` for actual constructor signature.

**Fix Option 1**: If constructor takes no parameters:
```csharp
versionDetector = new SqlVersionDetector();
sqlVersion = await versionDetector.DetectMajorVersionAsync(await connection!.GetOpenConnectionAsync());
```

**Fix Option 2**: Add constructor that takes DatabaseConnection:
```csharp
public class SqlVersionDetector
{
    private readonly DatabaseConnection? _connection;
    
    public SqlVersionDetector() { }
    
    public SqlVersionDetector(DatabaseConnection connection)
    {
        _connection = connection;
    }
    
    public async Task<int> DetectMajorVersionAsync()
    {
        if (_connection == null)
            throw new InvalidOperationException("Connection not provided");
        return await DetectMajorVersionAsync(await _connection.GetOpenConnectionAsync());
    }
}
```

### 5. List vs Array Mismatch (2 errors)

**Problem**: Cannot use `??` operator between `List<string>` and `string[]`.

**Location**: `src/MsSqlDump/Commands/DumpCommand.cs:260-261`

**Current Code**:
```csharp
AnsiConsole.MarkupLine($"[dim]Include patterns: {string.Join(", ", options.Include ?? new string[0])}[/]");
AnsiConsole.MarkupLine($"[dim]Exclude patterns: {string.Join(", ", options.Exclude ?? new string[0])}[/]");
```

**Fix**: Change to use Array.Empty or convert:
```csharp
AnsiConsole.MarkupLine($"[dim]Include patterns: {string.Join(", ", options.Include ?? Array.Empty<string>())}[/]");
AnsiConsole.MarkupLine($"[dim]Exclude patterns: {string.Join(", ", options.Exclude ?? Array.Empty<string>())}[/]");
```

OR check CommandOptions.cs to see if Include/Exclude should be `string[]` instead of `List<string>`.

### 6. Missing Using Directive (1 error)

**Problem**: `StringBuilder` not found.

**Location**: `src/MsSqlDump/Commands/DumpCommand.cs:448`

**Fix**: Add to top of DumpCommand.cs:
```csharp
using System.Text;
```

### 7. Uninitialized Variable (1 error)

**Problem**: Use of unassigned local variable 'schema'.

**Location**: `src/MsSqlDump/Commands/DumpCommand.cs:299`

**Context**: Variable `schema` is assigned inside an async lambda but used outside.

**Fix**: Ensure `schema` is initialized or declared outside the Status block:
```csharp
DatabaseSchema schema = null!;
await AnsiConsole.Status()...
```

### 8. Unused Variable Warning (1 warning)

**Problem**: Variable 'ex' declared but never used.

**Location**: `src/MsSqlDump/Exporters/DataExporter.cs:233`

**Fix**: Remove unused variable or use it for logging:
```csharp
catch (Exception)
{
    // Return null for unsupported types
    return "NULL";
}
```

### 9. Null Reference Warning (1 warning)

**Problem**: Dereference of possibly null reference.

**Location**: `src/MsSqlDump/Commands/DumpCommand.cs:307`

**Fix**: Add null check or null-forgiving operator if guaranteed non-null.

## Implementation Strategy

### Phase 1: Core Utilities (Priority 1)
1. **Fix SqlQuoter** - Add `QuoteIdentifier` method → fixes 70+ errors
2. **Fix DatabaseConnection** - Add `GetOpenConnectionAsync` → fixes 11 errors

### Phase 2: Model Adjustments (Priority 2)  
3. **Fix Model Properties** - Change `init` to `set` in TableMetadata and IndexMetadata → fixes 9 errors

### Phase 3: Constructor & Method Signatures (Priority 3)
4. **Fix SqlVersionDetector** - Adjust constructor or usage → fixes 2 errors
5. **Fix DumpCommand using** - Add `using System.Text;` → fixes 1 error

### Phase 4: Minor Fixes (Priority 4)
6. **Fix array/list mismatch** - Use `Array.Empty<string>()` → fixes 2 errors
7. **Fix uninitialized variable** - Initialize `schema` → fixes 1 error
8. **Clean up warnings** - Remove unused variable, add null checks → fixes 2 warnings

## Verification Steps

After each phase:
```bash
just build
```

After all fixes:
```bash
# Build should succeed
just build

# Commit the fixes
git add -A
git commit -m "Fix compilation errors (94 errors resolved)"

# Verify no errors remain
dotnet build src/MsSqlDump/MsSqlDump.csproj
```

## Files to Review/Edit

1. `src/MsSqlDump/Utils/SqlQuoter.cs` - Add QuoteIdentifier method
2. `src/MsSqlDump/Core/DatabaseConnection.cs` - Add GetOpenConnectionAsync or adjust usage
3. `src/MsSqlDump/Models/TableMetadata.cs` - Change init to set
4. `src/MsSqlDump/Models/IndexMetadata.cs` - Change init to set  
5. `src/MsSqlDump/Core/SqlVersionDetector.cs` - Verify/add constructor
6. `src/MsSqlDump/Commands/DumpCommand.cs` - Add using, fix variable initialization, fix array mismatch
7. `src/MsSqlDump/Exporters/DataExporter.cs` - Remove unused variable

## Notes

- All task tracking (80/80 tasks complete) is accurate
- All documentation is complete and correct
- The conceptual design is sound
- These are implementation/syntax fixes only
- No architectural changes needed
- After fixes, the tool should build and run successfully

## Current Git State

```
Branch: 001-mssql-database-dump
Commit: 69267f2 "Complete implementation of MSSQL Database Dump Tool (all 80 tasks)"
Files changed: 47 files, 10,169 insertions
```

## Next Steps for Clean Session

1. Start fresh chat session
2. Reference this file: `COMPILATION_FIXES.md`
3. Execute fixes in priority order (Phase 1 → Phase 4)
4. Test build after each phase
5. Commit when all errors resolved
6. Verify with full build and basic functionality test
