using System.Collections.Concurrent;
using System.Collections.Immutable;

namespace InterviewDemoApp.Core;

/// <summary>
/// Handles most of the business logic of this demo application.
/// </summary>
public class NumberManager
{
    /// <summary>
    /// This event is fired everytime the interval has passed and returns the current counts.
    /// </summary>
    public event EventHandler<List<KeyValuePair<ulong, ulong>>>? OnTick;

    /// <summary>
    /// Holds the counts of how many times each number has been passed in.
    /// Concurrent to handle access from timer and main thread safely.
    /// </summary>
    private readonly ConcurrentDictionary<ulong, ulong> _numberStorage = new();
    
    /// <summary>
    /// Set to true to pause and prevent OnTick event from firing.
    /// </summary>
    private bool _isHalted;
    
    /// <summary>
    /// Constructor
    /// </summary>
    /// <param name="interval">Number of seconds to wait before returning current counts.</param>
    /// <param name="timer">Timer object that will fire everytime the interval is reached.</param>
    /// <exception cref="ArgumentOutOfRangeException"></exception>
    public NumberManager(int interval, ITimer timer)
    {
        if (interval < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(interval), 
                "Expected a time interval larger than 0 seconds");
        }
        
        timer.SetInterval(interval);

        timer.OnTick += (sender, args) =>
        {
            if (!_isHalted)
            {
                OnTick?.Invoke(this, GetCounts());
            }
        };

    }

    /// <summary>
    /// Simple method to check if the number passed in is part of the first 1000 entries in the Fibonacci Series.
    /// </summary>
    /// <param name="number">Number the user has passed in. Note number is a unsigned long to prevent integer overflow.</param>
    /// <returns>True if the number is in fib sequence.</returns>
    private static bool IsFib(ulong number)
    {
        if (number is 0 or 1)
        {
            return true;
        }
        
        ulong first = 0;
        ulong second = 1;

        for (var i = 1; i <= 1000; i++)
        {
            ulong result = first + second;

            if (result == number)
            {
                return true;
            }

            if (result > number)
            {
                return false;
            }

            first = second;
            second = result;
        }

        return false;
    }

    /// <summary>
    /// Adds a number to be counted.
    /// </summary>
    /// <param name="number">Number to add to the count.</param>
    /// <returns>True if the number is part of the first 1000 entries in the Fibonacci Series.</returns>
    public bool AddNumber(ulong number)
    {
        _numberStorage.AddOrUpdate(number, 
            n => 1, 
            (k, v) => v + 1);

        return IsFib(number);
    }

    /// <summary>
    /// Stop returning values everytime the timer fires.
    /// </summary>
    public void Halt()
    {
        _isHalted = true;
    }

    /// <summary>
    /// Start returning values everytime the timer fires again.
    /// </summary>
    public void Resume()
    {
        _isHalted = false;
    }

    /// <summary>
    /// Gets the values that are currently in the count hash.
    /// This is designed for when the application quits to get the values immediately.
    /// </summary>
    /// <returns>An immutable copy of the count hashes.</returns>
    public List<KeyValuePair<ulong, ulong>> GetCounts()
    {
        return _numberStorage.ToImmutableDictionary().OrderByDescending(p => p.Value)
            .ToList();
    }
}