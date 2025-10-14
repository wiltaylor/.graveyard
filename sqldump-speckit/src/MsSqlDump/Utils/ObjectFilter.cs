using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;

namespace MsSqlDump.Utils;

/// <summary>
/// Utility for filtering database objects based on regex include/exclude patterns matching [schema].[object] format.
/// </summary>
public class ObjectFilter
{
    private readonly List<Regex> _includePatterns;
    private readonly List<Regex> _excludePatterns;

    /// <summary>
    /// Initializes a new instance of the ObjectFilter class.
    /// </summary>
    /// <param name="includePatterns">Regex patterns for objects to include.</param>
    /// <param name="excludePatterns">Regex patterns for objects to exclude.</param>
    public ObjectFilter(IEnumerable<string> includePatterns, IEnumerable<string> excludePatterns)
    {
        _includePatterns = CompilePatterns(includePatterns);
        _excludePatterns = CompilePatterns(excludePatterns);
    }

    /// <summary>
    /// Compiles string patterns to regex objects
    /// </summary>
    private List<Regex> CompilePatterns(IEnumerable<string> patterns)
    {
        var compiled = new List<Regex>();
        
        foreach (var pattern in patterns)
        {
            try
            {
                compiled.Add(new Regex(pattern, RegexOptions.IgnoreCase | RegexOptions.Compiled));
            }
            catch (ArgumentException ex)
            {
                throw new ArgumentException($"Invalid regex pattern: {pattern}", ex);
            }
        }
        
        return compiled;
    }

    /// <summary>
    /// Determines if an object should be included based on filters
    /// </summary>
    /// <param name="schemaName">Schema name</param>
    /// <summary>
    /// Determines if a database object should be included based on include/exclude patterns.
    /// </summary>
    /// <param name="schemaName">Schema name of the object.</param>
    /// <param name="objectName">Name of the object.</param>
    /// <returns>True if object should be included, false otherwise.</returns>
    public bool ShouldInclude(string schemaName, string objectName)
    {
        var fullName = $"[{schemaName}].[{objectName}]";
        return ShouldInclude(fullName);
    }

    /// <summary>
    /// Determines if an object should be included based on filters
    /// </summary>
    /// <param name="fullObjectName">Fully qualified object name (e.g., [dbo].[TableName])</param>
    /// <returns>True if object should be included, false otherwise</returns>
    public bool ShouldInclude(string fullObjectName)
    {
        // If exclude patterns exist and match, exclude the object
        if (_excludePatterns.Any() && _excludePatterns.Any(p => p.IsMatch(fullObjectName)))
        {
            return false;
        }

        // If include patterns exist, only include if it matches
        if (_includePatterns.Any())
        {
            return _includePatterns.Any(p => p.IsMatch(fullObjectName));
        }

        // No include patterns, so include by default (unless excluded above)
        return true;
    }

    /// <summary>
    /// Validates that all patterns are valid regex patterns.
    /// </summary>
    /// <param name="patterns">Patterns to validate.</param>
    /// <param name="errorMessage">Error message if validation fails.</param>
    /// <returns>True if all patterns are valid, false otherwise.</returns>
    public static bool ValidatePatterns(IEnumerable<string> patterns, out string? errorMessage)
    {
        foreach (var pattern in patterns)
        {
            try
            {
                _ = new Regex(pattern);
            }
            catch (ArgumentException ex)
            {
                errorMessage = $"Invalid regex pattern '{pattern}': {ex.Message}";
                return false;
            }
        }

        errorMessage = null;
        return true;
    }

    /// <summary>
    /// Gets a value indicating whether any include or exclude patterns are configured.
    /// </summary>
    public bool HasFilters => _includePatterns.Any() || _excludePatterns.Any();

    /// <summary>
    /// Gets the count of include patterns configured.
    /// </summary>
    public int IncludePatternCount => _includePatterns.Count;
    
    /// <summary>
    /// Gets the count of exclude patterns configured.
    /// </summary>
    public int ExcludePatternCount => _excludePatterns.Count;
}
