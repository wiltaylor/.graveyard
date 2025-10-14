using Microsoft.Data.SqlClient;

namespace MsSqlDump.Core;

/// <summary>
/// Manages database connections with retry logic and connection lifecycle.
/// </summary>
public class DatabaseConnection : IDisposable
{
    private readonly string _connectionString;
    private readonly int _maxRetries;
    private readonly int _retryDelaySeconds;
    private SqlConnection? _connection;
    private bool _disposed;

    public DatabaseConnection(
        string server,
        string database,
        string? userId = null,
        string? password = null,
        bool useWindowsAuth = true,
        int connectionTimeout = 30,
        int maxRetries = 3,
        int retryDelaySeconds = 5)
    {
        // Validate and sanitize inputs
        server = ValidateAndSanitizeParameter(server, nameof(server), "Server cannot be empty");
        database = ValidateAndSanitizeParameter(database, nameof(database), "Database cannot be empty");
        
        if (!useWindowsAuth)
        {
            if (string.IsNullOrWhiteSpace(userId) || string.IsNullOrWhiteSpace(password))
                throw new ArgumentException("UserId and Password are required when not using Windows Authentication");
                
            // Validate credentials don't contain SQL injection attempts
            userId = ValidateAndSanitizeParameter(userId, nameof(userId), "UserId cannot be empty");
        }
        
        if (connectionTimeout <= 0)
            throw new ArgumentException("Connection timeout must be greater than 0", nameof(connectionTimeout));
        if (maxRetries < 0)
            throw new ArgumentException("Max retries cannot be negative", nameof(maxRetries));
        if (retryDelaySeconds < 0)
            throw new ArgumentException("Retry delay cannot be negative", nameof(retryDelaySeconds));

        // Use SqlConnectionStringBuilder for safe connection string construction
        // This prevents SQL injection in connection parameters
        var builder = new SqlConnectionStringBuilder
        {
            DataSource = server,
            InitialCatalog = database,
            ConnectTimeout = connectionTimeout,
            TrustServerCertificate = true // For local/dev scenarios
        };

        if (useWindowsAuth)
        {
            builder.IntegratedSecurity = true;
        }
        else
        {
            builder.UserID = userId;
            builder.Password = password;
        }

        _connectionString = builder.ConnectionString;
        _maxRetries = maxRetries;
        _retryDelaySeconds = retryDelaySeconds;
    }

    /// <summary>
    /// Validates and sanitizes connection parameters to prevent SQL injection.
    /// </summary>
    private static string ValidateAndSanitizeParameter(string value, string paramName, string errorMessage)
    {
        if (string.IsNullOrWhiteSpace(value))
            throw new ArgumentException(errorMessage, paramName);

        value = value.Trim();

        // Reject parameters containing suspicious characters that could indicate SQL injection attempts
        // These should not appear in legitimate server names, database names, or usernames
        if (value.Contains(';') || value.Contains("--") || value.Contains("/*") || value.Contains("*/") || 
            value.Contains("xp_") || value.Contains("sp_") || value.Contains("'"))
        {
            throw new ArgumentException($"{paramName} contains invalid characters that could pose a security risk", paramName);
        }


        return value;
    }

    /// <summary>
    /// Opens the database connection with retry logic.
    /// </summary>
    public async Task OpenAsync(CancellationToken cancellationToken = default)
    {
        if (_connection?.State == System.Data.ConnectionState.Open)
            return;

        int retryCount = 0;
        Exception? lastException = null;

        while (retryCount <= _maxRetries)
        {
            try
            {
                _connection = new SqlConnection(_connectionString);
                await _connection.OpenAsync(cancellationToken);
                return;
            }
            catch (SqlException ex) when (retryCount < _maxRetries)
            {
                lastException = ex;
                retryCount++;
                await Task.Delay(_retryDelaySeconds * 1000, cancellationToken);
            }
        }

        throw new InvalidOperationException(
            $"Failed to connect to database after {_maxRetries} retries", 
            lastException);
    }

    /// <summary>
    /// Gets the active SQL connection.
    /// </summary>
    public SqlConnection GetConnection()
    {
        if (_connection == null || _connection.State != System.Data.ConnectionState.Open)
            throw new InvalidOperationException("Connection is not open. Call OpenAsync first.");
        
        return _connection;
    }

    /// <summary>
    /// Gets an open connection, opening it if necessary.
    /// </summary>
    public async Task<SqlConnection> GetOpenConnectionAsync(CancellationToken cancellationToken = default)
    {
        if (_connection == null || _connection.State != System.Data.ConnectionState.Open)
        {
            await OpenAsync(cancellationToken);
        }
        return _connection!;
    }

    /// <summary>
    /// Creates a SQL command for the current connection.
    /// </summary>
    public SqlCommand CreateCommand(string commandText)
    {
        var connection = GetConnection();
        return new SqlCommand(commandText, connection);
    }

    public void Dispose()
    {
        if (_disposed)
            return;

        _connection?.Dispose();
        _disposed = true;
        GC.SuppressFinalize(this);
    }
}
