namespace MsSqlDump.Models;

/// <summary>
/// Represents metadata for a single column in a table.
/// </summary>
public class ColumnMetadata
{
    public required string ColumnName { get; init; }
    public required int OrdinalPosition { get; init; }
    public required string DataType { get; init; }
    public int MaxLength { get; init; }
    public int Precision { get; init; }
    public int Scale { get; init; }
    public required bool IsNullable { get; init; }
    public string? DefaultValue { get; init; }
    public bool IsIdentity { get; init; }
    public long? IdentitySeed { get; init; }
    public long? IdentityIncrement { get; init; }
    public bool IsComputed { get; init; }
    public string? ComputedExpression { get; init; }
    public string? Collation { get; init; }

    /// <summary>
    /// Gets the full column definition for CREATE TABLE.
    /// </summary>
    public string GetFullDefinition()
    {
        var parts = new List<string>
        {
            $"[{ColumnName}]",
            GetDataTypeDefinition()
        };

        if (IsComputed)
        {
            parts.Add($"AS {ComputedExpression}");
        }
        else
        {
            // Identity
            if (IsIdentity && IdentitySeed.HasValue && IdentityIncrement.HasValue)
            {
                parts.Add($"IDENTITY({IdentitySeed.Value},{IdentityIncrement.Value})");
            }

            // Nullable
            parts.Add(IsNullable ? "NULL" : "NOT NULL");

            // Default
            if (!string.IsNullOrWhiteSpace(DefaultValue))
            {
                parts.Add($"DEFAULT {DefaultValue}");
            }
        }

        return string.Join(" ", parts);
    }

    /// <summary>
    /// Gets the data type definition with length/precision.
    /// </summary>
    public string GetDataTypeDefinition()
    {
        var dataType = DataType.ToLowerInvariant();

        // Handle types with max length
        if (dataType is "char" or "varchar" or "nchar" or "nvarchar" or "binary" or "varbinary")
        {
            if (MaxLength == -1)
            {
                return $"{DataType}(MAX)";
            }

            // For nvarchar and nchar, maxLength is in bytes but we need characters
            var length = dataType.StartsWith("n") ? MaxLength / 2 : MaxLength;
            return $"{DataType}({length})";
        }

        // Handle decimal/numeric types
        if (dataType is "decimal" or "numeric")
        {
            return $"{DataType}({Precision},{Scale})";
        }

        // Handle time/datetime2/datetimeoffset
        if (dataType is "time" or "datetime2" or "datetimeoffset")
        {
            return $"{DataType}({Scale})";
        }

        // All other types don't need length/precision
        return DataType;
    }
}
