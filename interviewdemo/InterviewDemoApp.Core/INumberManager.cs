namespace InterviewDemoApp.Core;

public interface INumberManager
{
    /// <summary>
    /// This event is fired everytime the interval has passed and returns the current counts.
    /// </summary>
    event EventHandler<List<KeyValuePair<ulong, ulong>>>? OnTick;

    /// <summary>
    /// Adds a number to be counted.
    /// </summary>
    /// <param name="number">Number to add to the count.</param>
    /// <returns>True if the number is part of the first 1000 entries in the Fibonacci Series.</returns>
    bool AddNumber(ulong number);

    /// <summary>
    /// Stop returning values everytime the timer fires.
    /// </summary>
    void Halt();

    /// <summary>
    /// Start returning values everytime the timer fires again.
    /// </summary>
    void Resume();

    /// <summary>
    /// Gets the values that are currently in the count hash.
    /// This is designed for when the application quits to get the values immediately.
    /// </summary>
    /// <returns>An immutable copy of the count hashes.</returns>
    List<KeyValuePair<ulong, ulong>> GetCounts();
}